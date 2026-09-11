//! Linear algebra module: checked vectors and matrices, exact and scientific
//! operations, decompositions, and solving.
//!
//! Exactness policy (also documented in `docs/methods/linear_algebra.md`):
//! - Integer/rational/decimal inputs stay exact for vector and matrix
//!   addition, subtraction, scaling, dot products, transposes, multiplication,
//!   determinants, traces, and solving (exact rational elimination).
//! - Any float64 element requires [`NumericMode::Scientific`]; the operation
//!   then uses binary64 with partial pivoting.
//! - QR, least squares, and condition estimation are scientific-only.
//!
//! Eigenvalues, SVD, power iteration, and PCA are scientific-only iterative
//! methods over binary64; exact inputs are converted explicitly in scientific
//! mode. Sparse solvers and tensor operations are future work and are not
//! registered.

mod decompositions;
mod pca;

use std::sync::Arc;

use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Module, ModuleDescriptor, Outcome,
    ParamDescriptor, SimpleFunction,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Number, NumberResult, NumericMode};
use bicmath_core::schema::{NumberKind, ValueSchema};
use bicmath_core::value::Value;

fn num_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

pub(crate) fn vector_schema() -> ValueSchema {
    ValueSchema::array(num_schema())
}

pub(crate) fn matrix_schema(square: bool) -> ValueSchema {
    ValueSchema::Matrix {
        max_elems: None,
        square,
    }
}

fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

pub(crate) fn scientific_modes() -> Vec<NumericMode> {
    vec![NumericMode::Scientific]
}

// ---------------------------------------------------------------------------
// Shared representations
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub(crate) struct Matrix {
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) data: Vec<Number>,
}

impl Matrix {
    pub(crate) fn new(rows: usize, cols: usize, data: Vec<Number>) -> Matrix {
        Matrix { rows, cols, data }
    }

    fn zero(rows: usize, cols: usize) -> Matrix {
        Matrix {
            rows,
            cols,
            data: vec![Number::integer(0); rows * cols],
        }
    }

    fn identity(n: usize) -> Matrix {
        let mut out = Matrix::zero(n, n);
        for i in 0..n {
            out.data[i * n + i] = Number::integer(1);
        }
        out
    }

    fn get(&self, i: usize, j: usize) -> &Number {
        &self.data[i * self.cols + j]
    }

    fn set(&mut self, i: usize, j: usize, value: Number) {
        self.data[i * self.cols + j] = value;
    }

    fn is_float(&self) -> bool {
        self.data.iter().any(|v| v.is_float())
    }

    pub(crate) fn element_count(&self) -> usize {
        self.rows * self.cols
    }

    pub(crate) fn to_value(&self) -> Value {
        Value::Matrix {
            rows: self.rows as u32,
            cols: self.cols as u32,
            data: self.data.iter().cloned().map(Value::Number).collect(),
        }
    }

    pub(crate) fn from_value(value: &Value, ctx: &ExecContext) -> Result<Matrix, EngineError> {
        let Value::Matrix { rows, cols, data } = value else {
            return Err(EngineError::malformed(format!(
                "expected a matrix, found {}",
                value.kind_name()
            )));
        };
        let (rows, cols) = (*rows as usize, *cols as usize);
        if rows == 0 || cols == 0 {
            return Err(EngineError::malformed("matrix dimensions must be positive"));
        }
        if rows.saturating_mul(cols) > ctx.limits.max_matrix_elements {
            return Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "matrix {rows}x{cols} exceeds the element limit of {}",
                    ctx.limits.max_matrix_elements
                ),
            ));
        }
        let mut numbers = Vec::with_capacity(data.len());
        for (index, item) in data.iter().enumerate() {
            match item {
                Value::Number(number) => numbers.push(number.clone()),
                other => {
                    return Err(EngineError::malformed(format!(
                        "matrix element {index} must be a number, found {}",
                        other.kind_name()
                    )));
                }
            }
        }
        Ok(Matrix::new(rows, cols, numbers))
    }
}

fn vector_from_value(value: &Value, ctx: &ExecContext) -> Result<Vec<Number>, EngineError> {
    let Value::Array(items) = value else {
        return Err(EngineError::malformed(format!(
            "expected a vector, found {}",
            value.kind_name()
        )));
    };
    if items.is_empty() {
        return Err(EngineError::malformed("vector must not be empty"));
    }
    if items.len() > ctx.limits.max_matrix_elements {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            "vector length exceeds the matrix element limit",
        ));
    }
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        match item {
            Value::Number(number) => out.push(number.clone()),
            other => {
                return Err(EngineError::malformed(format!(
                    "vector element {index} must be a number, found {}",
                    other.kind_name()
                )));
            }
        }
    }
    Ok(out)
}

fn vector_to_value(values: &[Number]) -> Value {
    Value::Array(values.iter().cloned().map(Value::Number).collect())
}

fn require_float_mode(values: &[Number], ctx: &ExecContext) -> Result<bool, EngineError> {
    let any_float = values.iter().any(|v| v.is_float());
    if any_float && ctx.numeric.mode != NumericMode::Scientific {
        return Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "float64 linear algebra requires scientific mode",
        ));
    }
    Ok(any_float)
}

fn outcome(value: Value, exactness: Exactness) -> Outcome {
    Outcome::new(value, exactness)
}

/// Reject non-scientific modes for the float64-only methods. Exact inputs are
/// accepted in scientific mode and converted explicitly by the caller.
pub(crate) fn require_scientific(ctx: &ExecContext, function: &str) -> Result<(), EngineError> {
    if ctx.numeric.mode != NumericMode::Scientific {
        return Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            format!(
                "{function} requires scientific mode; exact inputs are converted explicitly \
                 in that mode"
            ),
        ));
    }
    Ok(())
}

/// Convert a checked matrix to finite float64 values. The matrix element limit
/// was enforced by [`Matrix::from_value`] before this allocates.
pub(crate) fn scientific_matrix_floats(
    matrix: &Matrix,
    function: &str,
) -> Result<Vec<f64>, EngineError> {
    let mut out = Vec::with_capacity(matrix.element_count());
    for value in &matrix.data {
        let float = value.to_f64().ok_or_else(|| {
            EngineError::domain(format!("{function} input is not representable as float64"))
        })?;
        if !float.is_finite() {
            return Err(EngineError::domain(format!(
                "{function} input overflows float64"
            )));
        }
        out.push(float);
    }
    Ok(out)
}

/// Build a float64 wire value, rejecting non-finite results.
pub(crate) fn float_value(value: f64) -> Result<Value, EngineError> {
    Ok(Value::Number(Number::float(value)?))
}

/// Build an array of float64 wire values.
pub(crate) fn float_vector_value(values: &[f64]) -> Result<Value, EngineError> {
    let mut items = Vec::with_capacity(values.len());
    for value in values {
        items.push(float_value(*value)?);
    }
    Ok(Value::Array(items))
}

fn exactness_for(values: &[Number]) -> Exactness {
    if values.iter().any(|v| v.is_float()) {
        Exactness::Approximate
    } else {
        Exactness::Exact
    }
}

// ---------------------------------------------------------------------------
// Elementwise exact helpers
// ---------------------------------------------------------------------------

fn rational_of(number: &Number) -> Result<BigRational, EngineError> {
    number.as_exact_rational().ok_or_else(|| {
        EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "float64 requires scientific mode",
        )
    })
}

fn to_rationals(values: &[Number]) -> Result<Vec<BigRational>, EngineError> {
    values.iter().map(rational_of).collect()
}

fn to_f64s(values: &[Number]) -> Vec<f64> {
    values
        .iter()
        .map(|v| v.to_f64().unwrap_or(f64::NAN))
        .collect()
}

fn rational_number(value: BigRational) -> Number {
    if value.is_integer() {
        Number::Integer(value.to_integer())
    } else {
        Number::Rational(value)
    }
}

// ---------------------------------------------------------------------------
// Descriptor helper
// ---------------------------------------------------------------------------

pub(crate) struct Spec {
    pub(crate) id: &'static str,
    pub(crate) title: &'static str,
    pub(crate) summary: &'static str,
    pub(crate) description: &'static str,
    pub(crate) params: Vec<ParamDescriptor>,
    pub(crate) output: ValueSchema,
    pub(crate) output_description: &'static str,
    pub(crate) modes: Vec<NumericMode>,
    pub(crate) cost: CostClass,
    pub(crate) method: &'static str,
    pub(crate) examples: Vec<Example>,
}

pub(crate) fn descriptor(spec: Spec) -> FunctionDescriptor {
    FunctionDescriptor::new(spec.id, "linear_algebra", "1.0.0", spec.title, spec.summary)
        .with_description(spec.description)
        .with_parameters(spec.params)
        .with_output(spec.output, spec.output_description)
        .with_modes(spec.modes)
        .with_cost(spec.cost)
        .with_method_ref(spec.method)
        .with_examples(spec.examples)
}

pub(crate) fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}

pub(crate) fn example_args(
    pairs: &[(&str, serde_json::Value)],
) -> std::collections::BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| (name.to_string(), parse_value(raw.clone())))
        .collect()
}

// ---------------------------------------------------------------------------
// Vector functions
// ---------------------------------------------------------------------------

fn vector_binary(
    id: &'static str,
    title: &'static str,
    summary: &'static str,
    description: &'static str,
    example_a: serde_json::Value,
    example_b: serde_json::Value,
    example_expected: serde_json::Value,
) -> FunctionDescriptor {
    descriptor(Spec {
        id,
        title,
        summary,
        description,
        params: vec![
            ParamDescriptor::required("a", "First vector.", vector_schema()),
            ParamDescriptor::required("b", "Second vector.", vector_schema()),
        ],
        output: vector_schema(),
        output_description: "Elementwise result.",
        modes: all_modes(),
        cost: CostClass::Linear,
        method: "docs/methods/linear_algebra.md#vector-arithmetic",
        examples: vec![
            Example::new(
                "small vectors",
                example_args(&[("a", example_a), ("b", example_b)]),
            )
            .with_value(parse_value(example_expected)),
        ],
    })
}

fn vector_binary_invoke(
    args: &Args,
    ctx: &ExecContext,
    op: fn(&Number, &Number, &ExecContext) -> Result<NumberResult, EngineError>,
) -> Result<Outcome, EngineError> {
    let a = vector_from_value(args.require("a")?, ctx)?;
    let b = vector_from_value(args.require("b")?, ctx)?;
    if a.len() != b.len() {
        return Err(EngineError::malformed(format!(
            "vector length mismatch: {} vs {}",
            a.len(),
            b.len()
        )));
    }
    let mut out = Vec::with_capacity(a.len());
    let mut rounded = false;
    for (x, y) in a.iter().zip(b.iter()) {
        ctx.check()?;
        let result = op(x, y, ctx)?;
        rounded |= result.rounded;
        out.push(result.value);
    }
    let exactness = if exactness_for(&out) == Exactness::Approximate {
        Exactness::Approximate
    } else if rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    };
    Ok(outcome(vector_to_value(&out), exactness))
}

fn invoke_vector_add(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    vector_binary_invoke(args, ctx, vec_add_op)
}

fn invoke_vector_sub(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    vector_binary_invoke(args, ctx, vec_sub_op)
}

fn vec_add_op(a: &Number, b: &Number, ctx: &ExecContext) -> Result<NumberResult, EngineError> {
    a.add(b, &ctx.numeric, &ctx.limits)
}

fn vec_sub_op(a: &Number, b: &Number, ctx: &ExecContext) -> Result<NumberResult, EngineError> {
    a.sub(b, &ctx.numeric, &ctx.limits)
}

fn invoke_vector_scale(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let vector = vector_from_value(args.require("vector")?, ctx)?;
    let scalar = args.number("scalar")?.clone();
    require_float_mode(&vector, ctx)?;
    let mut out = Vec::with_capacity(vector.len());
    let mut rounded = false;
    for value in &vector {
        ctx.check()?;
        let result = value.mul(&scalar, &ctx.numeric, &ctx.limits)?;
        rounded |= result.rounded;
        out.push(result.value);
    }
    let exactness = if exactness_for(&out) == Exactness::Approximate {
        Exactness::Approximate
    } else if rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    };
    Ok(outcome(vector_to_value(&out), exactness))
}

fn invoke_vector_dot(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = vector_from_value(args.require("a")?, ctx)?;
    let b = vector_from_value(args.require("b")?, ctx)?;
    if a.len() != b.len() {
        return Err(EngineError::malformed(format!(
            "vector length mismatch: {} vs {}",
            a.len(),
            b.len()
        )));
    }
    let mut total = Number::integer(0);
    let mut rounded = false;
    for (x, y) in a.iter().zip(b.iter()) {
        ctx.check()?;
        let product = x.mul(y, &ctx.numeric, &ctx.limits)?;
        rounded |= product.rounded;
        let sum = total.add(&product.value, &ctx.numeric, &ctx.limits)?;
        rounded |= sum.rounded;
        total = sum.value;
    }
    let exactness = if total.is_float() {
        Exactness::Approximate
    } else if rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    };
    Ok(outcome(Value::Number(total), exactness))
}

fn invoke_vector_norm(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let vector = vector_from_value(args.require("vector")?, ctx)?;
    let order = args.optional_text("order")?.unwrap_or("2");
    match order {
        "1" => {
            let mut total = Number::integer(0);
            for value in &vector {
                ctx.check()?;
                total = total.add(&value.abs(), &ctx.numeric, &ctx.limits)?.value;
            }
            Ok(outcome(Value::Number(total), exactness_for(&vector)))
        }
        "inf" => {
            let mut best = vector[0].abs();
            for value in &vector[1..] {
                ctx.check()?;
                if value.abs().compare(&best)? == std::cmp::Ordering::Greater {
                    best = value.abs();
                }
            }
            Ok(outcome(Value::Number(best), exactness_for(&vector)))
        }
        "2" => {
            if vector.iter().any(|v| v.is_float()) {
                if ctx.numeric.mode != NumericMode::Scientific {
                    return Err(EngineError::new(
                        ErrorCode::UnsupportedNumericMode,
                        "float64 norm requires scientific mode",
                    ));
                }
                let floats = to_f64s(&vector);
                let mut scale = 0.0f64;
                for value in &floats {
                    scale = scale.max(value.abs());
                }
                if scale == 0.0 {
                    return Ok(outcome(Value::Number(Number::integer(0)), Exactness::Exact));
                }
                let mut sum = 0.0f64;
                for value in &floats {
                    let normalized = value / scale;
                    sum += normalized * normalized;
                }
                return Ok(outcome(
                    Value::Number(Number::float(scale * sum.sqrt())?),
                    Exactness::Approximate,
                ));
            }
            let mut total = Number::integer(0);
            for value in &vector {
                ctx.check()?;
                let square = value.mul(value, &ctx.numeric, &ctx.limits)?.value;
                total = total.add(&square, &ctx.numeric, &ctx.limits)?.value;
            }
            match &total {
                Number::Integer(integer) => {
                    let root = integer.sqrt();
                    if &root * &root == *integer {
                        return Ok(outcome(
                            Value::Number(Number::Integer(root)),
                            Exactness::Exact,
                        ));
                    }
                }
                Number::Rational(rational) => {
                    let numer = rational.numer().sqrt();
                    let denom = rational.denom().sqrt();
                    if &numer * &numer == *rational.numer() && &denom * &denom == *rational.denom()
                    {
                        return Ok(outcome(
                            Value::Number(Number::Rational(BigRational::new(numer, denom))),
                            Exactness::Exact,
                        ));
                    }
                }
                _ => {}
            }
            match ctx.numeric.mode {
                NumericMode::Exact => Err(EngineError::new(
                    ErrorCode::UnsupportedNumericMode,
                    "norm has no exact square root in the selected representation",
                )),
                _ => {
                    let value = total.to_f64().ok_or_else(|| {
                        EngineError::domain("norm is not representable as float64")
                    })?;
                    Ok(outcome(
                        Value::Number(Number::float(value.sqrt())?),
                        Exactness::Approximate,
                    ))
                }
            }
        }
        other => Err(EngineError::domain(format!(
            "unknown norm order {other:?}; expected \"1\", \"2\", or \"inf\""
        ))),
    }
}

// ---------------------------------------------------------------------------
// Matrix functions
// ---------------------------------------------------------------------------

fn matrix_elementwise(
    id: &'static str,
    title: &'static str,
    summary: &'static str,
    description: &'static str,
    example: serde_json::Value,
    expected: serde_json::Value,
) -> FunctionDescriptor {
    descriptor(Spec {
        id,
        title,
        summary,
        description,
        params: vec![
            ParamDescriptor::required("a", "First matrix.", matrix_schema(false)),
            ParamDescriptor::required("b", "Second matrix.", matrix_schema(false)),
        ],
        output: matrix_schema(false),
        output_description: "Elementwise result.",
        modes: all_modes(),
        cost: CostClass::Linear,
        method: "docs/methods/linear_algebra.md#matrix-arithmetic",
        examples: vec![
            Example::new(
                "2x2 matrices",
                example_args(&[("a", example.clone()), ("b", example)]),
            )
            .with_value(parse_value(expected)),
        ],
    })
}

fn matrix_elementwise_invoke(
    args: &Args,
    ctx: &ExecContext,
    subtract: bool,
) -> Result<Outcome, EngineError> {
    let a = Matrix::from_value(args.require("a")?, ctx)?;
    let b = Matrix::from_value(args.require("b")?, ctx)?;
    if a.rows != b.rows || a.cols != b.cols {
        return Err(EngineError::malformed(format!(
            "matrix shape mismatch: {}x{} vs {}x{}",
            a.rows, a.cols, b.rows, b.cols
        )));
    }
    require_float_mode(&a.data, ctx)?;
    require_float_mode(&b.data, ctx)?;
    let mut out = Vec::with_capacity(a.element_count());
    let mut rounded = false;
    for (x, y) in a.data.iter().zip(b.data.iter()) {
        ctx.check()?;
        let result = if subtract {
            x.sub(y, &ctx.numeric, &ctx.limits)?
        } else {
            x.add(y, &ctx.numeric, &ctx.limits)?
        };
        rounded |= result.rounded;
        out.push(result.value);
    }
    let exactness = if exactness_for(&out) == Exactness::Approximate {
        Exactness::Approximate
    } else if rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    };
    Ok(outcome(
        Matrix::new(a.rows, a.cols, out).to_value(),
        exactness,
    ))
}

fn invoke_matrix_add(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    matrix_elementwise_invoke(args, ctx, false)
}

fn invoke_matrix_sub(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    matrix_elementwise_invoke(args, ctx, true)
}

fn invoke_matrix_scale(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    let scalar = args.number("scalar")?.clone();
    require_float_mode(&matrix.data, ctx)?;
    let mut out = Vec::with_capacity(matrix.element_count());
    let mut rounded = false;
    for value in &matrix.data {
        ctx.check()?;
        let result = value.mul(&scalar, &ctx.numeric, &ctx.limits)?;
        rounded |= result.rounded;
        out.push(result.value);
    }
    let exactness = if exactness_for(&out) == Exactness::Approximate {
        Exactness::Approximate
    } else if rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    };
    Ok(outcome(
        Matrix::new(matrix.rows, matrix.cols, out).to_value(),
        exactness,
    ))
}

fn invoke_matrix_transpose(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    require_float_mode(&matrix.data, ctx)?;
    let mut out = Matrix::zero(matrix.cols, matrix.rows);
    for (i, row) in matrix.data.chunks(matrix.cols).enumerate() {
        ctx.check()?;
        for (j, value) in row.iter().enumerate() {
            out.set(j, i, value.clone());
        }
    }
    Ok(outcome(out.to_value(), exactness_for(&matrix.data)))
}

fn invoke_matrix_multiply(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = Matrix::from_value(args.require("a")?, ctx)?;
    let b = Matrix::from_value(args.require("b")?, ctx)?;
    if a.cols != b.rows {
        return Err(EngineError::malformed(format!(
            "cannot multiply {}x{} by {}x{}",
            a.rows, a.cols, b.rows, b.cols
        )));
    }
    require_float_mode(&a.data, ctx)?;
    require_float_mode(&b.data, ctx)?;
    let operations = (a.rows as u64)
        .saturating_mul(a.cols as u64)
        .saturating_mul(b.cols as u64);
    if operations > ctx.limits.max_operations {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "matrix multiplication needs {operations} operations, exceeding the limit of {}",
                ctx.limits.max_operations
            ),
        ));
    }
    let mut out = Matrix::zero(a.rows, b.cols);
    let mut rounded = false;
    for i in 0..a.rows {
        for j in 0..b.cols {
            ctx.check()?;
            let mut total = Number::integer(0);
            for k in 0..a.cols {
                let product = a.get(i, k).mul(b.get(k, j), &ctx.numeric, &ctx.limits)?;
                rounded |= product.rounded;
                let sum = total.add(&product.value, &ctx.numeric, &ctx.limits)?;
                rounded |= sum.rounded;
                total = sum.value;
            }
            out.set(i, j, total);
        }
    }
    let exactness = if exactness_for(&out.data) == Exactness::Approximate {
        Exactness::Approximate
    } else if rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    };
    Ok(outcome(out.to_value(), exactness))
}

fn invoke_matrix_vector_multiply(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    let vector = vector_from_value(args.require("vector")?, ctx)?;
    if matrix.cols != vector.len() {
        return Err(EngineError::malformed(format!(
            "cannot multiply {}x{} matrix by vector of length {}",
            matrix.rows,
            matrix.cols,
            vector.len()
        )));
    }
    require_float_mode(&matrix.data, ctx)?;
    require_float_mode(&vector, ctx)?;
    let mut out = Vec::with_capacity(matrix.rows);
    let mut rounded = false;
    for i in 0..matrix.rows {
        ctx.check()?;
        let mut total = Number::integer(0);
        for (j, element) in vector.iter().enumerate() {
            let product = matrix.get(i, j).mul(element, &ctx.numeric, &ctx.limits)?;
            rounded |= product.rounded;
            let sum = total.add(&product.value, &ctx.numeric, &ctx.limits)?;
            rounded |= sum.rounded;
            total = sum.value;
        }
        out.push(total);
    }
    let exactness = if exactness_for(&out) == Exactness::Approximate {
        Exactness::Approximate
    } else if rounded {
        Exactness::Rounded
    } else {
        Exactness::Exact
    };
    Ok(outcome(vector_to_value(&out), exactness))
}

fn invoke_trace(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    if matrix.rows != matrix.cols {
        return Err(EngineError::malformed("trace requires a square matrix"));
    }
    require_float_mode(&matrix.data, ctx)?;
    let mut total = Number::integer(0);
    for i in 0..matrix.rows {
        ctx.check()?;
        total = total
            .add(matrix.get(i, i), &ctx.numeric, &ctx.limits)?
            .value;
    }
    Ok(outcome(Value::Number(total), exactness_for(&matrix.data)))
}

fn invoke_identity(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let n = args.usize_param("n")?;
    if n == 0 {
        return Err(EngineError::domain("identity size must be positive"));
    }
    if n.saturating_mul(n) > ctx.limits.max_matrix_elements {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            "identity size exceeds the matrix element limit",
        ));
    }
    Ok(outcome(Matrix::identity(n).to_value(), Exactness::Exact))
}

fn invoke_zeros(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let rows = args.usize_param("rows")?;
    let cols = args.usize_param("cols")?;
    if rows == 0 || cols == 0 {
        return Err(EngineError::domain("matrix dimensions must be positive"));
    }
    if rows.saturating_mul(cols) > ctx.limits.max_matrix_elements {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            "matrix size exceeds the element limit",
        ));
    }
    Ok(outcome(
        Matrix::zero(rows, cols).to_value(),
        Exactness::Exact,
    ))
}

fn invoke_ones(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let rows = args.usize_param("rows")?;
    let cols = args.usize_param("cols")?;
    if rows == 0 || cols == 0 {
        return Err(EngineError::domain("matrix dimensions must be positive"));
    }
    if rows.saturating_mul(cols) > ctx.limits.max_matrix_elements {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            "matrix size exceeds the element limit",
        ));
    }
    let mut out = Matrix::zero(rows, cols);
    for value in &mut out.data {
        *value = Number::integer(1);
    }
    Ok(outcome(out.to_value(), Exactness::Exact))
}

// ---------------------------------------------------------------------------
// Exact determinant / LU / solve
// ---------------------------------------------------------------------------

fn rational_matrix(matrix: &Matrix) -> Result<Vec<BigRational>, EngineError> {
    to_rationals(&matrix.data)
}

fn determinant_exact(matrix: &Matrix) -> Result<BigRational, EngineError> {
    let n = matrix.rows;
    let mut a = rational_matrix(matrix)?;
    let mut det = BigRational::one();
    for k in 0..n {
        // Partial pivoting: choose the row with the largest absolute pivot.
        let mut pivot_row = k;
        let mut pivot_abs = a[k * n + k].abs();
        for i in (k + 1)..n {
            let candidate = a[i * n + k].abs();
            if candidate > pivot_abs {
                pivot_abs = candidate;
                pivot_row = i;
            }
        }
        if pivot_abs.is_zero() {
            return Ok(BigRational::zero());
        }
        if pivot_row != k {
            for j in 0..n {
                a.swap(k * n + j, pivot_row * n + j);
            }
            det = -det;
        }
        let pivot = a[k * n + k].clone();
        det *= &pivot;
        for i in (k + 1)..n {
            let factor = &a[i * n + k] / &pivot;
            for j in k..n {
                let value = a[i * n + j].clone() - &factor * &a[k * n + j];
                a[i * n + j] = value;
            }
        }
    }
    Ok(det)
}

struct LuResult {
    l: Matrix,
    u: Matrix,
    p: Vec<usize>,
    sign: i32,
    singular: bool,
}

fn lu_exact(matrix: &Matrix) -> Result<LuResult, EngineError> {
    let n = matrix.rows;
    let mut a = rational_matrix(matrix)?;
    let mut l = vec![BigRational::zero(); n * n];
    let mut p: Vec<usize> = (0..n).collect();
    let mut sign = 1i32;
    let mut singular = false;
    for k in 0..n {
        let mut pivot_row = k;
        let mut pivot_abs = a[k * n + k].abs();
        for i in (k + 1)..n {
            let candidate = a[i * n + k].abs();
            if candidate > pivot_abs {
                pivot_abs = candidate;
                pivot_row = i;
            }
        }
        if pivot_abs.is_zero() {
            singular = true;
            continue;
        }
        if pivot_row != k {
            for j in 0..n {
                a.swap(k * n + j, pivot_row * n + j);
            }
            l.swap(k * n, pivot_row * n);
            p.swap(k, pivot_row);
            sign = -sign;
        }
        let pivot = a[k * n + k].clone();
        for i in (k + 1)..n {
            let factor = &a[i * n + k] / &pivot;
            l[i * n + k] = factor.clone();
            for j in k..n {
                let value = a[i * n + j].clone() - &factor * &a[k * n + j];
                a[i * n + j] = value;
            }
        }
        l[k * n + k] = BigRational::one();
    }
    let mut u = Matrix::zero(n, n);
    for i in 0..n {
        for j in i..n {
            u.set(i, j, rational_number(a[i * n + j].clone()));
        }
    }
    let mut l_matrix = Matrix::zero(n, n);
    for i in 0..n {
        for j in 0..=i {
            l_matrix.set(i, j, rational_number(l[i * n + j].clone()));
        }
    }
    Ok(LuResult {
        l: l_matrix,
        u,
        p,
        sign,
        singular,
    })
}

fn solve_exact(matrix: &Matrix, rhs: &Matrix) -> Result<(Matrix, Number), EngineError> {
    let n = matrix.rows;
    let rhs_cols = rhs.cols;
    let mut a = rational_matrix(matrix)?;
    let mut b = rational_matrix(rhs)?;
    let mut pivots = Vec::new();
    for k in 0..n {
        let mut pivot_row = k;
        let mut pivot_abs = a[k * n + k].abs();
        for i in (k + 1)..n {
            let candidate = a[i * n + k].abs();
            if candidate > pivot_abs {
                pivot_abs = candidate;
                pivot_row = i;
            }
        }
        if pivot_abs.is_zero() {
            return Err(EngineError::new(
                ErrorCode::SingularMatrix,
                format!("matrix is singular at pivot column {k}"),
            )
            .with_details(serde_json::json!({"pivot_column": k})));
        }
        pivots.push(pivot_row);
        if pivot_row != k {
            for j in 0..n {
                a.swap(k * n + j, pivot_row * n + j);
            }
            for j in 0..rhs_cols {
                b.swap(k * rhs_cols + j, pivot_row * rhs_cols + j);
            }
        }
        let pivot = a[k * n + k].clone();
        for i in (k + 1)..n {
            let factor = &a[i * n + k] / &pivot;
            for j in k..n {
                let value = a[i * n + j].clone() - &factor * &a[k * n + j];
                a[i * n + j] = value;
            }
            for j in 0..rhs_cols {
                let value = b[i * rhs_cols + j].clone() - &factor * &b[k * rhs_cols + j];
                b[i * rhs_cols + j] = value;
            }
        }
    }
    // Back substitution.
    let mut x = vec![BigRational::zero(); n * rhs_cols];
    for col in 0..rhs_cols {
        for i in (0..n).rev() {
            let mut value = b[i * rhs_cols + col].clone();
            for j in (i + 1)..n {
                value -= &a[i * n + j] * &x[j * rhs_cols + col];
            }
            x[i * rhs_cols + col] = value / &a[i * n + i];
        }
    }
    let mut solution = Matrix::zero(n, rhs_cols);
    for i in 0..n {
        for j in 0..rhs_cols {
            solution.set(i, j, rational_number(x[i * rhs_cols + j].clone()));
        }
    }
    // Residual in the infinity norm.
    let mut max_residual = BigRational::zero();
    for i in 0..n {
        for col in 0..rhs_cols {
            let mut value = BigRational::zero();
            for j in 0..n {
                value += rational_of(matrix.get(i, j))? * &x[j * rhs_cols + col];
            }
            value -= rational_of(rhs.get(i, col))?;
            if value.abs() > max_residual {
                max_residual = value.abs();
            }
        }
    }
    Ok((solution, rational_number(max_residual)))
}

// ---------------------------------------------------------------------------
// Float decompositions and solving
// ---------------------------------------------------------------------------

fn float_matrix(matrix: &Matrix) -> Vec<f64> {
    to_f64s(&matrix.data)
}

pub(crate) fn matrix_from_floats(
    rows: usize,
    cols: usize,
    data: Vec<f64>,
) -> Result<Matrix, EngineError> {
    let mut out = Matrix::zero(rows, cols);
    for (index, value) in data.into_iter().enumerate() {
        out.data[index] = Number::float(value)?;
    }
    Ok(out)
}

struct LuFloat {
    l: Vec<f64>,
    u: Vec<f64>,
    p: Vec<usize>,
    sign: f64,
    singular: bool,
    min_pivot: f64,
    max_pivot: f64,
}

fn lu_float(matrix: &Matrix) -> LuFloat {
    let n = matrix.rows;
    let mut a = float_matrix(matrix);
    let mut l = vec![0.0f64; n * n];
    let mut p: Vec<usize> = (0..n).collect();
    let mut sign = 1.0f64;
    let mut singular = false;
    let mut min_pivot = f64::INFINITY;
    let mut max_pivot = 0.0f64;
    for k in 0..n {
        let mut pivot_row = k;
        let mut pivot_abs = a[k * n + k].abs();
        for i in (k + 1)..n {
            let candidate = a[i * n + k].abs();
            if candidate > pivot_abs {
                pivot_abs = candidate;
                pivot_row = i;
            }
        }
        if pivot_abs == 0.0 {
            singular = true;
            continue;
        }
        min_pivot = min_pivot.min(pivot_abs);
        max_pivot = max_pivot.max(pivot_abs);
        if pivot_row != k {
            for j in 0..n {
                a.swap(k * n + j, pivot_row * n + j);
            }
            l.swap(k * n, pivot_row * n);
            p.swap(k, pivot_row);
            sign = -sign;
        }
        let pivot = a[k * n + k];
        for i in (k + 1)..n {
            let factor = a[i * n + k] / pivot;
            l[i * n + k] = factor;
            for j in k..n {
                a[i * n + j] -= factor * a[k * n + j];
            }
        }
        l[k * n + k] = 1.0;
    }
    let mut u = vec![0.0f64; n * n];
    for i in 0..n {
        for j in i..n {
            u[i * n + j] = a[i * n + j];
        }
    }
    LuFloat {
        l,
        u,
        p,
        sign,
        singular,
        min_pivot: if min_pivot.is_finite() {
            min_pivot
        } else {
            0.0
        },
        max_pivot,
    }
}

fn solve_float(
    matrix: &Matrix,
    rhs: &Matrix,
    tolerance: f64,
) -> Result<(Matrix, f64), EngineError> {
    let n = matrix.rows;
    let cols = rhs.cols;
    let lu = lu_float(matrix);
    if lu.singular {
        return Err(EngineError::new(
            ErrorCode::SingularMatrix,
            "matrix is singular (zero pivot after partial pivoting)",
        ));
    }
    let scale = if lu.max_pivot == 0.0 {
        1.0
    } else {
        lu.max_pivot
    };
    if lu.min_pivot < tolerance * scale {
        return Err(EngineError::new(
            ErrorCode::IllConditioned,
            format!(
                "smallest pivot {:.3e} is below the conditioning tolerance relative to {:.3e}",
                lu.min_pivot, scale
            ),
        ));
    }
    let b = float_matrix(rhs);
    // Apply permutation and forward substitution L y = P b.
    let mut y = vec![0.0f64; n * cols];
    for i in 0..n {
        let src = lu.p[i];
        for col in 0..cols {
            let mut value = b[src * cols + col];
            for j in 0..i {
                value -= lu.l[i * n + j] * y[j * cols + col];
            }
            y[i * cols + col] = value;
        }
    }
    // Back substitution U x = y.
    let mut x = vec![0.0f64; n * cols];
    for i in (0..n).rev() {
        for col in 0..cols {
            let mut value = y[i * cols + col];
            for j in (i + 1)..n {
                value -= lu.u[i * n + j] * x[j * cols + col];
            }
            x[i * cols + col] = value / lu.u[i * n + i];
        }
    }
    // Residual infinity norm.
    let a = float_matrix(matrix);
    let mut max_residual = 0.0f64;
    for i in 0..n {
        for col in 0..cols {
            let mut value = 0.0f64;
            for j in 0..n {
                value += a[i * n + j] * x[j * cols + col];
            }
            value -= b[i * cols + col];
            max_residual = max_residual.max(value.abs());
        }
    }
    let solution = matrix_from_floats(n, cols, x)?;
    Ok((solution, max_residual))
}

fn invoke_solve(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let a = Matrix::from_value(args.require("a")?, ctx)?;
    if a.rows != a.cols {
        return Err(EngineError::malformed(
            "solve requires a square coefficient matrix",
        ));
    }
    let rhs = match args.require("b")? {
        Value::Matrix { .. } => Matrix::from_value(args.require("b")?, ctx)?,
        Value::Array(_) => {
            let vector = vector_from_value(args.require("b")?, ctx)?;
            if vector.len() != a.rows {
                return Err(EngineError::malformed(format!(
                    "right-hand side length {} does not match matrix size {}",
                    vector.len(),
                    a.rows
                )));
            }
            Matrix::new(vector.len(), 1, vector)
        }
        other => {
            return Err(EngineError::malformed(format!(
                "right-hand side must be a vector or matrix, found {}",
                other.kind_name()
            )));
        }
    };
    if rhs.rows != a.rows {
        return Err(EngineError::malformed("right-hand side row count mismatch"));
    }
    let tolerance = args.optional_f64("tolerance")?.unwrap_or(1e-12);
    let any_float = a.is_float() || rhs.is_float();
    if any_float {
        if ctx.numeric.mode != NumericMode::Scientific {
            return Err(EngineError::new(
                ErrorCode::UnsupportedNumericMode,
                "float64 solve requires scientific mode",
            ));
        }
        let (solution, residual) = solve_float(&a, &rhs, tolerance)?;
        let result_is_vector = matches!(args.require("b")?, Value::Array(_));
        let output = if result_is_vector {
            let column = (0..solution.rows)
                .map(|i| solution.get(i, 0).clone())
                .collect::<Vec<_>>();
            vector_to_value(&column)
        } else {
            solution.to_value()
        };
        let value = Value::record([
            ("solution", output),
            ("residual_norm", Value::Number(Number::float(residual)?)),
            ("residual_norm_kind", Value::text("infinity_norm")),
            ("method", Value::text("lu_partial_pivot")),
            ("tolerance", Value::Number(Number::float(tolerance)?)),
        ]);
        return Ok(outcome(value, Exactness::Approximate));
    }
    let (solution, residual) = solve_exact(&a, &rhs)?;
    let result_is_vector = matches!(args.require("b")?, Value::Array(_));
    let output = if result_is_vector {
        let column = (0..solution.rows)
            .map(|i| solution.get(i, 0).clone())
            .collect::<Vec<_>>();
        vector_to_value(&column)
    } else {
        solution.to_value()
    };
    let value = Value::record([
        ("solution", output),
        ("residual_norm", Value::Number(residual)),
        ("residual_norm_kind", Value::text("infinity_norm")),
        (
            "method",
            Value::text("rational_gaussian_elimination_partial_pivot"),
        ),
    ]);
    Ok(outcome(value, Exactness::Exact))
}

fn invoke_determinant(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    if matrix.rows != matrix.cols {
        return Err(EngineError::malformed(
            "determinant requires a square matrix",
        ));
    }
    if matrix.is_float() {
        if ctx.numeric.mode != NumericMode::Scientific {
            return Err(EngineError::new(
                ErrorCode::UnsupportedNumericMode,
                "float64 determinant requires scientific mode",
            ));
        }
        let lu = lu_float(&matrix);
        if lu.singular {
            return Ok(outcome(
                Value::Number(Number::integer(0)),
                Exactness::Approximate,
            ));
        }
        let mut det = lu.sign;
        for i in 0..matrix.rows {
            det *= lu.u[i * matrix.rows + i];
        }
        return Ok(outcome(
            Value::Number(Number::float(det)?),
            Exactness::Approximate,
        ));
    }
    let det = determinant_exact(&matrix)?;
    Ok(outcome(
        Value::Number(rational_number(det)),
        Exactness::Exact,
    ))
}

fn invoke_lu(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    if matrix.rows != matrix.cols {
        return Err(EngineError::malformed("lu requires a square matrix"));
    }
    if matrix.is_float() {
        if ctx.numeric.mode != NumericMode::Scientific {
            return Err(EngineError::new(
                ErrorCode::UnsupportedNumericMode,
                "float64 LU requires scientific mode",
            ));
        }
        let lu = lu_float(&matrix);
        if lu.singular {
            return Err(EngineError::new(
                ErrorCode::SingularMatrix,
                "matrix is singular; no LU factorization with non-zero pivots",
            ));
        }
        let l = matrix_from_floats(matrix.rows, matrix.cols, lu.l)?;
        let u = matrix_from_floats(matrix.rows, matrix.cols, lu.u)?;
        let value = Value::record([
            ("l", l.to_value()),
            ("u", u.to_value()),
            (
                "p",
                Value::Array(
                    lu.p.iter()
                        .map(|i| Value::Number(Number::integer(*i as i64)))
                        .collect(),
                ),
            ),
            ("sign", Value::Number(Number::float(lu.sign)?)),
            ("method", Value::text("lu_partial_pivot")),
        ]);
        return Ok(outcome(value, Exactness::Approximate));
    }
    let lu = lu_exact(&matrix)?;
    if lu.singular {
        return Err(EngineError::new(
            ErrorCode::SingularMatrix,
            "matrix is singular; no LU factorization with non-zero pivots",
        ));
    }
    let value = Value::record([
        ("l", lu.l.to_value()),
        ("u", lu.u.to_value()),
        (
            "p",
            Value::Array(
                lu.p.iter()
                    .map(|i| Value::Number(Number::integer(*i as i64)))
                    .collect(),
            ),
        ),
        ("sign", Value::Number(Number::integer(lu.sign as i64))),
        ("method", Value::text("rational_lu_partial_pivot")),
    ]);
    Ok(outcome(value, Exactness::Exact))
}

fn householder_qr(matrix: &Matrix) -> Result<(Matrix, Matrix), EngineError> {
    let m = matrix.rows;
    let n = matrix.cols;
    let k = m.min(n);
    let mut r = float_matrix(matrix);
    let mut q = vec![0.0f64; m * m];
    for i in 0..m {
        q[i * m + i] = 1.0;
    }
    for column in 0..k {
        let mut norm = 0.0f64;
        for i in column..m {
            norm += r[i * n + column] * r[i * n + column];
        }
        norm = norm.sqrt();
        if norm == 0.0 {
            continue;
        }
        let alpha = if r[column * n + column] >= 0.0 {
            -norm
        } else {
            norm
        };
        let mut v = vec![0.0f64; m];
        v[column] = r[column * n + column] - alpha;
        for i in (column + 1)..m {
            v[i] = r[i * n + column];
        }
        let mut v_norm_sq = 0.0f64;
        for value in &v {
            v_norm_sq += value * value;
        }
        if v_norm_sq == 0.0 {
            continue;
        }
        // Apply H = I - 2 v v^T / (v^T v) to R.
        for j in column..n {
            let mut dot = 0.0f64;
            for i in column..m {
                dot += v[i] * r[i * n + j];
            }
            let factor = 2.0 * dot / v_norm_sq;
            for i in column..m {
                r[i * n + j] -= factor * v[i];
            }
        }
        // Accumulate Q = Q H.
        for i in 0..m {
            let mut dot = 0.0f64;
            for j in column..m {
                dot += q[i * m + j] * v[j];
            }
            let factor = 2.0 * dot / v_norm_sq;
            for j in column..m {
                q[i * m + j] -= factor * v[j];
            }
        }
    }
    let q_matrix = matrix_from_floats(m, m, q)?;
    // Zero the below-diagonal entries of R for numerical cleanliness.
    for i in 0..m {
        for j in 0..n {
            if i > j {
                r[i * n + j] = 0.0;
            }
        }
    }
    let r_matrix = matrix_from_floats(m, n, r)?;
    Ok((q_matrix, r_matrix))
}

fn invoke_qr(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    if ctx.numeric.mode != NumericMode::Scientific {
        return Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "QR decomposition requires scientific mode",
        ));
    }
    if !matrix.is_float() {
        return Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "QR is a scientific method and requires float64 inputs; \
             convert explicitly to float64 first",
        ));
    }
    if matrix.rows < matrix.cols {
        return Err(EngineError::domain(
            "QR here supports matrices with at least as many rows as columns",
        ));
    }
    let (q, r) = householder_qr(&matrix)?;
    let value = Value::record([
        ("q", q.to_value()),
        ("r", r.to_value()),
        ("method", Value::text("householder")),
    ]);
    Ok(outcome(value, Exactness::Approximate))
}

fn invoke_least_squares(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let matrix = Matrix::from_value(args.require("a")?, ctx)?;
    let b = match args.require("b")? {
        Value::Array(_) => {
            let vector = vector_from_value(args.require("b")?, ctx)?;
            Matrix::new(vector.len(), 1, vector)
        }
        Value::Matrix { .. } => Matrix::from_value(args.require("b")?, ctx)?,
        other => {
            return Err(EngineError::malformed(format!(
                "b must be a vector or matrix, found {}",
                other.kind_name()
            )));
        }
    };
    if ctx.numeric.mode != NumericMode::Scientific {
        return Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "least squares requires scientific mode",
        ));
    }
    if matrix.rows < matrix.cols {
        return Err(EngineError::domain(
            "least squares requires at least as many rows as columns",
        ));
    }
    if b.rows != matrix.rows {
        return Err(EngineError::malformed("b row count does not match A"));
    }
    let (q, r) = householder_qr(&matrix)?;
    let m = matrix.rows;
    let n = matrix.cols;
    let cols = b.cols;
    let qf = float_matrix(&q);
    let rf = float_matrix(&r);
    let bf = float_matrix(&b);
    // q^T b (m x cols -> n x cols)
    let mut qtb = vec![0.0f64; n * cols];
    for i in 0..n {
        for col in 0..cols {
            let mut value = 0.0f64;
            for row in 0..m {
                value += qf[row * m + i] * bf[row * cols + col];
            }
            qtb[i * cols + col] = value;
        }
    }
    // Rank check and back substitution.
    let mut rank = n;
    for i in 0..n {
        if rf[i * n + i].abs() <= 1e-12 * rf[0].abs().max(1.0) {
            rank = i;
            break;
        }
    }
    if rank < n {
        return Err(EngineError::new(
            ErrorCode::IllConditioned,
            format!("design matrix is rank deficient (rank {rank} < {n})"),
        ));
    }
    let mut x = vec![0.0f64; n * cols];
    for col in 0..cols {
        for i in (0..n).rev() {
            let mut value = qtb[i * cols + col];
            for j in (i + 1)..n {
                value -= rf[i * n + j] * x[j * cols + col];
            }
            x[i * cols + col] = value / rf[i * n + i];
        }
    }
    // Residual norm ||A x - b||_2.
    let af = float_matrix(&matrix);
    let mut residual_sq = 0.0f64;
    for row in 0..m {
        for col in 0..cols {
            let mut value = 0.0f64;
            for j in 0..n {
                value += af[row * n + j] * x[j * cols + col];
            }
            value -= bf[row * cols + col];
            residual_sq += value * value;
        }
    }
    let solution = matrix_from_floats(n, cols, x)?;
    let output_solution = if matches!(args.require("b")?, Value::Array(_)) {
        vector_to_value(
            &(0..n)
                .map(|i| solution.get(i, 0).clone())
                .collect::<Vec<_>>(),
        )
    } else {
        solution.to_value()
    };
    let value = Value::record([
        ("solution", output_solution),
        (
            "residual_norm",
            Value::Number(Number::float(residual_sq.sqrt())?),
        ),
        ("rank", Value::Number(Number::integer(rank as i64))),
        ("method", Value::text("qr_least_squares")),
    ]);
    Ok(outcome(value, Exactness::Approximate))
}

fn invoke_condition_number_estimate(
    args: &Args,
    ctx: &ExecContext,
) -> Result<Outcome, EngineError> {
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    if matrix.rows != matrix.cols {
        return Err(EngineError::malformed(
            "condition number requires a square matrix",
        ));
    }
    if ctx.numeric.mode != NumericMode::Scientific {
        return Err(EngineError::new(
            ErrorCode::UnsupportedNumericMode,
            "condition estimation requires scientific mode; exact inputs are converted              explicitly in that mode",
        ));
    }
    let n = matrix.rows;
    let a = float_matrix(&matrix);
    let mut norm_a = 0.0f64;
    for j in 0..n {
        let mut column_sum = 0.0f64;
        for i in 0..n {
            column_sum += a[i * n + j].abs();
        }
        norm_a = norm_a.max(column_sum);
    }
    if norm_a == 0.0 {
        return Err(EngineError::new(
            ErrorCode::SingularMatrix,
            "zero matrix has no finite condition number",
        ));
    }
    // Hager-style estimate: solve A x = e_j for selected j.
    let mut norm_inv = 0.0f64;
    for j in 0..n {
        let mut e = vec![0.0f64; n];
        e[j] = 1.0;
        let rhs = matrix_from_floats(n, 1, e)?;
        match solve_float(&matrix, &rhs, 1e-14) {
            Ok((solution, _)) => {
                let mut column_sum = 0.0f64;
                for i in 0..n {
                    column_sum += solution.get(i, 0).to_f64().unwrap_or(0.0).abs();
                }
                norm_inv = norm_inv.max(column_sum);
            }
            Err(error) => return Err(error),
        }
    }
    let condition = norm_a * norm_inv;
    let value = Value::record([
        ("condition_number", Value::Number(Number::float(condition)?)),
        ("norm", Value::text("1")),
        ("method", Value::text("explicit_inverse_column_sums")),
        (
            "note",
            Value::text("condition estimate; a large value means results may lose accuracy"),
        ),
    ]);
    Ok(outcome(value, Exactness::Approximate))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn module() -> Module {
    let mut functions: Vec<Arc<dyn bicmath_core::contract::Function>> = Vec::new();

    functions.push(SimpleFunction::arc(
        vector_binary(
            "linear_algebra.vector_add",
            "Vector addition",
            "Elementwise vector addition.",
            "Exact for exact inputs; float64 requires scientific mode.",
            serde_json::json!([1, 2]),
            serde_json::json!([3, 4]),
            serde_json::json!([4, 6]),
        ),
        invoke_vector_add,
    ));

    functions.push(SimpleFunction::arc(
        vector_binary(
            "linear_algebra.vector_sub",
            "Vector subtraction",
            "Elementwise vector subtraction.",
            "Exact for exact inputs; float64 requires scientific mode.",
            serde_json::json!([1, 2]),
            serde_json::json!([3, 4]),
            serde_json::json!([-2, -2]),
        ),
        invoke_vector_sub,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.vector_scale",
            title: "Vector scaling",
            summary: "Multiply every vector element by a scalar.",
            description: "Exact for exact inputs; float64 requires scientific mode.",
            params: vec![
                ParamDescriptor::required("vector", "Input vector.", vector_schema()),
                ParamDescriptor::required("scalar", "Scalar multiplier.", num_schema()),
            ],
            output: vector_schema(),
            output_description: "Scaled vector.",
            modes: all_modes(),
            cost: CostClass::Linear,
            method: "docs/methods/linear_algebra.md#vector-scale",
            examples: vec![
                Example::new(
                    "scale a vector",
                    example_args(&[
                        ("vector", serde_json::json!([1, 2, 3])),
                        ("scalar", serde_json::json!(2)),
                    ]),
                )
                .with_value(parse_value(serde_json::json!([2, 4, 6]))),
            ],
        }),
        invoke_vector_scale,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.vector_dot",
            title: "Dot product",
            summary: "Inner product of two equal-length vectors.",
            description: "Exact for exact inputs; float64 requires scientific mode.",
            params: vec![
                ParamDescriptor::required("a", "First vector.", vector_schema()),
                ParamDescriptor::required("b", "Second vector.", vector_schema()),
            ],
            output: num_schema(),
            output_description: "Scalar dot product.",
            modes: all_modes(),
            cost: CostClass::Linear,
            method: "docs/methods/linear_algebra.md#dot",
            examples: vec![
                Example::new(
                    "dot product",
                    example_args(&[
                        ("a", serde_json::json!([1, 2, 3])),
                        ("b", serde_json::json!([4, 5, 6])),
                    ]),
                )
                .with_value(parse_value(
                    serde_json::json!({"kind": "integer", "value": "32"}),
                )),
            ],
        }),
        invoke_vector_dot,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.vector_norm",
            title: "Vector norm",
            summary: "1, 2, or infinity norm of a vector.",
            description: "The 2-norm is exact when the sum of squares has an exact square root; \
                 otherwise exact mode errors, auto mode returns an approximate decimal, and \
                 scientific mode returns float64.",
            params: vec![
                ParamDescriptor::required("vector", "Input vector.", vector_schema()),
                ParamDescriptor::optional(
                    "order",
                    "Norm order: \"2\" (default), \"1\", or \"inf\".",
                    ValueSchema::Enum {
                        variants: vec!["1".into(), "2".into(), "inf".into()],
                    },
                ),
            ],
            output: num_schema(),
            output_description: "Norm value.",
            modes: all_modes(),
            cost: CostClass::Linear,
            method: "docs/methods/linear_algebra.md#norm",
            examples: vec![
                Example::new(
                    "3-4-5 triangle",
                    example_args(&[("vector", serde_json::json!([3, 4]))]),
                )
                .with_value(parse_value(
                    serde_json::json!({"kind": "integer", "value": "5"}),
                )),
            ],
        }),
        invoke_vector_norm,
    ));

    functions.push(SimpleFunction::arc(
        matrix_elementwise(
            "linear_algebra.matrix_add",
            "Matrix addition",
            "Elementwise matrix addition.",
            "Exact for exact inputs; float64 requires scientific mode.",
            serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}),
            serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [2, 4, 6, 8]}),
        ),
        invoke_matrix_add,
    ));

    functions.push(SimpleFunction::arc(
        matrix_elementwise(
            "linear_algebra.matrix_sub",
            "Matrix subtraction",
            "Elementwise matrix subtraction.",
            "Exact for exact inputs; float64 requires scientific mode.",
            serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}),
            serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [0, 0, 0, 0]}),
        ),
        invoke_matrix_sub,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.matrix_scale",
            title: "Matrix scaling",
            summary: "Multiply every matrix element by a scalar.",
            description: "Exact for exact inputs; float64 requires scientific mode.",
            params: vec![
                ParamDescriptor::required("matrix", "Input matrix.", matrix_schema(false)),
                ParamDescriptor::required("scalar", "Scalar multiplier.", num_schema()),
            ],
            output: matrix_schema(false),
            output_description: "Scaled matrix.",
            modes: all_modes(),
            cost: CostClass::Linear,
            method: "docs/methods/linear_algebra.md#matrix-scale",
            examples: vec![Example::new(
                "scale a matrix",
                example_args(&[
                    (
                        "matrix",
                        serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}),
                    ),
                    ("scalar", serde_json::json!(2)),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [2, 4, 6, 8]}),
            ))],
        }),
        invoke_matrix_scale,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.matrix_transpose",
            title: "Transpose",
            summary: "Transpose a matrix.",
            description: "Exact for exact inputs; float64 requires scientific mode.",
            params: vec![ParamDescriptor::required(
                "matrix",
                "Input matrix.",
                matrix_schema(false),
            )],
            output: matrix_schema(false),
            output_description: "Transposed matrix.",
            modes: all_modes(),
            cost: CostClass::Linear,
            method: "docs/methods/linear_algebra.md#transpose",
            examples: vec![Example::new(
                "transpose a 2x2 matrix",
                example_args(&[(
                    "matrix",
                    serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}),
                )]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 3, 2, 4]}),
            ))],
        }),
        invoke_matrix_transpose,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.matrix_multiply",
            title: "Matrix multiplication",
            summary: "Multiply two conformable matrices.",
            description: "Exact for exact inputs; float64 requires scientific mode.",
            params: vec![
                ParamDescriptor::required("a", "Left matrix.", matrix_schema(false)),
                ParamDescriptor::required("b", "Right matrix.", matrix_schema(false)),
            ],
            output: matrix_schema(false),
            output_description: "Product matrix.",
            modes: all_modes(),
            cost: CostClass::Cubic,
            method: "docs/methods/linear_algebra.md#matmul",
            examples: vec![Example::new(
                "2x2 multiplication",
                example_args(&[
                    (
                        "a",
                        serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}),
                    ),
                    (
                        "b",
                        serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [5, 6, 7, 8]}),
                    ),
                ]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [19, 22, 43, 50]}),
            ))],
        }),
        invoke_matrix_multiply,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.matrix_vector_multiply",
            title: "Matrix-vector product",
            summary: "Multiply a matrix by a column vector.",
            description: "Exact for exact inputs; float64 requires scientific mode.",
            params: vec![
                ParamDescriptor::required("matrix", "Matrix.", matrix_schema(false)),
                ParamDescriptor::required("vector", "Vector.", vector_schema()),
            ],
            output: vector_schema(),
            output_description: "Result vector.",
            modes: all_modes(),
            cost: CostClass::Quadratic,
            method: "docs/methods/linear_algebra.md#matvec",
            examples: vec![Example::new(
                "2x2 times vector",
                example_args(&[
                    (
                        "matrix",
                        serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}),
                    ),
                    ("vector", serde_json::json!([1, 1])),
                ]),
            )
            .with_value(parse_value(serde_json::json!([3, 7])))],
        }),
        invoke_matrix_vector_multiply,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.determinant",
            title: "Determinant",
            summary: "Determinant of a square matrix.",
            description:
                "Exact for integer/rational/decimal inputs using rational elimination with \
                 partial pivoting; float64 requires scientific mode and uses LU.",
            params: vec![ParamDescriptor::required(
                "matrix",
                "Square matrix.",
                matrix_schema(true),
            )],
            output: num_schema(),
            output_description: "Determinant.",
            modes: all_modes(),
            cost: CostClass::Cubic,
            method: "docs/methods/linear_algebra.md#determinant",
            examples: vec![
                Example::new(
                    "integer determinant",
                    example_args(&[(
                        "matrix",
                        serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}),
                    )]),
                )
                .with_value(parse_value(serde_json::json!({"kind": "integer", "value": "-2"}))),
                Example::new(
                    "singular determinant",
                    example_args(&[(
                        "matrix",
                        serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 2, 4]}),
                    )]),
                )
                .with_value(parse_value(serde_json::json!({"kind": "integer", "value": "0"}))),
            ],
        }),
        invoke_determinant,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.trace",
            title: "Trace",
            summary: "Sum of diagonal elements of a square matrix.",
            description: "Exact for exact inputs; float64 requires scientific mode.",
            params: vec![ParamDescriptor::required(
                "matrix",
                "Square matrix.",
                matrix_schema(true),
            )],
            output: num_schema(),
            output_description: "Trace.",
            modes: all_modes(),
            cost: CostClass::Linear,
            method: "docs/methods/linear_algebra.md#trace",
            examples: vec![Example::new(
                "trace",
                example_args(&[(
                    "matrix",
                    serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}),
                )]),
            )
            .with_value(parse_value(serde_json::json!({"kind": "integer", "value": "5"})))],
        }),
        invoke_trace,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.identity",
            title: "Identity matrix",
            summary: "n x n identity matrix.",
            description: "Exact.",
            params: vec![ParamDescriptor::required(
                "n",
                "Matrix size.",
                ValueSchema::number(NumberKind::Integer),
            )],
            output: matrix_schema(true),
            output_description: "Identity matrix.",
            modes: all_modes(),
            cost: CostClass::Quadratic,
            method: "docs/methods/linear_algebra.md#constructors",
            examples: vec![Example::new(
                "2x2 identity",
                example_args(&[("n", serde_json::json!(2))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 0, 0, 1]}),
            ))],
        }),
        invoke_identity,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.zeros",
            title: "Zero matrix",
            summary: "Matrix of exact zeros.",
            description: "Exact.",
            params: vec![
                ParamDescriptor::required("rows", "Row count.", ValueSchema::number(NumberKind::Integer)),
                ParamDescriptor::required("cols", "Column count.", ValueSchema::number(NumberKind::Integer)),
            ],
            output: matrix_schema(false),
            output_description: "Zero matrix.",
            modes: all_modes(),
            cost: CostClass::Linear,
            method: "docs/methods/linear_algebra.md#constructors",
            examples: vec![Example::new(
                "2x3 zeros",
                example_args(&[("rows", serde_json::json!(2)), ("cols", serde_json::json!(3))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "matrix", "rows": 2, "cols": 3, "data": [0, 0, 0, 0, 0, 0]}),
            ))],
        }),
        invoke_zeros,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.ones",
            title: "Ones matrix",
            summary: "Matrix of exact ones.",
            description: "Exact.",
            params: vec![
                ParamDescriptor::required(
                    "rows",
                    "Row count.",
                    ValueSchema::number(NumberKind::Integer),
                ),
                ParamDescriptor::required(
                    "cols",
                    "Column count.",
                    ValueSchema::number(NumberKind::Integer),
                ),
            ],
            output: matrix_schema(false),
            output_description: "Ones matrix.",
            modes: all_modes(),
            cost: CostClass::Linear,
            method: "docs/methods/linear_algebra.md#constructors",
            examples: vec![Example::new(
                "2x2 ones",
                example_args(&[("rows", serde_json::json!(2)), ("cols", serde_json::json!(2))]),
            )
            .with_value(parse_value(
                serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 1, 1, 1]}),
            ))],
        }),
        invoke_ones,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.lu",
            title: "LU decomposition",
            summary: "Pivoted LU factorization with partial pivoting.",
            description:
                "Returns L, U, the row permutation, and the permutation sign such that \
                 P*A = L*U. Exact rational elimination for exact inputs; float64 requires \
                 scientific mode.",
            params: vec![ParamDescriptor::required(
                "matrix",
                "Square matrix.",
                matrix_schema(true),
            )],
            output: ValueSchema::Any,
            output_description: "Record with l, u, p, sign, and method.",
            modes: all_modes(),
            cost: CostClass::Cubic,
            method: "docs/methods/linear_algebra.md#lu",
            examples: vec![Example::new(
                "LU of a 2x2 matrix",
                example_args(&[(
                    "matrix",
                    serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [2, 1, 1, 3]}),
                )]),
            )
            .with_contains("method")],
        }),
        invoke_lu,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.qr",
            title: "QR decomposition",
            summary: "Householder QR factorization (scientific mode).",
            description:
                "Returns reduced Q and R with Q orthonormal and Q*R = A. Scientific mode and \
                 float64 inputs only; exact inputs must be converted explicitly.",
            params: vec![ParamDescriptor::required(
                "matrix",
                "Matrix with rows >= cols.",
                matrix_schema(false),
            )],
            output: ValueSchema::Any,
            output_description: "Record with q, r, and method.",
            modes: scientific_modes(),
            cost: CostClass::Cubic,
            method: "docs/methods/linear_algebra.md#qr",
            examples: vec![Example::new(
                "QR requires scientific mode",
                example_args(&[(
                    "matrix",
                    serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3, 4]}),
                )]),
            )
            .with_error(ErrorCode::UnsupportedNumericMode)],
        }),
        invoke_qr,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.solve",
            title: "Solve linear system",
            summary: "Solve A x = b for vector or matrix b.",
            description:
                "Exact rational Gaussian elimination with partial pivoting for exact inputs; \
                 float64 requires scientific mode. Returns the solution, residual infinity \
                 norm, method, and pivots. Singular systems are rejected; the inverse is never \
                 formed explicitly.",
            params: vec![
                ParamDescriptor::required("a", "Square coefficient matrix.", matrix_schema(true)),
                ParamDescriptor::required(
                    "b",
                    "Right-hand side vector or matrix.",
                    ValueSchema::Any,
                ),
                ParamDescriptor::optional(
                    "tolerance",
                    "Relative pivot tolerance for the scientific path (default 1e-12).",
                    num_schema(),
                ),
            ],
            output: ValueSchema::Any,
            output_description: "Record with solution, residual_norm, method.",
            modes: all_modes(),
            cost: CostClass::Cubic,
            method: "docs/methods/linear_algebra.md#solve",
            examples: vec![
                Example::new(
                    "exact 2x2 solve",
                    example_args(&[
                        (
                            "a",
                            serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [2, 1, 1, 3]}),
                        ),
                        ("b", serde_json::json!([5, 10])),
                    ]),
                )
                .with_contains("solution"),
                Example::new(
                    "singular system",
                    example_args(&[
                        (
                            "a",
                            serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 2, 4]}),
                        ),
                        ("b", serde_json::json!([1, 2])),
                    ]),
                )
                .with_error(ErrorCode::SingularMatrix),
            ],
        }),
        invoke_solve,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.least_squares",
            title: "Least squares",
            summary: "QR-based least-squares solution (scientific mode).",
            description:
                "Solves min ||A x - b||_2 for tall matrices using Householder QR. Rank-deficient \
                 or underdetermined systems are rejected.",
            params: vec![
                ParamDescriptor::required("a", "Design matrix (rows >= cols).", matrix_schema(false)),
                ParamDescriptor::required("b", "Observation vector or matrix.", ValueSchema::Any),
            ],
            output: ValueSchema::Any,
            output_description: "Record with solution, residual_norm, rank, method.",
            modes: scientific_modes(),
            cost: CostClass::Cubic,
            method: "docs/methods/linear_algebra.md#least-squares",
            examples: vec![Example::new(
                "line fit in scientific mode",
                example_args(&[
                    (
                        "a",
                        serde_json::json!({"kind": "matrix", "rows": 3, "cols": 2, "data": [1, 0, 1, 1, 1, 2]}),
                    ),
                    ("b", serde_json::json!([1, 2, 3])),
                ]),
            )
            .with_contains("solution")],
        }),
        invoke_least_squares,
    ));

    functions.push(SimpleFunction::arc(
        descriptor(Spec {
            id: "linear_algebra.condition_number_estimate",
            title: "Condition number estimate",
            summary: "1-norm condition number of a square float64 matrix.",
            description:
                "Scientific mode only. Computes ||A||_1 * max_j ||A^{-1} e_j||_1. The result is \
                 an estimate of conditioning; a large value means a solve may lose accuracy.",
            params: vec![ParamDescriptor::required(
                "matrix",
                "Square float64 matrix.",
                matrix_schema(true),
            )],
            output: ValueSchema::Any,
            output_description: "Record with condition_number, norm, method.",
            modes: scientific_modes(),
            cost: CostClass::Cubic,
            method: "docs/methods/linear_algebra.md#condition",
            examples: vec![Example::new(
                "identity conditioning in scientific mode",
                example_args(&[(
                    "matrix",
                    serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 0, 0, 1]}),
                )]),
            )
            .with_contains("condition_number")],
        }),
        invoke_condition_number_estimate,
    ));

    decompositions::register(&mut functions);
    pca::register(&mut functions);

    let descriptor = ModuleDescriptor::new(
        "linear_algebra",
        "Linear Algebra",
        "1.0.0",
        "Checked vectors and matrices, exact and scientific operations, \
         decompositions, and solving.",
    )
    .with_capabilities(vec![
        "exact_rational_linear_algebra",
        "lu",
        "qr",
        "solve",
        "least_squares",
        "eigenvalues",
        "svd",
        "power_iteration",
        "pca",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(all_modes())
    .with_source("crates/bicmath-linear-algebra");
    Module::new(descriptor, functions)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ExecContext {
        ExecContext::conservative()
    }

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        call_with(id, raw, ctx())
    }

    fn call_with(
        id: &str,
        raw: serde_json::Value,
        ctx: ExecContext,
    ) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == id)
            .expect("function exists");
        let args_json = raw.as_object().expect("object args");
        let mut values = std::collections::BTreeMap::new();
        for (name, value) in args_json {
            let param = function
                .descriptor()
                .parameter(name)
                .expect("parameter exists");
            match param.schema.coerce(value, name, &ctx.limits, true) {
                Ok(coerced) => {
                    values.insert(name.clone(), coerced);
                }
                Err(error) => return Err(error),
            }
        }
        function.invoke(&Args::new(values), &ctx)
    }

    fn matrix(data: &[i64]) -> serde_json::Value {
        let n = (data.len() as f64).sqrt() as usize;
        serde_json::json!({"kind": "matrix", "rows": n, "cols": n, "data": data})
    }

    #[test]
    fn determinant_exact() {
        let outcome = call(
            "linear_algebra.determinant",
            serde_json::json!({"matrix": matrix(&[1, 2, 3, 4])}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        match outcome.value {
            Value::Number(Number::Integer(value)) => assert_eq!(value.to_string(), "-2"),
            other => panic!("expected integer, got {other:?}"),
        }
        let outcome = call(
            "linear_algebra.determinant",
            serde_json::json!({"matrix": matrix(&[2, 0, 0, 3])}),
        )
        .unwrap();
        match outcome.value {
            Value::Number(Number::Integer(value)) => assert_eq!(value.to_string(), "6"),
            other => panic!("expected integer, got {other:?}"),
        }
    }

    #[test]
    fn solve_exact() {
        let outcome = call(
            "linear_algebra.solve",
            serde_json::json!({"a": matrix(&[2, 1, 1, 3]), "b": [5, 10]}),
        )
        .unwrap();
        let fields = match &outcome.value {
            Value::Record(fields) => fields,
            other => panic!("expected record, got {other:?}"),
        };
        let solution = fields.get("solution").unwrap();
        let values = solution.as_array().unwrap();
        assert_eq!(values[0].as_number().unwrap().to_string(), "1");
        assert_eq!(values[1].as_number().unwrap().to_string(), "3");
        assert_eq!(
            fields
                .get("residual_norm")
                .unwrap()
                .as_number()
                .unwrap()
                .to_string(),
            "0"
        );
    }

    #[test]
    fn singular_system_is_rejected() {
        let error = call(
            "linear_algebra.solve",
            serde_json::json!({"a": matrix(&[1, 2, 2, 4]), "b": [1, 2]}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::SingularMatrix);
    }

    #[test]
    fn ragged_matrix_is_rejected() {
        let error = call(
            "linear_algebra.determinant",
            serde_json::json!({"matrix": {"kind": "matrix", "rows": 2, "cols": 2, "data": [1, 2, 3]}}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::MalformedInput);
    }

    #[test]
    fn shape_mismatch_is_rejected() {
        let error = call(
            "linear_algebra.matrix_multiply",
            serde_json::json!({
                "a": {"kind": "matrix", "rows": 2, "cols": 3, "data": [1,2,3,4,5,6]},
                "b": {"kind": "matrix", "rows": 2, "cols": 2, "data": [1,2,3,4]}
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::MalformedInput);
    }

    #[test]
    fn float_matrix_in_auto_mode_is_rejected() {
        let error = call(
            "linear_algebra.determinant",
            serde_json::json!({"matrix": {"kind": "matrix", "rows": 1, "cols": 1, "data": [
                {"kind": "float64", "value": "1.5"}
            ]}}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);
    }

    #[test]
    fn norm_three_four_five_is_exact() {
        let outcome = call(
            "linear_algebra.vector_norm",
            serde_json::json!({"vector": [3, 4]}),
        )
        .unwrap();
        match outcome.value {
            Value::Number(Number::Integer(value)) => assert_eq!(value.to_string(), "5"),
            other => panic!("expected integer, got {other:?}"),
        }
    }

    #[test]
    fn lu_reconstructs_pa() {
        let mut ctx = ctx();
        ctx.numeric.mode = NumericMode::Scientific;
        let a = serde_json::json!({"kind": "matrix", "rows": 3, "cols": 3, "data": [
            {"kind": "float64", "value": "0"},
            {"kind": "float64", "value": "2"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "2"},
            {"kind": "float64", "value": "0"},
            {"kind": "float64", "value": "3"}
        ]});
        let outcome =
            call_with("linear_algebra.lu", serde_json::json!({"matrix": a}), ctx).unwrap();
        let fields = match &outcome.value {
            Value::Record(fields) => fields,
            other => panic!("expected record, got {other:?}"),
        };
        // Rebuild P*A - L*U and check every entry is within 1e-12.
        let l = fields.get("l").unwrap();
        let u = fields.get("u").unwrap();
        let Value::Matrix { data: l_data, .. } = l else {
            panic!("l")
        };
        let Value::Matrix { data: u_data, .. } = u else {
            panic!("u")
        };
        let lf: Vec<f64> = l_data
            .iter()
            .map(|v| v.as_number().unwrap().to_f64().unwrap())
            .collect();
        let uf: Vec<f64> = u_data
            .iter()
            .map(|v| v.as_number().unwrap().to_f64().unwrap())
            .collect();
        let a_values = [0.0, 2.0, 1.0, 1.0, 1.0, 1.0, 2.0, 0.0, 3.0];
        let p: Vec<usize> = match fields.get("p").unwrap() {
            Value::Array(items) => items
                .iter()
                .map(|v| v.as_number().unwrap().to_f64().unwrap() as usize)
                .collect(),
            other => panic!("p: {other:?}"),
        };
        for i in 0..3 {
            for j in 0..3 {
                let mut lu = 0.0;
                for k in 0..3 {
                    lu += lf[i * 3 + k] * uf[k * 3 + j];
                }
                let pa = a_values[p[i] * 3 + j];
                assert!((lu - pa).abs() < 1e-12, "entry ({i},{j}): {lu} vs {pa}");
            }
        }
    }

    #[test]
    fn qr_is_orthonormal_and_reconstructs() {
        let mut ctx = ctx();
        ctx.numeric.mode = NumericMode::Scientific;
        let a = serde_json::json!({"kind": "matrix", "rows": 3, "cols": 2, "data": [
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "0"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "2"}
        ]});
        let outcome =
            call_with("linear_algebra.qr", serde_json::json!({"matrix": a}), ctx).unwrap();
        let fields = match &outcome.value {
            Value::Record(fields) => fields,
            other => panic!("expected record, got {other:?}"),
        };
        let Value::Matrix {
            data: q_data,
            rows: qr,
            cols: qc,
            ..
        } = fields.get("q").unwrap()
        else {
            panic!("q")
        };
        let Value::Matrix {
            data: r_data,
            rows: rr,
            cols: rc,
            ..
        } = fields.get("r").unwrap()
        else {
            panic!("r")
        };
        let q: Vec<f64> = q_data
            .iter()
            .map(|v| v.as_number().unwrap().to_f64().unwrap())
            .collect();
        let r: Vec<f64> = r_data
            .iter()
            .map(|v| v.as_number().unwrap().to_f64().unwrap())
            .collect();
        let (m, n) = (*qr as usize, *rc as usize);
        let _ = qc;
        let _ = rr;
        // Q^T Q = I over the first n columns (Q is m x m).
        for i in 0..n {
            for j in 0..n {
                let mut dot = 0.0;
                for k in 0..m {
                    dot += q[k * m + i] * q[k * m + j];
                }
                let expected = if i == j { 1.0 } else { 0.0 };
                assert!((dot - expected).abs() < 1e-12, "Q^T Q ({i},{j}) = {dot}");
            }
        }
        // Q R = A (reduced R is n x n).
        let a_values = [1.0, 0.0, 1.0, 1.0, 1.0, 2.0];
        for i in 0..m {
            for j in 0..n {
                let mut value = 0.0;
                for k in 0..n {
                    value += q[i * m + k] * r[k * n + j];
                }
                assert!((value - a_values[i * n + j]).abs() < 1e-12);
            }
        }
        // R upper triangular
        for i in 0..m {
            for j in 0..i.min(n) {
                assert_eq!(r[i * n + j], 0.0);
            }
        }
    }

    #[test]
    fn least_squares_fits_a_line() {
        let mut ctx = ctx();
        ctx.numeric.mode = NumericMode::Scientific;
        let a = serde_json::json!({"kind": "matrix", "rows": 3, "cols": 2, "data": [
            {"kind": "float64", "value": "0"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "2"},
            {"kind": "float64", "value": "1"}
        ]});
        let b = serde_json::json!([
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "2"},
            {"kind": "float64", "value": "3"}
        ]);
        let outcome = call_with(
            "linear_algebra.least_squares",
            serde_json::json!({"a": a, "b": b}),
            ctx,
        )
        .unwrap();
        let fields = match &outcome.value {
            Value::Record(fields) => fields,
            other => panic!("expected record, got {other:?}"),
        };
        let solution = fields.get("solution").unwrap().as_array().unwrap();
        let intercept = solution[0].as_number().unwrap().to_f64().unwrap();
        let slope = solution[1].as_number().unwrap().to_f64().unwrap();
        assert!((intercept - 1.0).abs() < 1e-12);
        assert!((slope - 1.0).abs() < 1e-12);
    }

    #[test]
    fn condition_number_of_identity_is_one() {
        let mut ctx = ctx();
        ctx.numeric.mode = NumericMode::Scientific;
        let identity = serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [
            {"kind": "float64", "value": "1"},
            {"kind": "float64", "value": "0"},
            {"kind": "float64", "value": "0"},
            {"kind": "float64", "value": "1"}
        ]});
        let outcome = call_with(
            "linear_algebra.condition_number_estimate",
            serde_json::json!({"matrix": identity}),
            ctx,
        )
        .unwrap();
        let fields = match &outcome.value {
            Value::Record(fields) => fields,
            other => panic!("expected record, got {other:?}"),
        };
        let condition = fields
            .get("condition_number")
            .unwrap()
            .as_number()
            .unwrap()
            .to_f64()
            .unwrap();
        assert!((condition - 1.0).abs() < 1e-9);
    }

    #[test]
    fn every_function_has_examples() {
        for function in module().functions {
            assert!(
                !function.descriptor().examples.is_empty(),
                "function {} has no examples",
                function.descriptor().id
            );
        }
    }

    fn scientific() -> ExecContext {
        let mut ctx = ctx();
        ctx.numeric.mode = NumericMode::Scientific;
        ctx
    }

    fn record_field<'a>(outcome: &'a Outcome, name: &str) -> &'a Value {
        match &outcome.value {
            Value::Record(fields) => fields
                .get(name)
                .unwrap_or_else(|| panic!("missing record field {name}")),
            other => panic!("expected record, got {other:?}"),
        }
    }

    fn floats(value: &Value) -> Vec<f64> {
        match value {
            Value::Array(items) => items
                .iter()
                .map(|item| item.as_number().unwrap().to_f64().unwrap())
                .collect(),
            other => panic!("expected array, got {other:?}"),
        }
    }

    fn matrix_floats(value: &Value) -> (usize, usize, Vec<f64>) {
        match value {
            Value::Matrix { rows, cols, data } => (
                *rows as usize,
                *cols as usize,
                data.iter()
                    .map(|item| item.as_number().unwrap().to_f64().unwrap())
                    .collect(),
            ),
            other => panic!("expected matrix, got {other:?}"),
        }
    }

    #[test]
    fn eigen_symmetric_2x2_has_known_values_and_orthogonal_vectors() {
        let outcome = call_with(
            "linear_algebra.eigen_symmetric",
            serde_json::json!({"matrix": matrix(&[2, 1, 1, 2])}),
            scientific(),
        )
        .unwrap();
        let values = floats(record_field(&outcome, "values"));
        assert_eq!(values.len(), 2);
        assert!((values[0] - 1.0).abs() < 1e-10, "values[0] = {}", values[0]);
        assert!((values[1] - 3.0).abs() < 1e-10, "values[1] = {}", values[1]);
        let (rows, cols, vectors) = matrix_floats(record_field(&outcome, "vectors"));
        assert_eq!((rows, cols), (2, 2));
        for column in 0..2 {
            let norm = (0..2)
                .map(|row| vectors[row * 2 + column].powi(2))
                .sum::<f64>()
                .sqrt();
            assert!((norm - 1.0).abs() < 1e-10, "column norm {norm}");
            for row in 0..2 {
                let product = 2.0 * vectors[row * 2 + column] + vectors[(1 - row) * 2 + column];
                assert!(
                    (product - values[column] * vectors[row * 2 + column]).abs() < 1e-9,
                    "A v != lambda v at ({row},{column})"
                );
            }
        }
        let dot = vectors[0] * vectors[1] + vectors[2] * vectors[3];
        assert!(dot.abs() < 1e-10, "eigenvectors not orthogonal: {dot}");
    }

    #[test]
    fn eigen_symmetric_3x3_matches_known_spectrum() {
        // [[2,-1,0],[-1,2,-1],[0,-1,2]] has eigenvalues 2-sqrt(2), 2, 2+sqrt(2).
        let outcome = call_with(
            "linear_algebra.eigen_symmetric",
            serde_json::json!({
                "matrix": {
                    "kind": "matrix", "rows": 3, "cols": 3,
                    "data": [2, -1, 0, -1, 2, -1, 0, -1, 2]
                }
            }),
            scientific(),
        )
        .unwrap();
        let values = floats(record_field(&outcome, "values"));
        let root_two = 2.0f64.sqrt();
        assert!((values[0] - (2.0 - root_two)).abs() < 1e-10);
        assert!((values[1] - 2.0).abs() < 1e-10);
        assert!((values[2] - (2.0 + root_two)).abs() < 1e-10);
        let a = [2.0, -1.0, 0.0, -1.0, 2.0, -1.0, 0.0, -1.0, 2.0];
        let (rows, cols, vectors) = matrix_floats(record_field(&outcome, "vectors"));
        assert_eq!((rows, cols), (3, 3));
        for column in 0..3 {
            for i in 0..3 {
                let mut product = 0.0;
                for j in 0..3 {
                    product += a[i * 3 + j] * vectors[j * 3 + column];
                }
                assert!(
                    (product - values[column] * vectors[i * 3 + column]).abs() < 1e-9,
                    "A v != lambda v at ({i},{column})"
                );
            }
            let norm = (0..3)
                .map(|row| vectors[row * 3 + column].powi(2))
                .sum::<f64>()
                .sqrt();
            assert!((norm - 1.0).abs() < 1e-10);
        }
        for left in 0..3 {
            for right in (left + 1)..3 {
                let dot: f64 = (0..3)
                    .map(|row| vectors[row * 3 + left] * vectors[row * 3 + right])
                    .sum();
                assert!(dot.abs() < 1e-10, "columns {left},{right} dot {dot}");
            }
        }
    }

    #[test]
    fn svd_reconstructs_3x2_matrix() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let outcome = call_with(
            "linear_algebra.svd",
            serde_json::json!({
                "matrix": {"kind": "matrix", "rows": 3, "cols": 2, "data": [1, 2, 3, 4, 5, 6]}
            }),
            scientific(),
        )
        .unwrap();
        let (m, k, u) = matrix_floats(record_field(&outcome, "u"));
        let s = floats(record_field(&outcome, "s"));
        let (n, k2, v) = matrix_floats(record_field(&outcome, "v"));
        assert_eq!((m, k), (3, 2));
        assert_eq!((n, k2), (2, 2));
        assert_eq!(s.len(), 2);
        for i in 0..3 {
            for j in 0..2 {
                let mut reconstructed = 0.0;
                for c in 0..2 {
                    reconstructed += u[i * 2 + c] * s[c] * v[j * 2 + c];
                }
                assert!(
                    (reconstructed - a[i * 2 + j]).abs() < 1e-10,
                    "({i},{j}): {reconstructed} vs {}",
                    a[i * 2 + j]
                );
            }
        }
    }

    #[test]
    fn svd_reconstructs_2x3_matrix() {
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let outcome = call_with(
            "linear_algebra.svd",
            serde_json::json!({
                "matrix": {"kind": "matrix", "rows": 2, "cols": 3, "data": [1, 2, 3, 4, 5, 6]}
            }),
            scientific(),
        )
        .unwrap();
        let (m, k, u) = matrix_floats(record_field(&outcome, "u"));
        let s = floats(record_field(&outcome, "s"));
        let (n, k2, v) = matrix_floats(record_field(&outcome, "v"));
        assert_eq!((m, k), (2, 2));
        assert_eq!((n, k2), (3, 2));
        for i in 0..2 {
            for j in 0..3 {
                let mut reconstructed = 0.0;
                for c in 0..2 {
                    reconstructed += u[i * 2 + c] * s[c] * v[j * 2 + c];
                }
                assert!(
                    (reconstructed - a[i * 3 + j]).abs() < 1e-10,
                    "({i},{j}): {reconstructed} vs {}",
                    a[i * 3 + j]
                );
            }
        }
    }

    #[test]
    fn svd_of_diagonal_matrix_has_expected_singular_values() {
        let outcome = call_with(
            "linear_algebra.svd",
            serde_json::json!({
                "matrix": {"kind": "matrix", "rows": 3, "cols": 3, "data": [3, 0, 0, 0, 2, 0, 0, 0, 1]}
            }),
            scientific(),
        )
        .unwrap();
        let s = floats(record_field(&outcome, "s"));
        assert_eq!(s.len(), 3);
        assert!((s[0] - 3.0).abs() < 1e-12, "s[0] = {}", s[0]);
        assert!((s[1] - 2.0).abs() < 1e-12, "s[1] = {}", s[1]);
        assert!((s[2] - 1.0).abs() < 1e-12, "s[2] = {}", s[2]);
    }

    #[test]
    fn power_iteration_finds_dominant_eigenvalue() {
        let outcome = call_with(
            "linear_algebra.power_iteration",
            serde_json::json!({"matrix": matrix(&[2, 0, 0, 1])}),
            scientific(),
        )
        .unwrap();
        let eigenvalue = record_field(&outcome, "eigenvalue")
            .as_number()
            .unwrap()
            .to_f64()
            .unwrap();
        assert!((eigenvalue - 2.0).abs() < 1e-9, "eigenvalue = {eigenvalue}");
        let eigenvector = floats(record_field(&outcome, "eigenvector"));
        assert!((eigenvector[0].abs() - 1.0).abs() < 1e-9);
        assert!(eigenvector[1].abs() < 1e-9);
        let residual = (2.0 * eigenvector[0] - eigenvalue * eigenvector[0])
            .abs()
            .max((1.0 * eigenvector[1] - eigenvalue * eigenvector[1]).abs());
        assert!(residual < 1e-9);
    }

    #[test]
    fn power_iteration_reports_non_convergence() {
        let error = call_with(
            "linear_algebra.power_iteration",
            serde_json::json!({"matrix": matrix(&[2, 0, 0, 1]), "max_iterations": 1}),
            scientific(),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::NonConvergence);
    }

    #[test]
    fn pca_perfect_correlation_first_ratio_is_one() {
        let data = serde_json::json!({
            "kind": "matrix", "rows": 3, "cols": 2, "data": [1, 2, 2, 4, 3, 6]
        });
        for standardize in [false, true] {
            let outcome = call_with(
                "linear_algebra.pca",
                serde_json::json!({"data": data, "standardize": standardize}),
                scientific(),
            )
            .unwrap();
            let means = floats(record_field(&outcome, "means"));
            assert!((means[0] - 2.0).abs() < 1e-12);
            assert!((means[1] - 4.0).abs() < 1e-12);
            let stddevs = floats(record_field(&outcome, "stddevs"));
            assert!((stddevs[0] - 1.0).abs() < 1e-12);
            assert!((stddevs[1] - 2.0).abs() < 1e-12);
            let ratios = floats(record_field(&outcome, "explained_variance_ratio"));
            assert!(
                (ratios[0] - 1.0).abs() < 1e-10,
                "standardize={standardize}: ratios = {ratios:?}"
            );
            assert!(ratios[1].abs() < 1e-10);
            let eigenvalues = floats(record_field(&outcome, "eigenvalues"));
            assert!(eigenvalues[1].abs() < 1e-10);
            let (rows, cols, components) = matrix_floats(record_field(&outcome, "components"));
            assert_eq!((rows, cols), (2, 2));
            let (score_rows, score_cols, _) = matrix_floats(record_field(&outcome, "scores"));
            assert_eq!((score_rows, score_cols), (3, 2));
            let _ = components;
        }
    }

    #[test]
    fn spectral_functions_reject_non_scientific_modes() {
        let error = call(
            "linear_algebra.eigen_symmetric",
            serde_json::json!({"matrix": matrix(&[2, 1, 1, 2])}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);
        let error = call(
            "linear_algebra.svd",
            serde_json::json!({"matrix": matrix(&[1, 2, 2, 4])}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);
        let error = call(
            "linear_algebra.power_iteration",
            serde_json::json!({"matrix": matrix(&[2, 0, 0, 1])}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);
        let error = call(
            "linear_algebra.pca",
            serde_json::json!({"data": {"kind": "matrix", "rows": 3, "cols": 2, "data": [1, 2, 2, 4, 3, 6]}}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);
    }

    #[test]
    fn eigen_symmetric_rejects_asymmetric_input() {
        let error = call_with(
            "linear_algebra.eigen_symmetric",
            serde_json::json!({"matrix": matrix(&[2, 1, 0, 2])}),
            scientific(),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    #[test]
    fn documented_examples_execute() {
        use bicmath_core::contract::ExampleExpectation;
        for function in module().functions {
            let id = function.descriptor().id.clone();
            for example in function.descriptor().examples.clone() {
                let raw = serde_json::to_value(&example.arguments).expect("serializable");
                let result = call_with(&id, raw, scientific());
                match (&example.expected, result) {
                    (Some(ExampleExpectation::Error(expected)), Err(error)) => {
                        assert_eq!(
                            error.code, *expected,
                            "{id} example {:?}: wrong error",
                            example.title
                        );
                    }
                    (Some(ExampleExpectation::Error(expected)), Ok(outcome)) => {
                        panic!(
                            "{id} example {:?}: expected error {expected:?}, got {:?}",
                            example.title, outcome.value
                        );
                    }
                    (Some(ExampleExpectation::Value(expected)), Ok(outcome)) => {
                        assert_eq!(&outcome.value, expected, "{id} example {:?}", example.title);
                    }
                    (Some(ExampleExpectation::Contains(needle)), Ok(outcome)) => {
                        let rendered = serde_json::to_string(&outcome.value).unwrap();
                        assert!(
                            rendered.contains(needle.as_str()),
                            "{id} example {:?}: {rendered}",
                            example.title
                        );
                    }
                    (_, Err(error)) => {
                        panic!("{id} example {:?} failed: {error}", example.title);
                    }
                    (None, Ok(_)) => {}
                }
            }
        }
    }
}

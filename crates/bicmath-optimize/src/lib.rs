//! Optimization module: exact rational linear programming, bounded univariate
//! minimization, nonlinear least squares, and linear assignment.
//!
//! `optimize.linear_program` runs an exact two-phase simplex over
//! [`num_rational::BigRational`] with Bland's rule, so it is deterministic and
//! cannot cycle; no solution is fabricated when the problem is infeasible or
//! unbounded. The remaining functions are deterministic binary64 methods over
//! restricted expressions; no filesystem, network, clock, randomness, or unsafe
//! code is used.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Module, ModuleDescriptor, Outcome,
    ParamDescriptor, SimpleFunction, require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::eval::{SimpleCalls, evaluate_f64};
use bicmath_core::expr::{Expr, parse_expression};
use bicmath_core::number::{Number, NumericMode};
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;

use num_bigint::BigInt;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};

const MODULE: &str = "optimize";
const VERSION: &str = "1.0.0";

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn integer_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Integer)
}

fn float64_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Float64)
}

fn any_number_schema() -> ValueSchema {
    ValueSchema::number(NumberKind::Any)
}

fn all_modes() -> Vec<NumericMode> {
    vec![
        NumericMode::Exact,
        NumericMode::Auto,
        NumericMode::Scientific,
    ]
}

fn scientific_modes() -> Vec<NumericMode> {
    vec![NumericMode::Scientific]
}

fn require_scientific(ctx: &ExecContext, id: &str) -> Result<(), EngineError> {
    require_mode(ctx, &scientific_modes(), id)
}

fn parse_value(raw: serde_json::Value) -> Value {
    serde_json::from_value(raw).expect("example value must be valid")
}

fn example_args(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(name, raw)| (name.to_string(), parse_value(raw.clone())))
        .collect()
}

fn float_value(value: f64) -> Result<Value, EngineError> {
    Ok(Value::Number(Number::float(value)?))
}

fn integer_value(value: u64) -> Value {
    Value::Number(Number::Integer(BigInt::from(value)))
}

fn rational_value(value: &BigRational) -> Value {
    if value.is_integer() {
        Value::Number(Number::Integer(value.to_integer()))
    } else {
        Value::Number(Number::Rational(value.clone()))
    }
}

/// Extract an exact rational from a wire value, rejecting float64.
fn exact_rational(value: &Value, path: &str) -> Result<BigRational, EngineError> {
    let number = value
        .as_number()
        .map_err(|error| error.with_path(path.to_string()))?;
    number.as_exact_rational().ok_or_else(|| {
        EngineError::malformed(format!(
            "expected an exact number (integer, rational, or decimal), found {}",
            number.kind_name()
        ))
        .with_path(path.to_string())
    })
}

/// Convert a numeric payload to a finite f64.
fn finite_f64(number: &Number, path: &str) -> Result<f64, EngineError> {
    let value = number.to_f64().ok_or_else(|| {
        EngineError::domain(format!(
            "{} value is not representable as float64",
            number.kind_name()
        ))
        .with_path(path.to_string())
    })?;
    if !value.is_finite() {
        return Err(EngineError::domain("value overflows float64").with_path(path.to_string()));
    }
    Ok(value)
}

fn finite_argument(args: &Args, name: &str) -> Result<f64, EngineError> {
    finite_f64(args.number(name)?, name)
}

fn number_array_f64(args: &Args, name: &str) -> Result<Vec<f64>, EngineError> {
    let items = args.array(name)?;
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let path = format!("{name}[{index}]");
        let number = item
            .as_number()
            .map_err(|error| error.with_path(path.clone()))?;
        out.push(finite_f64(number, &path)?);
    }
    Ok(out)
}

fn positive_tolerance(args: &Args, name: &str, default: f64) -> Result<f64, EngineError> {
    match args.optional_f64(name)? {
        None => Ok(default),
        Some(value) if value > 0.0 && value.is_finite() => Ok(value),
        Some(_) => Err(
            EngineError::domain(format!("{name} must be positive and finite"))
                .with_path(name.to_string()),
        ),
    }
}

fn optional_positive_u64(args: &Args, name: &str, default: u64) -> Result<u64, EngineError> {
    match args.optional_integer(name)? {
        None => Ok(default),
        Some(value) => value.to_u64().filter(|value| *value > 0).ok_or_else(|| {
            EngineError::domain(format!("{name} must be a positive integer"))
                .with_path(name.to_string())
        }),
    }
}

/// Validate an ASCII identifier used as a variable or parameter name.
fn validate_identifier(name: &str, path: &str) -> Result<(), EngineError> {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => {
            return Err(EngineError::domain("expected a non-empty ASCII identifier")
                .with_path(path.to_string()));
        }
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(EngineError::domain(
            "identifier may contain only ASCII letters, digits, and underscores",
        )
        .with_path(path.to_string()));
    }
    Ok(())
}

/// The restricted function table shared by every expression objective.
///
/// Only the documented functions are reachable: `sin`, `cos`, `tan`, `asin`,
/// `acos`, `atan`, `sinh`, `cosh`, `tanh`, `exp`, `ln`, `log10`, `log2`,
/// `sqrt`, `abs`, `pow`, `floor`, `ceil`, `min`, `max`.
fn optimization_calls() -> SimpleCalls {
    SimpleCalls::new()
        .add("sin", 1, |args| Ok(libm::sin(args[0])))
        .add("cos", 1, |args| Ok(libm::cos(args[0])))
        .add("tan", 1, |args| Ok(libm::tan(args[0])))
        .add("asin", 1, |args| {
            if args[0].abs() <= 1.0 {
                Ok(libm::asin(args[0]))
            } else {
                Err(EngineError::domain("asin requires |x| <= 1"))
            }
        })
        .add("acos", 1, |args| {
            if args[0].abs() <= 1.0 {
                Ok(libm::acos(args[0]))
            } else {
                Err(EngineError::domain("acos requires |x| <= 1"))
            }
        })
        .add("atan", 1, |args| Ok(libm::atan(args[0])))
        .add("sinh", 1, |args| Ok(libm::sinh(args[0])))
        .add("cosh", 1, |args| Ok(libm::cosh(args[0])))
        .add("tanh", 1, |args| Ok(libm::tanh(args[0])))
        .add("exp", 1, |args| Ok(libm::exp(args[0])))
        .add("ln", 1, |args| {
            if args[0] > 0.0 {
                Ok(libm::log(args[0]))
            } else {
                Err(EngineError::domain("ln requires x > 0"))
            }
        })
        .add("log10", 1, |args| {
            if args[0] > 0.0 {
                Ok(libm::log10(args[0]))
            } else {
                Err(EngineError::domain("log10 requires x > 0"))
            }
        })
        .add("log2", 1, |args| {
            if args[0] > 0.0 {
                Ok(libm::log2(args[0]))
            } else {
                Err(EngineError::domain("log2 requires x > 0"))
            }
        })
        .add("sqrt", 1, |args| {
            if args[0] >= 0.0 {
                Ok(libm::sqrt(args[0]))
            } else {
                Err(EngineError::domain("sqrt requires x >= 0"))
            }
        })
        .add("abs", 1, |args| Ok(libm::fabs(args[0])))
        .add("pow", 2, |args| Ok(libm::pow(args[0], args[1])))
        .add("floor", 1, |args| Ok(libm::floor(args[0])))
        .add("ceil", 1, |args| Ok(libm::ceil(args[0])))
        .add("min", 2, |args| Ok(args[0].min(args[1])))
        .add("max", 2, |args| Ok(args[0].max(args[1])))
}

// ---------------------------------------------------------------------------
// Linear programming: exact two-phase simplex with Bland's rule
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sense {
    Max,
    Min,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Relation {
    Le,
    Ge,
    Eq,
}

impl Relation {
    fn parse(text: &str, path: &str) -> Result<Relation, EngineError> {
        match text {
            "le" => Ok(Relation::Le),
            "ge" => Ok(Relation::Ge),
            "eq" => Ok(Relation::Eq),
            other => Err(EngineError::domain(format!(
                "unknown relation {other:?}; expected \"le\", \"ge\", or \"eq\""
            ))
            .with_path(path.to_string())),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LpStatus {
    Optimal,
    Infeasible,
    Unbounded,
}

impl LpStatus {
    fn as_str(self) -> &'static str {
        match self {
            LpStatus::Optimal => "optimal",
            LpStatus::Infeasible => "infeasible",
            LpStatus::Unbounded => "unbounded",
        }
    }
}

struct LpConstraint {
    coefficients: Vec<BigRational>,
    relation: Relation,
    rhs: BigRational,
}

struct LinearProgram {
    objective: Vec<BigRational>,
    constraints: Vec<LpConstraint>,
    sense: Sense,
    variable_names: Option<Vec<String>>,
}

struct LpSolution {
    status: LpStatus,
    objective: Option<BigRational>,
    x: Vec<BigRational>,
    dual: Vec<BigRational>,
    reduced_costs: Vec<BigRational>,
    iterations: u64,
}

fn parse_sense(args: &Args) -> Result<Sense, EngineError> {
    match args.optional_text("sense")? {
        None | Some("max") => Ok(Sense::Max),
        Some("min") => Ok(Sense::Min),
        Some(other) => Err(EngineError::domain(format!(
            "unknown sense {other:?}; expected \"max\" or \"min\""
        ))
        .with_path("sense".to_string())),
    }
}

fn parse_variable_names(args: &Args, variables: usize) -> Result<Option<Vec<String>>, EngineError> {
    let Some(value) = args.get("variable_names") else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let items = value
        .as_array()
        .map_err(|error| error.with_path("variable_names".to_string()))?;
    if items.len() != variables {
        return Err(EngineError::domain(format!(
            "variable_names has {} name(s) but the objective declares {variables} variable(s)",
            items.len()
        ))
        .with_path("variable_names".to_string()));
    }
    let mut names = Vec::with_capacity(variables);
    let mut seen = BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        let path = format!("variable_names[{index}]");
        let name = item
            .as_text()
            .map_err(|error| error.with_path(path.clone()))?;
        validate_identifier(name, &path)?;
        if !seen.insert(name.to_string()) {
            return Err(
                EngineError::domain(format!("duplicate variable name {name:?}")).with_path(path),
            );
        }
        names.push(name.to_string());
    }
    Ok(Some(names))
}

fn parse_linear_program(args: &Args) -> Result<LinearProgram, EngineError> {
    let objective_values = args.array("objective")?;
    if objective_values.is_empty() {
        return Err(
            EngineError::malformed("objective must have at least one coefficient")
                .with_path("objective".to_string()),
        );
    }
    let mut objective = Vec::with_capacity(objective_values.len());
    for (index, value) in objective_values.iter().enumerate() {
        objective.push(exact_rational(value, &format!("objective[{index}]"))?);
    }
    let variables = objective.len();

    let constraint_values = args.array("constraints")?;
    let mut constraints = Vec::with_capacity(constraint_values.len());
    for (index, value) in constraint_values.iter().enumerate() {
        let path = format!("constraints[{index}]");
        let record = value
            .as_record()
            .map_err(|error| error.with_path(path.clone()))?;
        let coefficients_value = record.get("coefficients").ok_or_else(|| {
            EngineError::malformed("missing required field \"coefficients\"")
                .with_path(path.clone())
        })?;
        let coefficient_items = coefficients_value
            .as_array()
            .map_err(|error| error.with_path(format!("{path}.coefficients")))?;
        if coefficient_items.len() != variables {
            return Err(EngineError::domain(format!(
                "constraint {index} has {} coefficient(s); the objective declares {variables} \
                 variable(s)",
                coefficient_items.len()
            ))
            .with_path(format!("{path}.coefficients")));
        }
        let mut coefficients = Vec::with_capacity(variables);
        for (column, item) in coefficient_items.iter().enumerate() {
            coefficients.push(exact_rational(
                item,
                &format!("{path}.coefficients[{column}]"),
            )?);
        }
        let relation_value = record.get("relation").ok_or_else(|| {
            EngineError::malformed("missing required field \"relation\"").with_path(path.clone())
        })?;
        let relation_text = relation_value
            .as_text()
            .map_err(|error| error.with_path(format!("{path}.relation")))?;
        let relation = Relation::parse(relation_text, &format!("{path}.relation"))?;
        let rhs_value = record.get("rhs").ok_or_else(|| {
            EngineError::malformed("missing required field \"rhs\"").with_path(path.clone())
        })?;
        let rhs = exact_rational(rhs_value, &format!("{path}.rhs"))?;
        constraints.push(LpConstraint {
            coefficients,
            relation,
            rhs,
        });
    }

    let sense = parse_sense(args)?;
    let variable_names = parse_variable_names(args, variables)?;
    Ok(LinearProgram {
        objective,
        constraints,
        sense,
        variable_names,
    })
}

/// A row normalized so that its right-hand side is non-negative.
struct NormalizedRow {
    coefficients: Vec<BigRational>,
    rhs: BigRational,
    relation: Relation,
    sigma: i32,
}

#[derive(Clone, Copy)]
enum AuxKind {
    Slack,
    Surplus,
    Artificial,
}

/// The exact simplex tableau. `reduced` holds the reduced costs of the current
/// objective and `objective` its value; `basis[i]` is the basic column of row
/// `i`. Artificial columns are tracked so they can be excluded from entering
/// candidates in phase two while still carrying the basis inverse for duals.
struct Simplex {
    rows: Vec<Vec<BigRational>>,
    rhs: Vec<BigRational>,
    basis: Vec<usize>,
    reduced: Vec<BigRational>,
    objective: BigRational,
    artificial: Vec<bool>,
}

impl Simplex {
    /// Express the supplied objective coefficients in terms of the nonbasic
    /// variables by pricing out the current basis.
    fn canonicalize(&mut self, costs: &[BigRational]) {
        self.reduced = costs.to_vec();
        self.objective = BigRational::zero();
        for i in 0..self.rows.len() {
            let factor = self.reduced[self.basis[i]].clone();
            if factor.is_zero() {
                continue;
            }
            let row = self.rows[i].clone();
            for (value, coefficient) in self.reduced.iter_mut().zip(row.iter()) {
                *value -= &factor * coefficient;
            }
            self.objective += &factor * &self.rhs[i];
        }
    }

    fn pivot(&mut self, pivot_row: usize, pivot_column: usize) {
        let pivot = self.rows[pivot_row][pivot_column].clone();
        for value in &mut self.rows[pivot_row] {
            *value /= &pivot;
        }
        self.rhs[pivot_row] /= &pivot;
        let pivot_values = self.rows[pivot_row].clone();
        let pivot_rhs = self.rhs[pivot_row].clone();
        for (i, row) in self.rows.iter_mut().enumerate() {
            if i == pivot_row {
                continue;
            }
            let factor = row[pivot_column].clone();
            if factor.is_zero() {
                continue;
            }
            for (value, coefficient) in row.iter_mut().zip(pivot_values.iter()) {
                *value -= &factor * coefficient;
            }
            self.rhs[i] -= &factor * &pivot_rhs;
        }
        let factor = self.reduced[pivot_column].clone();
        if !factor.is_zero() {
            for (value, coefficient) in self.reduced.iter_mut().zip(pivot_values.iter()) {
                *value -= &factor * coefficient;
            }
            self.objective += &factor * &self.rhs[pivot_row];
        }
        self.basis[pivot_row] = pivot_column;
    }

    /// Bland's rule: the entering variable is the smallest-index column with a
    /// positive reduced cost; ties in the minimum-ratio test are broken by the
    /// smallest basic index. This guarantees termination without cycling.
    fn run(
        &mut self,
        ctx: &ExecContext,
        iterations: &mut u64,
        include_artificial: bool,
    ) -> Result<bool, EngineError> {
        loop {
            ctx.check()?;
            let entering = (0..self.reduced.len()).find(|&column| {
                (include_artificial || !self.artificial[column])
                    && self.reduced[column].is_positive()
            });
            let Some(entering) = entering else {
                return Ok(true);
            };
            let mut leaving: Option<usize> = None;
            for i in 0..self.rows.len() {
                let coefficient = &self.rows[i][entering];
                if !coefficient.is_positive() {
                    continue;
                }
                let replace = match leaving {
                    None => true,
                    Some(current) => {
                        let candidate_ratio = &self.rhs[i] * &self.rows[current][entering];
                        let current_ratio = &self.rhs[current] * coefficient;
                        candidate_ratio < current_ratio
                            || (candidate_ratio == current_ratio
                                && self.basis[i] < self.basis[current])
                    }
                };
                if replace {
                    leaving = Some(i);
                }
            }
            let Some(leaving) = leaving else {
                return Ok(false);
            };
            self.pivot(leaving, entering);
            *iterations += 1;
            if *iterations > ctx.limits.max_iterations {
                return Err(EngineError::new(
                    ErrorCode::ResourceLimit,
                    format!(
                        "linear_program exceeded the iteration limit of {}",
                        ctx.limits.max_iterations
                    ),
                ));
            }
            self.check_sizes(ctx)?;
        }
    }

    fn check_sizes(&self, ctx: &ExecContext) -> Result<(), EngineError> {
        let cap = ctx.limits.max_integer_bits as u64;
        let within =
            |value: &BigRational| value.numer().bits() <= cap && value.denom().bits() <= cap;
        if self.rows.iter().flatten().all(within)
            && self.rhs.iter().all(within)
            && self.reduced.iter().all(within)
        {
            Ok(())
        } else {
            Err(EngineError::new(
                ErrorCode::ResourceLimit,
                format!(
                    "simplex intermediate values exceeded the {cap}-bit exact arithmetic limit"
                ),
            ))
        }
    }
}

fn solve_linear_program(
    program: &LinearProgram,
    ctx: &ExecContext,
) -> Result<LpSolution, EngineError> {
    let variables = program.objective.len();
    let constraints = program.constraints.len();
    let sense_sign: i32 = match program.sense {
        Sense::Max => 1,
        Sense::Min => -1,
    };
    let costs: Vec<BigRational> = program
        .objective
        .iter()
        .map(|value| {
            if sense_sign > 0 {
                value.clone()
            } else {
                -value
            }
        })
        .collect();

    let mut normalized = Vec::with_capacity(constraints);
    for constraint in &program.constraints {
        let mut coefficients = constraint.coefficients.clone();
        let mut rhs = constraint.rhs.clone();
        let mut relation = constraint.relation;
        let mut sigma = 1i32;
        if rhs.is_negative() {
            for coefficient in &mut coefficients {
                *coefficient = -&*coefficient;
            }
            rhs = -rhs;
            relation = match relation {
                Relation::Le => Relation::Ge,
                Relation::Ge => Relation::Le,
                Relation::Eq => Relation::Eq,
            };
            sigma = -1;
        }
        normalized.push(NormalizedRow {
            coefficients,
            rhs,
            relation,
            sigma,
        });
    }

    let mut aux_kinds = Vec::with_capacity(constraints);
    let mut columns = variables;
    for row in &normalized {
        let kind = match row.relation {
            Relation::Le => {
                columns += 1;
                AuxKind::Slack
            }
            Relation::Ge => {
                columns += 2;
                AuxKind::Surplus
            }
            Relation::Eq => {
                columns += 1;
                AuxKind::Artificial
            }
        };
        aux_kinds.push(kind);
    }
    if constraints > 0 && columns.saturating_mul(constraints) > ctx.limits.max_matrix_elements {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "the {constraints}x{columns} simplex tableau exceeds the {} element limit",
                ctx.limits.max_matrix_elements
            ),
        ));
    }

    let mut rows = Vec::with_capacity(constraints);
    let mut rhs_values = Vec::with_capacity(constraints);
    let mut basis = Vec::with_capacity(constraints);
    let mut id_columns = Vec::with_capacity(constraints);
    let mut artificial = vec![false; columns];
    let mut next_column = variables;
    for (index, row) in normalized.iter().enumerate() {
        let mut tableau_row = vec![BigRational::zero(); columns];
        tableau_row[..variables].clone_from_slice(&row.coefficients);
        let (id_column, basic) = match aux_kinds[index] {
            AuxKind::Slack => {
                let column = next_column;
                next_column += 1;
                tableau_row[column] = BigRational::one();
                (column, column)
            }
            AuxKind::Surplus => {
                let surplus = next_column;
                let artificial_column = next_column + 1;
                next_column += 2;
                tableau_row[surplus] = -BigRational::one();
                tableau_row[artificial_column] = BigRational::one();
                artificial[artificial_column] = true;
                (artificial_column, artificial_column)
            }
            AuxKind::Artificial => {
                let column = next_column;
                next_column += 1;
                tableau_row[column] = BigRational::one();
                artificial[column] = true;
                (column, column)
            }
        };
        rows.push(tableau_row);
        rhs_values.push(row.rhs.clone());
        basis.push(basic);
        id_columns.push(id_column);
    }

    let mut simplex = Simplex {
        rows,
        rhs: rhs_values,
        basis,
        reduced: Vec::new(),
        objective: BigRational::zero(),
        artificial,
    };

    let phase_one_costs: Vec<BigRational> = (0..columns)
        .map(|column| {
            if simplex.artificial[column] {
                -BigRational::one()
            } else {
                BigRational::zero()
            }
        })
        .collect();
    simplex.canonicalize(&phase_one_costs);
    let mut iterations = 0u64;
    if !simplex.run(ctx, &mut iterations, true)? {
        return Err(EngineError::internal(
            "phase-one simplex reported an unbounded objective",
        ));
    }
    if simplex.objective.is_negative() {
        return Ok(LpSolution {
            status: LpStatus::Infeasible,
            objective: None,
            x: Vec::new(),
            dual: Vec::new(),
            reduced_costs: Vec::new(),
            iterations,
        });
    }

    let phase_two_costs: Vec<BigRational> = (0..columns)
        .map(|column| {
            if column < variables {
                costs[column].clone()
            } else {
                BigRational::zero()
            }
        })
        .collect();
    simplex.canonicalize(&phase_two_costs);
    if !simplex.run(ctx, &mut iterations, false)? {
        return Ok(LpSolution {
            status: LpStatus::Unbounded,
            objective: None,
            x: Vec::new(),
            dual: Vec::new(),
            reduced_costs: Vec::new(),
            iterations,
        });
    }

    let mut x = vec![BigRational::zero(); variables];
    for (row_index, &basic) in simplex.basis.iter().enumerate() {
        if basic < variables {
            x[basic] = simplex.rhs[row_index].clone();
        }
    }
    let objective = if sense_sign > 0 {
        simplex.objective.clone()
    } else {
        -simplex.objective.clone()
    };
    let reduced_costs: Vec<BigRational> = (0..variables)
        .map(|column| {
            if sense_sign > 0 {
                simplex.reduced[column].clone()
            } else {
                -simplex.reduced[column].clone()
            }
        })
        .collect();
    let mut dual = Vec::with_capacity(constraints);
    for row_index in 0..constraints {
        let mut value = BigRational::zero();
        for (basis_row, &basic) in simplex.basis.iter().enumerate() {
            if basic < variables {
                value += &costs[basic] * &simplex.rows[basis_row][id_columns[row_index]];
            }
        }
        if normalized[row_index].sigma * sense_sign < 0 {
            value = -value;
        }
        dual.push(value);
    }
    Ok(LpSolution {
        status: LpStatus::Optimal,
        objective: Some(objective),
        x,
        dual,
        reduced_costs,
        iterations,
    })
}

fn variable_solution(values: &[BigRational], names: Option<&[String]>) -> Value {
    match names {
        Some(names) => Value::record(names.iter().cloned().zip(values.iter().map(rational_value))),
        None => Value::Array(values.iter().map(rational_value).collect()),
    }
}

// ---------------------------------------------------------------------------
// Descriptors: linear programming
// ---------------------------------------------------------------------------

fn constraint_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("coefficients", ValueSchema::array(ValueSchema::exact()))
                .with_description("Constraint row coefficients in objective order."),
            FieldSchema::required(
                "relation",
                ValueSchema::Enum {
                    variants: vec!["le".to_string(), "ge".to_string(), "eq".to_string()],
                },
            )
            .with_description("\"le\" (<=), \"ge\" (>=), or \"eq\" (=)."),
            FieldSchema::required("rhs", ValueSchema::exact())
                .with_description("Exact right-hand side constant."),
        ],
        allow_extra: false,
    }
}

fn linear_program_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required(
                "status",
                ValueSchema::Enum {
                    variants: vec![
                        "optimal".to_string(),
                        "infeasible".to_string(),
                        "unbounded".to_string(),
                    ],
                },
            )
            .with_description("\"optimal\", \"infeasible\", or \"unbounded\"."),
            FieldSchema::optional("objective", ValueSchema::exact())
                .with_description("Present only when the status is \"optimal\"."),
            FieldSchema::optional("x", ValueSchema::Any).with_description(
                "Array of variable values, or a record keyed by variable_names when supplied; \
                 present only when the status is \"optimal\".",
            ),
            FieldSchema::optional("dual", ValueSchema::array(ValueSchema::exact()))
                .with_description("One value per constraint; present only when optimal."),
            FieldSchema::optional("reduced_costs", ValueSchema::array(ValueSchema::exact()))
                .with_description("One value per decision variable; present only when optimal."),
            FieldSchema::required("iterations", integer_schema())
                .with_description("Number of exact simplex pivots performed."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"rational_simplex_bland\"."),
        ],
        allow_extra: false,
    }
}

fn linear_program_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "optimize.linear_program",
        MODULE,
        VERSION,
        "Linear program",
        "Solve a linear program exactly with a two-phase rational simplex.",
    )
    .with_description(
        "Maximizes (or minimizes, with sense \"min\") c^T x subject to linear constraints \
         with relations \"le\", \"ge\", or \"eq\" and x >= 0. All arithmetic is exact over \
         rationals and pivots follow Bland's rule, so the method is deterministic and cannot \
         cycle. The result reports the exact objective, primal solution, constraint duals, and \
         reduced costs. Infeasible and unbounded problems return the corresponding status \
         without fabricating a solution; dual and reduced-cost sign conventions follow the \
         declared sense.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "objective",
            "Objective coefficients, one per decision variable.",
            ValueSchema::array_with_len(ValueSchema::exact(), 1, None),
        ),
        ParamDescriptor::required(
            "constraints",
            "Constraint rows.",
            ValueSchema::array(constraint_schema()),
        ),
        ParamDescriptor::optional(
            "sense",
            "Optimization sense: \"max\" (default) or \"min\".",
            ValueSchema::Enum {
                variants: vec!["max".to_string(), "min".to_string()],
            },
        ),
        ParamDescriptor::optional(
            "variable_names",
            "Optional names; when supplied, x is a record keyed by name.",
            ValueSchema::array(ValueSchema::text()),
        ),
    ])
    .with_output(
        linear_program_output_schema(),
        "Status, exact objective and solution, duals, reduced costs, pivot count, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/optimize.md#linear_program")
    .with_examples(vec![
        Example::new(
            "maximize 3x + 5y",
            example_args(&[
                ("objective", serde_json::json!([3, 5])),
                (
                    "constraints",
                    serde_json::json!([
                        {"coefficients": [1, 0], "relation": "le", "rhs": 4},
                        {"coefficients": [0, 2], "relation": "le", "rhs": 12},
                        {"coefficients": [3, 2], "relation": "le", "rhs": 18}
                    ]),
                ),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "optimal",
            "objective": {"kind": "integer", "value": "36"},
            "x": [
                {"kind": "integer", "value": "2"},
                {"kind": "integer", "value": "6"}
            ],
            "dual": [
                {"kind": "integer", "value": "0"},
                {"kind": "rational", "numerator": "3", "denominator": "2"},
                {"kind": "integer", "value": "1"}
            ],
            "reduced_costs": [
                {"kind": "integer", "value": "0"},
                {"kind": "integer", "value": "0"}
            ],
            "iterations": {"kind": "integer", "value": "3"},
            "method": "rational_simplex_bland"
        }))),
        Example::new(
            "infeasible system",
            example_args(&[
                ("objective", serde_json::json!([1, 1])),
                (
                    "constraints",
                    serde_json::json!([
                        {"coefficients": [1, 1], "relation": "le", "rhs": 1},
                        {"coefficients": [1, 1], "relation": "ge", "rhs": 3}
                    ]),
                ),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "infeasible",
            "iterations": {"kind": "integer", "value": "1"},
            "method": "rational_simplex_bland"
        }))),
        Example::new(
            "unbounded direction",
            example_args(&[
                ("objective", serde_json::json!([1, 1])),
                (
                    "constraints",
                    serde_json::json!([
                        {"coefficients": [1, 0], "relation": "le", "rhs": 1}
                    ]),
                ),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "status": "unbounded",
            "iterations": {"kind": "integer", "value": "1"},
            "method": "rational_simplex_bland"
        }))),
    ])
}

fn invoke_linear_program(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let program = parse_linear_program(args)?;
    let solution = solve_linear_program(&program, ctx)?;
    let mut entries: Vec<(String, Value)> = vec![
        ("status".to_string(), Value::text(solution.status.as_str())),
        ("iterations".to_string(), integer_value(solution.iterations)),
        ("method".to_string(), Value::text("rational_simplex_bland")),
    ];
    if let Some(objective) = &solution.objective {
        entries.push(("objective".to_string(), rational_value(objective)));
        entries.push((
            "x".to_string(),
            variable_solution(&solution.x, program.variable_names.as_deref()),
        ));
        entries.push((
            "dual".to_string(),
            Value::Array(solution.dual.iter().map(rational_value).collect()),
        ));
        entries.push((
            "reduced_costs".to_string(),
            Value::Array(solution.reduced_costs.iter().map(rational_value).collect()),
        ));
    }
    Ok(Outcome::exact(Value::record(entries)))
}

// ---------------------------------------------------------------------------
// Univariate minimization: Brent and golden-section
// ---------------------------------------------------------------------------

struct Objective<'a> {
    expression: &'a Expr,
    variable: &'a str,
    calls: &'a SimpleCalls,
    ctx: &'a ExecContext,
    evaluations: u64,
}

impl Objective<'_> {
    fn evaluate(&mut self, x: f64) -> Result<f64, EngineError> {
        self.ctx.check()?;
        self.evaluations += 1;
        let mut bindings = BTreeMap::new();
        bindings.insert(self.variable.to_string(), x);
        let value = evaluate_f64(
            self.expression,
            &bindings,
            &self.ctx.limits,
            &self.calls.as_table(),
        )?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err(EngineError::domain(format!(
                "expression evaluated to a non-finite value at {} = {x}",
                self.variable
            )))
        }
    }
}

struct BrentOutcome {
    x: f64,
    value: f64,
    iterations: u64,
}

fn brent_minimize(
    objective: &mut Objective<'_>,
    lower: f64,
    upper: f64,
    tolerance: f64,
    max_iterations: u64,
) -> Result<BrentOutcome, EngineError> {
    const GOLDEN: f64 = 0.381_966_011_250_105_1;
    const ZEPS: f64 = 1e-21;
    let mut a = lower.min(upper);
    let mut b = lower.max(upper);
    let mut x = a + GOLDEN * (b - a);
    let mut w = x;
    let mut v = x;
    let mut fx = objective.evaluate(x)?;
    let mut fw = fx;
    let mut fv = fx;
    let mut d = 0.0f64;
    let mut e = 0.0f64;
    let mut iterations = 0u64;
    while iterations < max_iterations {
        iterations += 1;
        let midpoint = 0.5 * (a + b);
        let tolerance_x = tolerance * x.abs() + ZEPS;
        let tolerance_2 = 2.0 * tolerance_x;
        if (x - midpoint).abs() <= tolerance_2 - 0.5 * (b - a) {
            return Ok(BrentOutcome {
                x,
                value: fx,
                iterations,
            });
        }
        let mut used_parabola = false;
        if e.abs() > tolerance_x {
            let r = (x - w) * (fx - fv);
            let q = (x - v) * (fx - fw);
            let mut p = (x - v) * q - (x - w) * r;
            let mut q2 = 2.0 * (q - r);
            if q2 > 0.0 {
                p = -p;
            } else {
                q2 = -q2;
            }
            let previous_e = e;
            e = d;
            if p.abs() < 0.5 * q2 * previous_e.abs() && p > q2 * (a - x) && p < q2 * (b - x) {
                d = p / q2;
                let u = x + d;
                if (u - a) < tolerance_2 || (b - u) < tolerance_2 {
                    d = if midpoint >= x {
                        tolerance_x
                    } else {
                        -tolerance_x
                    };
                }
                used_parabola = true;
            }
        }
        if !used_parabola {
            e = if x < midpoint { b - x } else { a - x };
            d = GOLDEN * e;
        }
        let u = if d.abs() >= tolerance_x {
            x + d
        } else if d > 0.0 {
            x + tolerance_x
        } else {
            x - tolerance_x
        };
        let fu = objective.evaluate(u)?;
        if fu <= fx {
            if u < x {
                b = x;
            } else {
                a = x;
            }
            v = w;
            fv = fw;
            w = x;
            fw = fx;
            x = u;
            fx = fu;
        } else {
            if u < x {
                a = u;
            } else {
                b = u;
            }
            if fu <= fw || w == x {
                v = w;
                fv = fw;
                w = u;
                fw = fu;
            } else if fu <= fv || v == x || v == w {
                v = u;
                fv = fu;
            }
        }
    }
    Err(EngineError::new(
        ErrorCode::NonConvergence,
        format!(
            "brent_minimization did not meet the tolerance {tolerance} within {max_iterations} \
             iteration(s)"
        ),
    ))
}

struct GoldenOutcome {
    x: f64,
    value: f64,
    iterations: u64,
}

fn golden_section_minimize(
    objective: &mut Objective<'_>,
    lower: f64,
    upper: f64,
    tolerance: f64,
    max_iterations: u64,
) -> Result<GoldenOutcome, EngineError> {
    const INV_PHI: f64 = 0.618_033_988_749_894_9;
    let mut a = lower.min(upper);
    let mut b = lower.max(upper);
    let mut c = b - INV_PHI * (b - a);
    let mut d = a + INV_PHI * (b - a);
    let mut fc = objective.evaluate(c)?;
    let mut fd = objective.evaluate(d)?;
    let mut iterations = 0u64;
    while iterations < max_iterations {
        let midpoint = 0.5 * (a + b);
        if (b - a).abs() <= tolerance * (1.0 + midpoint.abs()) {
            break;
        }
        iterations += 1;
        if fc < fd {
            b = d;
            d = c;
            fd = fc;
            c = b - INV_PHI * (b - a);
            fc = objective.evaluate(c)?;
        } else {
            a = c;
            c = d;
            fc = fd;
            d = a + INV_PHI * (b - a);
            fd = objective.evaluate(d)?;
        }
    }
    let (x, value) = if fc <= fd { (c, fc) } else { (d, fd) };
    Ok(GoldenOutcome {
        x,
        value,
        iterations,
    })
}

fn minimize_output_schema(method: &str) -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("x", float64_schema())
                .with_description("Argument of the smallest value found."),
            FieldSchema::required("value", float64_schema())
                .with_description("Smallest objective value found."),
            FieldSchema::required("iterations", integer_schema())
                .with_description("Number of search iterations performed."),
            FieldSchema::required("evaluations", integer_schema())
                .with_description("Number of expression evaluations performed."),
            FieldSchema::required("converged", ValueSchema::Bool)
                .with_description("Always true for a successful result."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description(format!("Always \"{method}\".")),
        ],
        allow_extra: false,
    }
}

fn expression_parameters() -> Vec<ParamDescriptor> {
    vec![
        ParamDescriptor::required(
            "expression",
            "Restricted expression in the variable.",
            ValueSchema::Expression {
                variables: vec!["x".to_string()],
            },
        ),
        ParamDescriptor::required(
            "variable",
            "Name of the single variable used by the expression.",
            ValueSchema::text(),
        ),
        ParamDescriptor::required("lower", "Lower bracket endpoint.", any_number_schema()),
        ParamDescriptor::required("upper", "Upper bracket endpoint.", any_number_schema()),
        ParamDescriptor::optional(
            "tolerance",
            "Absolute x tolerance; default 1e-10.",
            any_number_schema(),
        ),
    ]
}

fn minimize_1d_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "optimize.minimize_1d",
        MODULE,
        VERSION,
        "Minimize 1-D",
        "Minimize a scalar expression on a closed interval with Brent's method.",
    )
    .with_description(
        "Brent's method combines inverse parabolic interpolation with golden-section \
         safeguarding on the closed interval [lower, upper]. The expression is a restricted \
         string using the named variable, numeric literals, parentheses, + - * / % ^, and the \
         functions sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, exp, ln, log10, log2, \
         sqrt, abs, pow, floor, ceil, min, and max. Non-convergence within the context \
         iteration budget is reported as an error; the last iterate is never returned as a \
         success.",
    )
    .with_parameters(expression_parameters())
    .with_output(
        minimize_output_schema("brent_minimization"),
        "Minimizer, value, iteration and evaluation counts, convergence flag, and method.",
    )
    .with_modes(scientific_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/optimize.md#minimize_1d")
    .with_examples(vec![
        Example::new(
            "minimize (x - 3)^2 on [0, 10]",
            example_args(&[
                ("expression", serde_json::json!("(x - 3)^2")),
                ("variable", serde_json::json!("x")),
                ("lower", serde_json::json!(0)),
                ("upper", serde_json::json!(10)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "x": {"kind": "float64", "value": "3"},
            "value": {"kind": "float64", "value": "0"},
            "iterations": {"kind": "integer", "value": "6"},
            "evaluations": {"kind": "integer", "value": "6"},
            "converged": true,
            "method": "brent_minimization"
        }))),
    ])
}

fn invoke_minimize_1d(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "optimize.minimize_1d")?;
    let source = args.text("expression")?;
    let variable = args.text("variable")?;
    validate_identifier(variable, "variable")?;
    let lower = finite_argument(args, "lower")?;
    let upper = finite_argument(args, "upper")?;
    if lower == upper {
        return Err(
            EngineError::domain("lower and upper must differ").with_path("upper".to_string())
        );
    }
    let tolerance = positive_tolerance(args, "tolerance", 1e-10)?;
    let expression = parse_expression(source, &ctx.limits)?;
    let calls = optimization_calls();
    let mut objective = Objective {
        expression: &expression,
        variable,
        calls: &calls,
        ctx,
        evaluations: 0,
    };
    let max_iterations = ctx.limits.max_iterations.min(10_000);
    let result = brent_minimize(&mut objective, lower, upper, tolerance, max_iterations)?;
    let value = Value::record([
        ("x", float_value(result.x)?),
        ("value", float_value(result.value)?),
        ("iterations", integer_value(result.iterations)),
        ("evaluations", integer_value(objective.evaluations)),
        ("converged", Value::Bool(true)),
        ("method", Value::text("brent_minimization")),
    ]);
    Ok(Outcome::approximate(value))
}

fn golden_section_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "optimize.golden_section",
        MODULE,
        VERSION,
        "Golden-section search",
        "Minimize a scalar expression on a closed interval by golden-section search.",
    )
    .with_description(
        "Golden-section search shrinks the bracket by the golden ratio until the interval is \
         smaller than the tolerance, evaluating one new point per iteration. It is slower \
         than Brent's method but has no interpolation assumptions and never fails to make \
         progress. The same restricted expression grammar and function set as \
         optimize.minimize_1d applies.",
    )
    .with_parameters(expression_parameters())
    .with_output(
        minimize_output_schema("golden_section"),
        "Minimizer, value, iteration and evaluation counts, convergence flag, and method.",
    )
    .with_modes(scientific_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/optimize.md#golden_section")
    .with_examples(vec![
        Example::new(
            "minimize x^2 - 4x + 7 on [0, 5]",
            example_args(&[
                ("expression", serde_json::json!("x^2 - 4*x + 7")),
                ("variable", serde_json::json!("x")),
                ("lower", serde_json::json!(0)),
                ("upper", serde_json::json!(5)),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "x": {"kind": "float64", "value": "2.000000020991479"},
            "value": {"kind": "float64", "value": "3"},
            "iterations": {"kind": "integer", "value": "49"},
            "evaluations": {"kind": "integer", "value": "51"},
            "converged": true,
            "method": "golden_section"
        }))),
    ])
}

fn invoke_golden_section(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "optimize.golden_section")?;
    let source = args.text("expression")?;
    let variable = args.text("variable")?;
    validate_identifier(variable, "variable")?;
    let lower = finite_argument(args, "lower")?;
    let upper = finite_argument(args, "upper")?;
    if lower == upper {
        return Err(
            EngineError::domain("lower and upper must differ").with_path("upper".to_string())
        );
    }
    let tolerance = positive_tolerance(args, "tolerance", 1e-10)?;
    let expression = parse_expression(source, &ctx.limits)?;
    let calls = optimization_calls();
    let mut objective = Objective {
        expression: &expression,
        variable,
        calls: &calls,
        ctx,
        evaluations: 0,
    };
    let max_iterations = ctx.limits.max_iterations.min(10_000);
    let result = golden_section_minimize(&mut objective, lower, upper, tolerance, max_iterations)?;
    let value = Value::record([
        ("x", float_value(result.x)?),
        ("value", float_value(result.value)?),
        ("iterations", integer_value(result.iterations)),
        ("evaluations", integer_value(objective.evaluations)),
        ("converged", Value::Bool(true)),
        ("method", Value::text("golden_section")),
    ]);
    Ok(Outcome::approximate(value))
}

// ---------------------------------------------------------------------------
// Nonlinear least squares: Levenberg-Marquardt
// ---------------------------------------------------------------------------

struct LmOutcome {
    parameters: Vec<f64>,
    residual_sum_squares: f64,
    iterations: u64,
}

struct LeastSquares<'a> {
    expression: &'a Expr,
    parameter_names: &'a [String],
    points: &'a [f64],
    targets: &'a [f64],
    calls: SimpleCalls,
    ctx: &'a ExecContext,
}

impl LeastSquares<'_> {
    fn residuals(&self, parameters: &[f64]) -> Result<Vec<f64>, EngineError> {
        let mut bindings: BTreeMap<String, f64> = self
            .parameter_names
            .iter()
            .cloned()
            .zip(parameters.iter().copied())
            .collect();
        let mut residual = Vec::with_capacity(self.points.len());
        for (point, target) in self.points.iter().zip(self.targets.iter()) {
            self.ctx.check()?;
            bindings.insert("x".to_string(), *point);
            let predicted = evaluate_f64(
                self.expression,
                &bindings,
                &self.ctx.limits,
                &self.calls.as_table(),
            )?;
            if !predicted.is_finite() {
                return Err(EngineError::domain("model evaluated to a non-finite value"));
            }
            residual.push(predicted - target);
        }
        Ok(residual)
    }

    /// Central-difference Jacobian of the residual vector.
    fn jacobian(&self, parameters: &[f64]) -> Result<Vec<Vec<f64>>, EngineError> {
        let columns = parameters.len();
        let rows = self.points.len();
        let mut matrix = vec![vec![0.0f64; columns]; rows];
        for (column, &value) in parameters.iter().enumerate() {
            let step = 1e-6 * (1.0 + value.abs());
            let mut plus = parameters.to_vec();
            let mut minus = parameters.to_vec();
            plus[column] += step;
            minus[column] -= step;
            let plus_residual = self.residuals(&plus)?;
            let minus_residual = self.residuals(&minus)?;
            for (row, (up, down)) in plus_residual.iter().zip(minus_residual.iter()).enumerate() {
                matrix[row][column] = (up - down) / (2.0 * step);
            }
        }
        Ok(matrix)
    }

    fn solve(
        &self,
        initial: Vec<f64>,
        tolerance: f64,
        max_iterations: u64,
    ) -> Result<LmOutcome, EngineError> {
        let parameters_count = initial.len();
        let mut parameters = initial;
        let mut residual = self.residuals(&parameters)?;
        let mut cost = residual.iter().map(|value| value * value).sum::<f64>();
        let mut lambda = 1e-3f64;
        let mut iterations = 0u64;
        let mut converged = false;
        while iterations < max_iterations {
            iterations += 1;
            self.ctx.check()?;
            let jacobian = self.jacobian(&parameters)?;
            let mut gradient = vec![0.0f64; parameters_count];
            for (column, gradient_value) in gradient.iter_mut().enumerate() {
                let mut sum = 0.0;
                for (row, jacobian_row) in jacobian.iter().enumerate() {
                    sum += jacobian_row[column] * residual[row];
                }
                *gradient_value = sum;
            }
            let gradient_norm = gradient
                .iter()
                .fold(0.0f64, |acc, value| acc.max(value.abs()));
            if gradient_norm <= tolerance {
                converged = true;
                break;
            }
            let mut normal = vec![vec![0.0f64; parameters_count]; parameters_count];
            for (row_index, row) in normal.iter_mut().enumerate() {
                for (column, value) in row.iter_mut().enumerate() {
                    let mut sum = 0.0;
                    for jacobian_row in &jacobian {
                        sum += jacobian_row[row_index] * jacobian_row[column];
                    }
                    *value = sum;
                }
            }
            let mut accepted = false;
            for _ in 0..40 {
                let mut damped = normal.clone();
                for (index, row) in damped.iter_mut().enumerate() {
                    let diagonal = normal[index][index];
                    row[index] += lambda * if diagonal > 0.0 { diagonal } else { 1.0 };
                }
                let right_hand_side: Vec<f64> = gradient.iter().map(|value| -value).collect();
                let Some(step) = solve_linear_system(damped, right_hand_side) else {
                    lambda *= 10.0;
                    if lambda > 1e12 {
                        break;
                    }
                    continue;
                };
                let trial: Vec<f64> = parameters
                    .iter()
                    .zip(step.iter())
                    .map(|(value, delta)| value + delta)
                    .collect();
                let trial_residual = match self.residuals(&trial) {
                    Ok(value) => value,
                    Err(_) => {
                        lambda *= 10.0;
                        if lambda > 1e12 {
                            break;
                        }
                        continue;
                    }
                };
                let trial_cost = trial_residual
                    .iter()
                    .map(|value| value * value)
                    .sum::<f64>();
                if trial_cost < cost {
                    let step_norm = step.iter().fold(0.0f64, |acc, value| acc.max(value.abs()));
                    let parameter_norm =
                        trial.iter().fold(0.0f64, |acc, value| acc.max(value.abs()));
                    parameters = trial;
                    residual = trial_residual;
                    cost = trial_cost;
                    lambda = (lambda * 0.1).max(1e-12);
                    accepted = true;
                    if step_norm <= tolerance * (1.0 + parameter_norm) || cost <= f64::MIN_POSITIVE
                    {
                        converged = true;
                    }
                    break;
                }
                lambda *= 10.0;
                if lambda > 1e12 {
                    break;
                }
            }
            if converged || !accepted {
                break;
            }
        }
        if !converged {
            return Err(EngineError::new(
                ErrorCode::NonConvergence,
                format!(
                    "levenberg_marquardt did not converge within {iterations} iteration(s) \
                     (residual sum of squares {cost})"
                ),
            ));
        }
        Ok(LmOutcome {
            parameters,
            residual_sum_squares: cost,
            iterations,
        })
    }
}

/// Dense Gaussian elimination with partial pivoting.
fn solve_linear_system(mut matrix: Vec<Vec<f64>>, mut rhs: Vec<f64>) -> Option<Vec<f64>> {
    let size = rhs.len();
    for column in 0..size {
        let mut pivot = column;
        for row in column + 1..size {
            if matrix[row][column].abs() > matrix[pivot][column].abs() {
                pivot = row;
            }
        }
        if !matrix[pivot][column].is_finite() || matrix[pivot][column].abs() < 1e-300 {
            return None;
        }
        matrix.swap(column, pivot);
        rhs.swap(column, pivot);
        let diagonal = matrix[column][column];
        let pivot_row = matrix[column].clone();
        for row in column + 1..size {
            let factor = matrix[row][column] / diagonal;
            if factor == 0.0 {
                continue;
            }
            for (value, pivot_value) in matrix[row][column..]
                .iter_mut()
                .zip(pivot_row[column..].iter())
            {
                *value -= factor * pivot_value;
            }
            rhs[row] -= factor * rhs[column];
        }
    }
    let mut solution = vec![0.0f64; size];
    for row in (0..size).rev() {
        let mut sum = rhs[row];
        for k in row + 1..size {
            sum -= matrix[row][k] * solution[k];
        }
        let diagonal = matrix[row][row];
        if diagonal == 0.0 {
            return None;
        }
        solution[row] = sum / diagonal;
    }
    Some(solution)
}

fn parse_parameter_names(args: &Args) -> Result<Vec<String>, EngineError> {
    let items = args.array("parameters")?;
    if items.is_empty() {
        return Err(EngineError::malformed("parameters must not be empty")
            .with_path("parameters".to_string()));
    }
    let mut names = Vec::with_capacity(items.len());
    let mut seen = BTreeSet::new();
    for (index, item) in items.iter().enumerate() {
        let path = format!("parameters[{index}]");
        let name = item
            .as_text()
            .map_err(|error| error.with_path(path.clone()))?;
        validate_identifier(name, &path)?;
        if name == "x" {
            return Err(EngineError::domain(
                "parameter name \"x\" is reserved for the data variable",
            )
            .with_path(path));
        }
        if !seen.insert(name.to_string()) {
            return Err(
                EngineError::domain(format!("duplicate parameter name {name:?}")).with_path(path),
            );
        }
        names.push(name.to_string());
    }
    Ok(names)
}

fn initial_parameters(args: &Args, count: usize) -> Result<Vec<f64>, EngineError> {
    match args.get("initial") {
        None | Some(Value::Null) => Ok(vec![0.0; count]),
        Some(value) => {
            let items = value
                .as_array()
                .map_err(|error| error.with_path("initial".to_string()))?;
            if items.len() != count {
                return Err(EngineError::domain(format!(
                    "initial has {} value(s) but {count} parameter(s) were declared",
                    items.len()
                ))
                .with_path("initial".to_string()));
            }
            let mut out = Vec::with_capacity(count);
            for (index, item) in items.iter().enumerate() {
                let path = format!("initial[{index}]");
                let number = item
                    .as_number()
                    .map_err(|error| error.with_path(path.clone()))?;
                out.push(finite_f64(number, &path)?);
            }
            Ok(out)
        }
    }
}

fn least_squares_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required(
                "parameters",
                ValueSchema::Record {
                    fields: Vec::new(),
                    allow_extra: true,
                },
            )
            .with_description("Record mapping each parameter name to its fitted float64 value."),
            FieldSchema::required("residual_sum_squares", float64_schema())
                .with_description("Sum of squared residuals at the fitted parameters."),
            FieldSchema::required("iterations", integer_schema())
                .with_description("Number of Levenberg-Marquardt iterations performed."),
            FieldSchema::required("converged", ValueSchema::Bool)
                .with_description("Always true for a successful result."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"levenberg_marquardt\"."),
        ],
        allow_extra: false,
    }
}

fn least_squares_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "optimize.nonlinear_least_squares",
        MODULE,
        VERSION,
        "Nonlinear least squares",
        "Fit a nonlinear model to data with the Levenberg-Marquardt method.",
    )
    .with_description(
        "Minimizes the sum of squared residuals between the model expression and the supplied \
         targets. The model is a restricted expression in the declared parameter names plus \
         the data variable x; the same restricted grammar and function set as \
         optimize.minimize_1d applies. Derivatives are computed with central differences and \
         the normal equations are damped and solved with partial-pivoted Gaussian \
         elimination. Non-convergence is reported as an error; partial fits are never \
         returned as success. The default initial guess is zero for every parameter.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "model",
            "Restricted model expression in the parameter names and x.",
            ValueSchema::Expression {
                variables: vec!["x".to_string()],
            },
        ),
        ParamDescriptor::required(
            "parameters",
            "Parameter names used by the model.",
            ValueSchema::array_with_len(ValueSchema::text(), 1, None),
        ),
        ParamDescriptor::required(
            "points",
            "Data abscissae.",
            ValueSchema::array_with_len(any_number_schema(), 1, None),
        ),
        ParamDescriptor::required(
            "targets",
            "Observed targets, one per point.",
            ValueSchema::array_with_len(any_number_schema(), 1, None),
        ),
        ParamDescriptor::optional(
            "initial",
            "Initial parameter guesses; default zeros.",
            ValueSchema::array(any_number_schema()),
        ),
        ParamDescriptor::optional(
            "tolerance",
            "Convergence tolerance; default 1e-10.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "max_iterations",
            "Iteration cap; default 100, never above the context limit.",
            integer_schema(),
        ),
    ])
    .with_output(
        least_squares_output_schema(),
        "Fitted parameters, residual sum of squares, iteration count, convergence flag, and method.",
    )
    .with_modes(scientific_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/optimize.md#nonlinear_least_squares")
    .with_examples(vec![
        Example::new(
            "fit a*x + b to (0,1), (1,2), (2,3)",
            example_args(&[
                ("model", serde_json::json!("a*x + b")),
                ("parameters", serde_json::json!(["a", "b"])),
                ("points", serde_json::json!([0, 1, 2])),
                ("targets", serde_json::json!([1, 2, 3])),
            ]),
        )
        .with_value(parse_value(serde_json::json!({
            "parameters": {
                "a": {"kind": "float64", "value": "0.9999999999900482"},
                "b": {"kind": "float64", "value": "1.0000000000124378"}
            },
            "residual_sum_squares": {
                "kind": "float64",
                "value": "0.00000000000000000000021662140262708086"
            },
            "iterations": {"kind": "integer", "value": "4"},
            "converged": true,
            "method": "levenberg_marquardt"
        }))),
    ])
}

fn invoke_nonlinear_least_squares(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "optimize.nonlinear_least_squares")?;
    let source = args.text("model")?;
    let parameter_names = parse_parameter_names(args)?;
    let points = number_array_f64(args, "points")?;
    let targets = number_array_f64(args, "targets")?;
    if points.len() != targets.len() {
        return Err(EngineError::malformed(format!(
            "points has {} value(s) but targets has {}",
            points.len(),
            targets.len()
        ))
        .with_path("targets".to_string()));
    }
    if points.len() < parameter_names.len() {
        return Err(EngineError::new(
            ErrorCode::InsufficientObservations,
            format!(
                "{} observation(s) cannot identify {} parameter(s)",
                points.len(),
                parameter_names.len()
            ),
        )
        .with_path("points".to_string()));
    }
    let initial = initial_parameters(args, parameter_names.len())?;
    let tolerance = positive_tolerance(args, "tolerance", 1e-10)?;
    let max_iterations =
        optional_positive_u64(args, "max_iterations", 100)?.min(ctx.limits.max_iterations);
    let expression = parse_expression(source, &ctx.limits)?;
    let problem = LeastSquares {
        expression: &expression,
        parameter_names: &parameter_names,
        points: &points,
        targets: &targets,
        calls: optimization_calls(),
        ctx,
    };
    let result = problem.solve(initial, tolerance, max_iterations)?;
    let mut parameters = Vec::with_capacity(parameter_names.len());
    for (name, value) in parameter_names.iter().zip(result.parameters.iter()) {
        parameters.push((name.clone(), float_value(*value)?));
    }
    let value = Value::record([
        ("parameters", Value::record(parameters)),
        (
            "residual_sum_squares",
            float_value(result.residual_sum_squares)?,
        ),
        ("iterations", integer_value(result.iterations)),
        ("converged", Value::Bool(true)),
        ("method", Value::text("levenberg_marquardt")),
    ]);
    Ok(Outcome::approximate(value))
}

// ---------------------------------------------------------------------------
// Linear assignment: exact Hungarian algorithm
// ---------------------------------------------------------------------------

fn hungarian(
    cost: &[Vec<BigRational>],
    ctx: &ExecContext,
) -> Result<(Vec<usize>, BigRational), EngineError> {
    let size = cost.len();
    let mut u = vec![BigRational::zero(); size + 1];
    let mut v = vec![BigRational::zero(); size + 1];
    let mut p = vec![0usize; size + 1];
    let mut way = vec![0usize; size + 1];
    for i in 1..=size {
        ctx.check()?;
        p[0] = i;
        let mut j0 = 0usize;
        let mut minv: Vec<Option<BigRational>> = vec![None; size + 1];
        let mut used = vec![false; size + 1];
        loop {
            ctx.check()?;
            used[j0] = true;
            let i0 = p[j0];
            let mut delta: Option<BigRational> = None;
            let mut j1 = 0usize;
            for j in 1..=size {
                if used[j] {
                    continue;
                }
                let current = &cost[i0 - 1][j - 1] - &u[i0] - &v[j];
                let improves = match &minv[j] {
                    None => true,
                    Some(existing) => current < *existing,
                };
                if improves {
                    minv[j] = Some(current.clone());
                    way[j] = j0;
                }
                let candidate = minv[j].as_ref().expect("minimum is set");
                let is_smaller = match &delta {
                    None => true,
                    Some(best) => candidate < best,
                };
                if is_smaller {
                    delta = Some(candidate.clone());
                    j1 = j;
                }
            }
            let delta = delta.expect("a complete bipartite graph always has an edge");
            for j in 0..=size {
                if used[j] {
                    u[p[j]] += &delta;
                    v[j] -= &delta;
                } else if let Some(value) = &mut minv[j] {
                    *value -= &delta;
                }
            }
            j0 = j1;
            if p[j0] == 0 {
                break;
            }
        }
        loop {
            let j1 = way[j0];
            p[j0] = p[j1];
            j0 = j1;
            if j0 == 0 {
                break;
            }
        }
    }
    let mut assignment = vec![0usize; size];
    for (column, &row) in p.iter().enumerate().skip(1) {
        assignment[row - 1] = column - 1;
    }
    let mut total = BigRational::zero();
    for (row, &column) in assignment.iter().enumerate() {
        total += &cost[row][column];
    }
    Ok((assignment, total))
}

fn assignment_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("assignment", ValueSchema::array(integer_schema()))
                .with_description("assignment[i] is the column assigned to row i."),
            FieldSchema::required("total_cost", ValueSchema::exact())
                .with_description("Exact sum of the assigned entries."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"hungarian\"."),
        ],
        allow_extra: false,
    }
}

fn linear_assignment_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "optimize.linear_assignment",
        MODULE,
        VERSION,
        "Linear assignment",
        "Solve the square linear assignment problem exactly with the Hungarian algorithm.",
    )
    .with_description(
        "Finds the minimum-cost perfect matching of a square cost matrix using the exact \
         rational Hungarian algorithm (shortest augmenting paths with potentials). The cost \
         matrix must be square; a non-square matrix is rejected. Costs may be any exact \
         numbers, including negative values, and the total cost is returned exactly.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "cost_matrix",
        "Square matrix of exact costs.",
        ValueSchema::array_with_len(
            ValueSchema::array_with_len(ValueSchema::exact(), 1, None),
            1,
            None,
        ),
    )])
    .with_output(
        assignment_output_schema(),
        "Optimal row-to-column assignment, exact total cost, and method.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Cubic)
    .with_method_ref("docs/methods/optimize.md#linear_assignment")
    .with_examples(vec![
        Example::new(
            "3x3 assignment",
            example_args(&[(
                "cost_matrix",
                serde_json::json!([[4, 1, 3], [2, 0, 5], [3, 2, 2]]),
            )]),
        )
        .with_value(parse_value(serde_json::json!({
            "assignment": [
                {"kind": "integer", "value": "1"},
                {"kind": "integer", "value": "0"},
                {"kind": "integer", "value": "2"}
            ],
            "total_cost": {"kind": "integer", "value": "5"},
            "method": "hungarian"
        }))),
    ])
}

fn invoke_linear_assignment(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    let rows = args.array("cost_matrix")?;
    let size = rows.len();
    if size == 0 {
        return Err(EngineError::domain("cost_matrix must not be empty")
            .with_path("cost_matrix".to_string()));
    }
    if size.saturating_mul(size) > ctx.limits.max_matrix_elements {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "a {size}x{size} cost matrix exceeds the {} element limit",
                ctx.limits.max_matrix_elements
            ),
        ));
    }
    let mut matrix = Vec::with_capacity(size);
    for (row_index, row) in rows.iter().enumerate() {
        let items = row
            .as_array()
            .map_err(|error| error.with_path(format!("cost_matrix[{row_index}]")))?;
        if items.len() != size {
            return Err(EngineError::domain(format!(
                "cost_matrix must be square: row {row_index} has {} entries, expected {size}",
                items.len()
            ))
            .with_path(format!("cost_matrix[{row_index}]")));
        }
        let mut values = Vec::with_capacity(size);
        for (column, item) in items.iter().enumerate() {
            values.push(exact_rational(
                item,
                &format!("cost_matrix[{row_index}][{column}]"),
            )?);
        }
        matrix.push(values);
    }
    let (assignment, total_cost) = hungarian(&matrix, ctx)?;
    let assignment_value = Value::Array(
        assignment
            .iter()
            .map(|column| Value::Number(Number::Integer(BigInt::from(*column))))
            .collect(),
    );
    let value = Value::record([
        ("assignment", assignment_value),
        ("total_cost", rational_value(&total_cost)),
        ("method", Value::text("hungarian")),
    ]);
    Ok(Outcome::exact(value))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Build the optimization module with all of its registered functions.
pub fn module() -> Module {
    let functions: Vec<Arc<dyn Function>> = vec![
        SimpleFunction::arc(linear_program_descriptor(), invoke_linear_program),
        SimpleFunction::arc(minimize_1d_descriptor(), invoke_minimize_1d),
        SimpleFunction::arc(golden_section_descriptor(), invoke_golden_section),
        SimpleFunction::arc(least_squares_descriptor(), invoke_nonlinear_least_squares),
        SimpleFunction::arc(linear_assignment_descriptor(), invoke_linear_assignment),
    ];
    let descriptor = ModuleDescriptor::new(
        "optimize",
        "Optimization",
        "1.0.0",
        "Exact rational linear programming, bounded univariate minimization, nonlinear \
         least squares, and exact linear assignment.",
    )
    .with_capabilities(vec![
        "linear_programming",
        "univariate_minimization",
        "nonlinear_least_squares",
        "linear_assignment",
    ])
    .with_dependencies(vec!["core"])
    .with_modes(all_modes())
    .with_source("crates/bicmath-optimize");
    Module::new(descriptor, functions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bicmath_core::contract::ExampleExpectation;
    use bicmath_core::envelope::Exactness;

    fn call_with(
        id: &str,
        raw: serde_json::Value,
        ctx: ExecContext,
    ) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|function| function.descriptor().id == id)
            .unwrap_or_else(|| panic!("function {id} is not registered"));
        let arguments = raw.as_object().expect("arguments must be an object");
        let mut values = BTreeMap::new();
        for (name, raw_value) in arguments {
            let parameter = function
                .descriptor()
                .parameter(name)
                .unwrap_or_else(|| panic!("parameter {name} is not declared"));
            let value = parameter
                .schema
                .coerce(raw_value, name, &ctx.limits, true)
                .unwrap_or_else(|error| panic!("argument {name} does not coerce: {error}"));
            values.insert(name.clone(), value);
        }
        function.invoke(&Args::new(values), &ctx)
    }

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = module();
        let function = module
            .functions
            .iter()
            .find(|function| function.descriptor().id == id)
            .unwrap_or_else(|| panic!("function {id} is not registered"));
        let ctx = if function.descriptor().modes.contains(&NumericMode::Auto) {
            ExecContext::conservative()
        } else {
            ExecContext::scientific()
        };
        call_with(id, raw, ctx)
    }

    fn record_result(outcome: &Outcome) -> &BTreeMap<String, Value> {
        match &outcome.value {
            Value::Record(fields) => fields,
            other => panic!("expected a record result, got {other:?}"),
        }
    }

    fn record_f64(fields: &BTreeMap<String, Value>, name: &str) -> f64 {
        match fields.get(name) {
            Some(Value::Number(Number::Float64(value))) => value.get(),
            other => panic!("expected float64 field {name}, got {other:?}"),
        }
    }

    fn record_text<'a>(fields: &'a BTreeMap<String, Value>, name: &str) -> &'a str {
        match fields.get(name) {
            Some(Value::Text(value)) => value,
            other => panic!("expected text field {name}, got {other:?}"),
        }
    }

    fn exact_int(value: i64) -> Value {
        Value::Number(Number::Integer(BigInt::from(value)))
    }

    fn exact_ratio(numerator: i64, denominator: i64) -> Value {
        Value::Number(Number::Rational(BigRational::new(
            BigInt::from(numerator),
            BigInt::from(denominator),
        )))
    }

    fn max_program() -> serde_json::Value {
        serde_json::json!({
            "objective": [3, 5],
            "constraints": [
                {"coefficients": [1, 0], "relation": "le", "rhs": 4},
                {"coefficients": [0, 2], "relation": "le", "rhs": 12},
                {"coefficients": [3, 2], "relation": "le", "rhs": 18}
            ]
        })
    }

    // -- Linear programming -------------------------------------------------

    #[test]
    fn linear_program_maximizes_exactly() {
        let outcome = call("optimize.linear_program", max_program()).unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        let fields = record_result(&outcome);
        assert_eq!(fields.get("status"), Some(&Value::text("optimal")));
        assert_eq!(fields.get("objective"), Some(&exact_int(36)));
        assert_eq!(
            fields.get("x"),
            Some(&Value::Array(vec![exact_int(2), exact_int(6)]))
        );
        assert_eq!(
            fields.get("dual"),
            Some(&Value::Array(vec![
                exact_int(0),
                exact_ratio(3, 2),
                exact_int(1)
            ]))
        );
        assert_eq!(
            fields.get("reduced_costs"),
            Some(&Value::Array(vec![exact_int(0), exact_int(0)]))
        );
        assert_eq!(record_text(fields, "method"), "rational_simplex_bland");
    }

    #[test]
    fn linear_program_detects_infeasibility() {
        let outcome = call(
            "optimize.linear_program",
            serde_json::json!({
                "objective": [1, 1],
                "constraints": [
                    {"coefficients": [1, 1], "relation": "le", "rhs": 1},
                    {"coefficients": [1, 1], "relation": "ge", "rhs": 3}
                ]
            }),
        )
        .unwrap();
        let fields = record_result(&outcome);
        assert_eq!(record_text(fields, "status"), "infeasible");
        assert!(!fields.contains_key("objective"));
        assert!(!fields.contains_key("x"));
        assert!(!fields.contains_key("dual"));
        assert!(!fields.contains_key("reduced_costs"));
    }

    #[test]
    fn linear_program_detects_unboundedness() {
        let outcome = call(
            "optimize.linear_program",
            serde_json::json!({
                "objective": [1, 1],
                "constraints": [
                    {"coefficients": [1, 0], "relation": "le", "rhs": 1}
                ]
            }),
        )
        .unwrap();
        let fields = record_result(&outcome);
        assert_eq!(record_text(fields, "status"), "unbounded");
        assert!(!fields.contains_key("x"));
        assert!(!fields.contains_key("objective"));
    }

    #[test]
    fn linear_program_handles_degenerate_ties() {
        let outcome = call(
            "optimize.linear_program",
            serde_json::json!({
                "objective": [1, 1],
                "constraints": [
                    {"coefficients": [1, 1], "relation": "le", "rhs": 1},
                    {"coefficients": [1, 0], "relation": "le", "rhs": 1},
                    {"coefficients": [0, 1], "relation": "le", "rhs": 1}
                ]
            }),
        )
        .unwrap();
        let fields = record_result(&outcome);
        assert_eq!(record_text(fields, "status"), "optimal");
        assert_eq!(fields.get("objective"), Some(&exact_int(1)));
        assert_eq!(
            fields.get("x"),
            Some(&Value::Array(vec![exact_int(1), exact_int(0)]))
        );
    }

    #[test]
    fn linear_program_minimizes_and_handles_equalities() {
        let outcome = call(
            "optimize.linear_program",
            serde_json::json!({
                "objective": [2, 1],
                "sense": "min",
                "constraints": [
                    {"coefficients": [1, 1], "relation": "ge", "rhs": 1},
                    {"coefficients": [1, -1], "relation": "eq", "rhs": 0},
                    {"coefficients": [1, 0], "relation": "le", "rhs": 2},
                    {"coefficients": [0, 1], "relation": "le", "rhs": 2}
                ]
            }),
        )
        .unwrap();
        let fields = record_result(&outcome);
        assert_eq!(record_text(fields, "status"), "optimal");
        assert_eq!(fields.get("objective"), Some(&exact_ratio(3, 2)));
        assert_eq!(
            fields.get("x"),
            Some(&Value::Array(vec![exact_ratio(1, 2), exact_ratio(1, 2)]))
        );
        assert_eq!(
            fields.get("dual"),
            Some(&Value::Array(vec![
                exact_ratio(3, 2),
                exact_ratio(1, 2),
                exact_int(0),
                exact_int(0)
            ]))
        );
    }

    #[test]
    fn linear_program_supports_named_variables() {
        let outcome = call(
            "optimize.linear_program",
            serde_json::json!({
                "objective": [3, 5],
                "constraints": [
                    {"coefficients": [1, 0], "relation": "le", "rhs": 4},
                    {"coefficients": [0, 2], "relation": "le", "rhs": 12},
                    {"coefficients": [3, 2], "relation": "le", "rhs": 18}
                ],
                "variable_names": ["x", "y"]
            }),
        )
        .unwrap();
        let fields = record_result(&outcome);
        let x = fields
            .get("x")
            .expect("x is present")
            .as_record()
            .expect("x is a record");
        assert_eq!(x.get("x"), Some(&exact_int(2)));
        assert_eq!(x.get("y"), Some(&exact_int(6)));
    }

    #[test]
    fn linear_program_normalizes_negative_rhs() {
        let outcome = call(
            "optimize.linear_program",
            serde_json::json!({
                "objective": [-1],
                "constraints": [
                    {"coefficients": [-1], "relation": "le", "rhs": -1}
                ]
            }),
        )
        .unwrap();
        let fields = record_result(&outcome);
        assert_eq!(record_text(fields, "status"), "optimal");
        assert_eq!(fields.get("objective"), Some(&exact_int(-1)));
        assert_eq!(fields.get("x"), Some(&Value::Array(vec![exact_int(1)])));
        assert_eq!(fields.get("dual"), Some(&Value::Array(vec![exact_int(1)])));
    }

    #[test]
    fn linear_program_returns_exact_rational_vertices() {
        let outcome = call(
            "optimize.linear_program",
            serde_json::json!({
                "objective": [1, 1],
                "constraints": [
                    {"coefficients": [2, 1], "relation": "le", "rhs": 1},
                    {"coefficients": [1, 2], "relation": "le", "rhs": 1}
                ]
            }),
        )
        .unwrap();
        let fields = record_result(&outcome);
        assert_eq!(fields.get("objective"), Some(&exact_ratio(2, 3)));
        assert_eq!(
            fields.get("x"),
            Some(&Value::Array(vec![exact_ratio(1, 3), exact_ratio(1, 3)]))
        );
    }

    // -- Univariate minimization --------------------------------------------

    #[test]
    fn brent_minimizes_quadratic() {
        let outcome = call(
            "optimize.minimize_1d",
            serde_json::json!({
                "expression": "(x - 3)^2",
                "variable": "x",
                "lower": 0,
                "upper": 10
            }),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        let fields = record_result(&outcome);
        assert!((record_f64(fields, "x") - 3.0).abs() <= 1e-6);
        assert!(record_f64(fields, "value").abs() <= 1e-12);
        assert_eq!(record_text(fields, "method"), "brent_minimization");
        assert_eq!(fields.get("converged"), Some(&Value::Bool(true)));
    }

    #[test]
    fn brent_reports_non_convergence() {
        let mut ctx = ExecContext::scientific();
        ctx.limits.max_iterations = 1;
        let error = call_with(
            "optimize.minimize_1d",
            serde_json::json!({
                "expression": "(x - 3)^2",
                "variable": "x",
                "lower": 0,
                "upper": 10
            }),
            ctx,
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::NonConvergence);
    }

    #[test]
    fn brent_rejects_unknown_variables() {
        let error = call(
            "optimize.minimize_1d",
            serde_json::json!({
                "expression": "y^2",
                "variable": "x",
                "lower": 0,
                "upper": 1
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    #[test]
    fn golden_section_minimizes_quadratic() {
        let outcome = call(
            "optimize.golden_section",
            serde_json::json!({
                "expression": "x^2 - 4*x + 7",
                "variable": "x",
                "lower": 0,
                "upper": 5
            }),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        let fields = record_result(&outcome);
        assert!((record_f64(fields, "x") - 2.0).abs() <= 1e-6);
        assert!((record_f64(fields, "value") - 3.0).abs() <= 1e-6);
        assert_eq!(record_text(fields, "method"), "golden_section");
    }

    // -- Nonlinear least squares --------------------------------------------

    #[test]
    fn levenberg_marquardt_fits_a_line() {
        let outcome = call(
            "optimize.nonlinear_least_squares",
            serde_json::json!({
                "model": "a*x + b",
                "parameters": ["a", "b"],
                "points": [0, 1, 2],
                "targets": [1, 2, 3]
            }),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Approximate);
        let fields = record_result(&outcome);
        let parameters = fields
            .get("parameters")
            .expect("parameters are present")
            .as_record()
            .expect("parameters are a record");
        let a = match parameters.get("a") {
            Some(Value::Number(Number::Float64(value))) => value.get(),
            other => panic!("expected float64 parameter a, got {other:?}"),
        };
        let b = match parameters.get("b") {
            Some(Value::Number(Number::Float64(value))) => value.get(),
            other => panic!("expected float64 parameter b, got {other:?}"),
        };
        assert!((a - 1.0).abs() <= 1e-8, "a = {a}");
        assert!((b - 1.0).abs() <= 1e-8, "b = {b}");
        assert!(record_f64(fields, "residual_sum_squares") <= 1e-16);
        assert_eq!(record_text(fields, "method"), "levenberg_marquardt");
        assert_eq!(fields.get("converged"), Some(&Value::Bool(true)));
    }

    #[test]
    fn levenberg_marquardt_honours_initial_guess() {
        let outcome = call(
            "optimize.nonlinear_least_squares",
            serde_json::json!({
                "model": "a*x + b",
                "parameters": ["a", "b"],
                "points": [0, 1, 2],
                "targets": [1, 2, 3],
                "initial": [2, -1]
            }),
        )
        .unwrap();
        let fields = record_result(&outcome);
        let parameters = fields.get("parameters").unwrap().as_record().unwrap();
        let a = match parameters.get("a") {
            Some(Value::Number(Number::Float64(value))) => value.get(),
            other => panic!("expected float64 parameter a, got {other:?}"),
        };
        assert!((a - 1.0).abs() <= 1e-8, "a = {a}");
    }

    // -- Linear assignment --------------------------------------------------

    #[test]
    fn hungarian_solves_a_3x3_assignment() {
        let outcome = call(
            "optimize.linear_assignment",
            serde_json::json!({"cost_matrix": [[4, 1, 3], [2, 0, 5], [3, 2, 2]]}),
        )
        .unwrap();
        assert_eq!(outcome.exactness, Exactness::Exact);
        let fields = record_result(&outcome);
        assert_eq!(
            fields.get("assignment"),
            Some(&Value::Array(vec![
                exact_int(1),
                exact_int(0),
                exact_int(2)
            ]))
        );
        assert_eq!(fields.get("total_cost"), Some(&exact_int(5)));
        assert_eq!(record_text(fields, "method"), "hungarian");
    }

    #[test]
    fn hungarian_handles_negative_exact_costs() {
        let outcome = call(
            "optimize.linear_assignment",
            serde_json::json!({"cost_matrix": [["0.5", "-1"], ["-1", "0.5"]]}),
        )
        .unwrap();
        let fields = record_result(&outcome);
        assert_eq!(fields.get("total_cost"), Some(&exact_int(-2)));
    }

    #[test]
    fn hungarian_rejects_non_square_matrices() {
        let error = call(
            "optimize.linear_assignment",
            serde_json::json!({"cost_matrix": [[1, 2, 3], [4, 5, 6]]}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }

    // -- Contract and documentation -----------------------------------------

    #[test]
    fn descriptor_contract_is_complete() {
        let module = module();
        assert_eq!(module.descriptor.id, "optimize");
        assert_eq!(module.descriptor.version, "1.0.0");
        assert_eq!(module.functions.len(), 5);
        let mut ids = BTreeSet::new();
        for function in &module.functions {
            let descriptor = function.descriptor();
            assert!(
                descriptor.id.starts_with("optimize."),
                "bad id {}",
                descriptor.id
            );
            assert_eq!(descriptor.module, "optimize");
            assert_eq!(descriptor.version, "1.0.0");
            assert!(
                descriptor
                    .method_ref
                    .starts_with("docs/methods/optimize.md#"),
                "bad method_ref {}",
                descriptor.method_ref
            );
            assert!(
                !descriptor.examples.is_empty(),
                "function {} has no executable example",
                descriptor.id
            );
            assert!(ids.insert(descriptor.id.clone()), "duplicate id");
        }
    }

    #[test]
    fn every_example_executes_and_matches() {
        let module = module();
        for function in &module.functions {
            let descriptor = function.descriptor();
            let ctx = if descriptor.modes.contains(&NumericMode::Auto) {
                ExecContext::conservative()
            } else {
                ExecContext::scientific()
            };
            for example in &descriptor.examples {
                let mut values = BTreeMap::new();
                for (name, raw) in &example.arguments {
                    let parameter = descriptor
                        .parameter(name)
                        .unwrap_or_else(|| panic!("example parameter {name} is not declared"));
                    parameter
                        .schema
                        .validate(raw, name, &ctx.limits)
                        .unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} has an invalid {name}: {error}",
                                example.title, descriptor.id
                            )
                        });
                    values.insert(name.clone(), raw.clone());
                }
                let result = function.invoke(&Args::new(values), &ctx);
                match &example.expected {
                    Some(ExampleExpectation::Value(expected)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} failed: {error}",
                                example.title, descriptor.id
                            )
                        });
                        assert_eq!(
                            &outcome.value, expected,
                            "example {:?} of {} produced an unexpected value",
                            example.title, descriptor.id
                        );
                    }
                    Some(ExampleExpectation::Error(code)) => {
                        let error = match result {
                            Err(error) => error,
                            Ok(outcome) => panic!(
                                "example {:?} of {} expected {code:?}, produced {:?}",
                                example.title, descriptor.id, outcome.value
                            ),
                        };
                        assert_eq!(
                            error.code, *code,
                            "example {:?} of {} produced the wrong error: {}",
                            example.title, descriptor.id, error.message
                        );
                    }
                    Some(ExampleExpectation::Contains(text)) => {
                        let outcome = result.unwrap_or_else(|error| {
                            panic!(
                                "example {:?} of {} failed: {error}",
                                example.title, descriptor.id
                            )
                        });
                        assert!(
                            format!("{:?}", outcome.value).contains(text),
                            "example {:?} of {} does not contain {text:?}",
                            example.title,
                            descriptor.id
                        );
                    }
                    None => {
                        let _ = result;
                    }
                }
            }
        }
    }
}

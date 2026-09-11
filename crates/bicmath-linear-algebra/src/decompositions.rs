//! Iterative spectral methods: Jacobi eigenvalues for real symmetric matrices,
//! one-sided Jacobi SVD, and power iteration for a dominant eigenpair.
//!
//! All three methods are scientific-only. Exact inputs are converted explicitly
//! to binary64 in scientific mode, and non-convergence is reported as an error
//! instead of returning an unconverged iterate as a success.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::Number;
use bicmath_core::schema::{FieldSchema, NumberKind, ValueSchema};
use bicmath_core::value::Value;
use num_traits::ToPrimitive;

use crate::{
    Matrix, Spec, descriptor, example_args, float_value, float_vector_value, matrix_from_floats,
    matrix_schema, num_schema, require_scientific, scientific_matrix_floats, scientific_modes,
};

const DEFAULT_TOLERANCE: f64 = 1e-12;
const DEFAULT_MAX_SWEEPS: u64 = 100;
const DEFAULT_MAX_ITERATIONS: u64 = 1000;

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

fn positive_count(args: &Args, name: &str, default: u64) -> Result<u64, EngineError> {
    match args.optional_integer(name)? {
        None => Ok(default),
        Some(value) => value.to_u64().filter(|value| *value > 0).ok_or_else(|| {
            EngineError::domain(format!("{name} must be a positive integer"))
                .with_path(name.to_string())
        }),
    }
}

fn enforce_budget(function: &str, estimate: u64, ctx: &ExecContext) -> Result<(), EngineError> {
    if estimate > ctx.limits.max_operations {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "{function} needs an estimated {estimate} operations, exceeding the limit of {}",
                ctx.limits.max_operations
            ),
        ));
    }
    Ok(())
}

fn iteration_cap(function: &str, requested: u64, ctx: &ExecContext) -> Result<usize, EngineError> {
    if requested > ctx.limits.max_iterations {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "{function} requested {requested} iterations, exceeding the limit of {}",
                ctx.limits.max_iterations
            ),
        ));
    }
    Ok(requested as usize)
}

// ---------------------------------------------------------------------------
// Symmetric Jacobi eigenvalue algorithm
// ---------------------------------------------------------------------------

struct EigenResult {
    values: Vec<f64>,
    vectors: Vec<f64>,
    sweeps: usize,
    residual_norm: f64,
}

fn off_diagonal_norm(a: &[f64], n: usize) -> f64 {
    let mut sum = 0.0f64;
    for i in 0..n {
        for j in (i + 1)..n {
            let value = a[i * n + j];
            sum += value * value;
        }
    }
    (2.0 * sum).sqrt()
}

fn frobenius_norm(a: &[f64], n: usize) -> f64 {
    let mut sum = 0.0f64;
    for value in a.iter().take(n * n) {
        sum += value * value;
    }
    sum.sqrt()
}

fn jacobi_eigen(
    a: &[f64],
    n: usize,
    tolerance: f64,
    max_sweeps: usize,
    ctx: &ExecContext,
) -> Result<EigenResult, EngineError> {
    let mut a = a.to_vec();
    let mut vectors = vec![0.0f64; n * n];
    for i in 0..n {
        vectors[i * n + i] = 1.0;
    }
    let mut sweeps = 0usize;
    let mut residual_norm = off_diagonal_norm(&a, n);
    let mut converged = residual_norm == 0.0;
    while !converged && sweeps < max_sweeps {
        ctx.check()?;
        sweeps += 1;
        let threshold = tolerance * frobenius_norm(&a, n);
        let mut rotated = false;
        for p in 0..n {
            ctx.check()?;
            for q in (p + 1)..n {
                let apq = a[p * n + q];
                if apq == 0.0 || apq.abs() <= threshold {
                    continue;
                }
                rotated = true;
                let app = a[p * n + p];
                let aqq = a[q * n + q];
                let tau = (aqq - app) / (2.0 * apq);
                let t = if tau >= 0.0 {
                    1.0 / (tau + (1.0 + tau * tau).sqrt())
                } else {
                    -1.0 / (-tau + (1.0 + tau * tau).sqrt())
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;
                for k in 0..n {
                    if k == p || k == q {
                        continue;
                    }
                    let akp = a[k * n + p];
                    let akq = a[k * n + q];
                    a[k * n + p] = c * akp - s * akq;
                    a[p * n + k] = a[k * n + p];
                    a[k * n + q] = s * akp + c * akq;
                    a[q * n + k] = a[k * n + q];
                }
                a[p * n + p] = c * c * app - 2.0 * s * c * apq + s * s * aqq;
                a[q * n + q] = s * s * app + 2.0 * s * c * apq + c * c * aqq;
                a[p * n + q] = 0.0;
                a[q * n + p] = 0.0;
                for k in 0..n {
                    let vkp = vectors[k * n + p];
                    let vkq = vectors[k * n + q];
                    vectors[k * n + p] = c * vkp - s * vkq;
                    vectors[k * n + q] = s * vkp + c * vkq;
                }
            }
        }
        residual_norm = off_diagonal_norm(&a, n);
        converged = !rotated || residual_norm <= tolerance * frobenius_norm(&a, n);
    }
    if !converged {
        return Err(EngineError::new(
            ErrorCode::NonConvergence,
            format!(
                "eigen_symmetric did not converge within {max_sweeps} sweeps; \
                 off-diagonal residual norm is {residual_norm:.3e}"
            ),
        )
        .with_details(serde_json::json!({
            "sweeps": sweeps,
            "residual_norm": residual_norm,
        })));
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|left, right| a[left * n + left].total_cmp(&a[right * n + right]));
    let values: Vec<f64> = order.iter().map(|i| a[i * n + i]).collect();
    let mut sorted_vectors = vec![0.0f64; n * n];
    for (new_column, old_column) in order.iter().enumerate() {
        for row in 0..n {
            sorted_vectors[row * n + new_column] = vectors[row * n + *old_column];
        }
    }
    Ok(EigenResult {
        values,
        vectors: sorted_vectors,
        sweeps,
        residual_norm,
    })
}

fn eigen_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("values", ValueSchema::array(ValueSchema::float64()))
                .with_description("Eigenvalues in ascending order."),
            FieldSchema::required("vectors", matrix_schema(false)).with_description(
                "Matrix whose columns are the corresponding orthonormal eigenvectors.",
            ),
            FieldSchema::required("sweeps", ValueSchema::number(NumberKind::Integer))
                .with_description("Jacobi sweeps performed."),
            FieldSchema::required("converged", ValueSchema::Bool)
                .with_description("True for a successful result."),
            FieldSchema::required("residual_norm", ValueSchema::float64())
                .with_description("Off-diagonal Frobenius norm at exit."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"jacobi_rotation\"."),
        ],
        allow_extra: false,
    }
}

fn eigen_descriptor() -> FunctionDescriptor {
    descriptor(Spec {
        id: "linear_algebra.eigen_symmetric",
        title: "Symmetric eigenvalues",
        summary: "Eigenvalues and orthonormal eigenvectors of a real symmetric matrix.",
        description: "Cyclic Jacobi rotations diagonalize a real symmetric matrix. Eigenvalues are \
             returned in ascending order and the matching orthonormal eigenvectors are the \
             columns of the vectors matrix, so A*V = V*diag(values). Scientific mode only: \
             exact inputs are converted explicitly to float64 in that mode. A matrix that is \
             not symmetric within the tolerance is rejected. Non-convergence within max_sweeps \
             is reported as non_convergence. Defaults: tolerance 1e-12 and 100 sweeps.",
        params: vec![
            ParamDescriptor::required(
                "matrix",
                "Real symmetric square matrix.",
                matrix_schema(true),
            ),
            ParamDescriptor::optional(
                "tolerance",
                "Off-diagonal convergence tolerance relative to the Frobenius norm; default 1e-12.",
                num_schema(),
            ),
            ParamDescriptor::optional(
                "max_sweeps",
                "Maximum Jacobi sweeps; default 100, never above the context iteration limit.",
                ValueSchema::number(NumberKind::Integer),
            ),
        ],
        output: eigen_output_schema(),
        output_description: "Record with values, vectors, sweeps, converged, residual_norm, method.",
        modes: scientific_modes(),
        cost: CostClass::Iterative,
        method: "docs/methods/linear_algebra.md#eigen-symmetric",
        examples: vec![Example::new(
            "symmetric 2x2 matrix",
            example_args(&[(
                "matrix",
                serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [2, 1, 1, 2]}),
            )]),
        )
        .with_contains("jacobi_rotation")],
    })
}

pub(crate) fn invoke_eigen_symmetric(
    args: &Args,
    ctx: &ExecContext,
) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "linear_algebra.eigen_symmetric")?;
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    if matrix.rows != matrix.cols {
        return Err(EngineError::malformed(
            "eigen_symmetric requires a square matrix",
        ));
    }
    let tolerance = positive_tolerance(args, "tolerance", DEFAULT_TOLERANCE)?;
    let max_sweeps = positive_count(args, "max_sweeps", DEFAULT_MAX_SWEEPS)?;
    let max_sweeps = iteration_cap("linear_algebra.eigen_symmetric", max_sweeps, ctx)?;
    let n = matrix.rows;
    enforce_budget(
        "linear_algebra.eigen_symmetric",
        (n as u64)
            .saturating_mul(n as u64)
            .saturating_mul(max_sweeps as u64),
        ctx,
    )?;
    let a = scientific_matrix_floats(&matrix, "eigen_symmetric")?;
    let scale = a.iter().fold(1.0f64, |max, value| max.max(value.abs()));
    for i in 0..n {
        for j in (i + 1)..n {
            if (a[i * n + j] - a[j * n + i]).abs() > tolerance * scale {
                return Err(EngineError::domain(
                    "eigen_symmetric requires a real symmetric matrix",
                ));
            }
        }
    }
    let result = jacobi_eigen(&a, n, tolerance, max_sweeps, ctx)?;
    let value = Value::record([
        ("values", float_vector_value(&result.values)?),
        (
            "vectors",
            matrix_from_floats(n, n, result.vectors)?.to_value(),
        ),
        ("sweeps", Value::Number(Number::integer(result.sweeps))),
        ("converged", Value::Bool(true)),
        ("residual_norm", float_value(result.residual_norm)?),
        ("method", Value::text("jacobi_rotation")),
    ]);
    Ok(Outcome::approximate(value))
}

// ---------------------------------------------------------------------------
// One-sided Jacobi SVD
// ---------------------------------------------------------------------------

pub(crate) struct SvdResult {
    /// Left singular vectors, `m x k` with `k = min(m, n)`.
    pub(crate) u: Vec<f64>,
    /// Singular values in descending order.
    pub(crate) s: Vec<f64>,
    /// Right singular vectors, `n x k`.
    pub(crate) v: Vec<f64>,
    pub(crate) sweeps: usize,
}

/// One-sided Jacobi SVD for a tall matrix (`m >= n`). Returns `u` as `m x n`,
/// `s` as length `n`, and `v` as `n x n`; columns of `b = a * v` are
/// orthogonalized in place.
fn one_sided_jacobi(
    a: &[f64],
    m: usize,
    n: usize,
    tolerance: f64,
    max_sweeps: usize,
    ctx: &ExecContext,
) -> Result<SvdResult, EngineError> {
    let mut b = a.to_vec();
    let mut v = vec![0.0f64; n * n];
    for i in 0..n {
        v[i * n + i] = 1.0;
    }
    let mut sweeps = 0usize;
    let mut converged = false;
    while !converged && sweeps < max_sweeps {
        ctx.check()?;
        sweeps += 1;
        converged = true;
        for p in 0..n.saturating_sub(1) {
            ctx.check()?;
            for q in (p + 1)..n {
                let mut alpha = 0.0f64;
                let mut beta = 0.0f64;
                let mut gamma = 0.0f64;
                for i in 0..m {
                    let bp = b[i * n + p];
                    let bq = b[i * n + q];
                    alpha += bp * bp;
                    beta += bq * bq;
                    gamma += bp * bq;
                }
                if gamma == 0.0 || gamma.abs() <= tolerance * (alpha * beta).sqrt() {
                    continue;
                }
                converged = false;
                let zeta = (beta - alpha) / (2.0 * gamma);
                let t = if zeta >= 0.0 {
                    1.0 / (zeta + (1.0 + zeta * zeta).sqrt())
                } else {
                    -1.0 / (-zeta + (1.0 + zeta * zeta).sqrt())
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;
                for i in 0..m {
                    let bp = b[i * n + p];
                    let bq = b[i * n + q];
                    b[i * n + p] = c * bp - s * bq;
                    b[i * n + q] = s * bp + c * bq;
                }
                for i in 0..n {
                    let vp = v[i * n + p];
                    let vq = v[i * n + q];
                    v[i * n + p] = c * vp - s * vq;
                    v[i * n + q] = s * vp + c * vq;
                }
            }
        }
    }
    if !converged {
        return Err(EngineError::new(
            ErrorCode::NonConvergence,
            format!("svd did not converge within {max_sweeps} sweeps"),
        )
        .with_details(serde_json::json!({"sweeps": sweeps})));
    }
    let mut u = vec![0.0f64; m * n];
    let mut s = vec![0.0f64; n];
    for column in 0..n {
        let mut norm = 0.0f64;
        for row in 0..m {
            let value = b[row * n + column];
            norm += value * value;
        }
        norm = norm.sqrt();
        s[column] = norm;
        if norm > 0.0 {
            for row in 0..m {
                u[row * n + column] = b[row * n + column] / norm;
            }
        }
    }
    Ok(SvdResult { u, s, v, sweeps })
}

/// SVD for any `m x n` matrix. Wide inputs are handled by decomposing the
/// transpose and swapping the factors. Singular values are sorted descending.
pub(crate) fn svd_sorted(
    a: &[f64],
    m: usize,
    n: usize,
    tolerance: f64,
    max_sweeps: usize,
    ctx: &ExecContext,
) -> Result<SvdResult, EngineError> {
    let (u, s, v, sweeps) = if m >= n {
        let result = one_sided_jacobi(a, m, n, tolerance, max_sweeps, ctx)?;
        (result.u, result.s, result.v, result.sweeps)
    } else {
        let mut transpose = vec![0.0f64; n * m];
        for i in 0..m {
            for j in 0..n {
                transpose[j * m + i] = a[i * n + j];
            }
        }
        let result = one_sided_jacobi(&transpose, n, m, tolerance, max_sweeps, ctx)?;
        (result.v, result.s, result.u, result.sweeps)
    };
    let k = m.min(n);
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|left, right| s[*right].total_cmp(&s[*left]));
    let mut sorted_u = vec![0.0f64; m * k];
    let mut sorted_v = vec![0.0f64; n * k];
    let mut sorted_s = vec![0.0f64; k];
    for (new_column, old_column) in order.iter().enumerate() {
        sorted_s[new_column] = s[*old_column];
        for row in 0..m {
            sorted_u[row * k + new_column] = u[row * k + *old_column];
        }
        for row in 0..n {
            sorted_v[row * k + new_column] = v[row * k + *old_column];
        }
    }
    Ok(SvdResult {
        u: sorted_u,
        s: sorted_s,
        v: sorted_v,
        sweeps,
    })
}

fn svd_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("u", matrix_schema(false))
                .with_description("Left singular vectors (m x k)."),
            FieldSchema::required("s", ValueSchema::array(ValueSchema::float64()))
                .with_description("Singular values in descending order."),
            FieldSchema::required("v", matrix_schema(false))
                .with_description("Right singular vectors (n x k)."),
            FieldSchema::required("sweeps", ValueSchema::number(NumberKind::Integer))
                .with_description("Jacobi sweeps performed."),
            FieldSchema::required("converged", ValueSchema::Bool)
                .with_description("True for a successful result."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"one_sided_jacobi\"."),
        ],
        allow_extra: false,
    }
}

fn svd_descriptor() -> FunctionDescriptor {
    descriptor(Spec {
        id: "linear_algebra.svd",
        title: "Singular value decomposition",
        summary: "One-sided Jacobi SVD with singular values in descending order.",
        description:
            "Computes the economy SVD A = U*diag(s)*V^T with U (m x k), V (n x k), and \
             k = min(m, n). Columns of U and V are orthonormal; zero singular values yield \
             zero columns of U. Scientific mode only: exact inputs are converted explicitly \
             to float64 in that mode. Non-convergence within max_sweeps is reported as \
             non_convergence. Defaults: tolerance 1e-12 and 100 sweeps.",
        params: vec![
            ParamDescriptor::required("matrix", "Matrix to decompose.", matrix_schema(false)),
            ParamDescriptor::optional(
                "tolerance",
                "Column-orthogonality tolerance; default 1e-12.",
                num_schema(),
            ),
            ParamDescriptor::optional(
                "max_sweeps",
                "Maximum Jacobi sweeps; default 100, never above the context iteration limit.",
                ValueSchema::number(NumberKind::Integer),
            ),
        ],
        output: svd_output_schema(),
        output_description: "Record with u, s (descending), v, sweeps, converged, method.",
        modes: scientific_modes(),
        cost: CostClass::Iterative,
        method: "docs/methods/linear_algebra.md#svd",
        examples: vec![Example::new(
            "3x2 matrix",
            example_args(&[(
                "matrix",
                serde_json::json!({"kind": "matrix", "rows": 3, "cols": 2, "data": [1, 2, 3, 4, 5, 6]}),
            )]),
        )
        .with_contains("one_sided_jacobi")],
    })
}

pub(crate) fn invoke_svd(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "linear_algebra.svd")?;
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    let tolerance = positive_tolerance(args, "tolerance", DEFAULT_TOLERANCE)?;
    let max_sweeps = positive_count(args, "max_sweeps", DEFAULT_MAX_SWEEPS)?;
    let max_sweeps = iteration_cap("linear_algebra.svd", max_sweeps, ctx)?;
    let m = matrix.rows;
    let n = matrix.cols;
    enforce_budget(
        "linear_algebra.svd",
        (m as u64)
            .saturating_mul(n as u64)
            .saturating_mul(max_sweeps as u64),
        ctx,
    )?;
    let a = scientific_matrix_floats(&matrix, "svd")?;
    let result = svd_sorted(&a, m, n, tolerance, max_sweeps, ctx)?;
    let k = m.min(n);
    let value = Value::record([
        ("u", matrix_from_floats(m, k, result.u)?.to_value()),
        ("s", float_vector_value(&result.s)?),
        ("v", matrix_from_floats(n, k, result.v)?.to_value()),
        ("sweeps", Value::Number(Number::integer(result.sweeps))),
        ("converged", Value::Bool(true)),
        ("method", Value::text("one_sided_jacobi")),
    ]);
    Ok(Outcome::approximate(value))
}

// ---------------------------------------------------------------------------
// Power iteration
// ---------------------------------------------------------------------------

struct PowerResult {
    eigenvalue: f64,
    eigenvector: Vec<f64>,
    iterations: u64,
}

fn power_iteration(
    a: &[f64],
    n: usize,
    tolerance: f64,
    max_iterations: u64,
    ctx: &ExecContext,
) -> Result<PowerResult, EngineError> {
    let mut x = vec![1.0 / (n as f64).sqrt(); n];
    let mut last_eigenvalue = 0.0f64;
    for iteration in 1..=max_iterations {
        ctx.check()?;
        let mut y = vec![0.0f64; n];
        for i in 0..n {
            let mut total = 0.0f64;
            for j in 0..n {
                total += a[i * n + j] * x[j];
            }
            y[i] = total;
        }
        let norm = y.iter().map(|value| value * value).sum::<f64>().sqrt();
        if norm == 0.0 {
            return Err(EngineError::new(
                ErrorCode::NonConvergence,
                "power_iteration reached the zero vector; the dominant eigenpair is not \
                 reachable from the uniform start vector",
            ));
        }
        let eigenvalue = x
            .iter()
            .zip(y.iter())
            .map(|(left, right)| left * right)
            .sum::<f64>();
        let residual = y
            .iter()
            .zip(x.iter())
            .map(|(value, direction)| (value - eigenvalue * direction).powi(2))
            .sum::<f64>()
            .sqrt();
        if residual <= tolerance * eigenvalue.abs().max(1.0) {
            return Ok(PowerResult {
                eigenvalue,
                eigenvector: x,
                iterations: iteration,
            });
        }
        last_eigenvalue = eigenvalue;
        for i in 0..n {
            x[i] = y[i] / norm;
        }
    }
    Err(EngineError::new(
        ErrorCode::NonConvergence,
        format!("power_iteration did not converge within {max_iterations} iterations"),
    )
    .with_details(serde_json::json!({
        "iterations": max_iterations,
        "last_eigenvalue": last_eigenvalue,
    })))
}

fn power_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("eigenvalue", ValueSchema::float64())
                .with_description("Dominant eigenvalue estimate (Rayleigh quotient)."),
            FieldSchema::required("eigenvector", ValueSchema::array(ValueSchema::float64()))
                .with_description("Unit-norm dominant eigenvector."),
            FieldSchema::required("iterations", ValueSchema::number(NumberKind::Integer))
                .with_description("Power iterations performed."),
            FieldSchema::required("converged", ValueSchema::Bool)
                .with_description("True for a successful result."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"power_iteration\"."),
        ],
        allow_extra: false,
    }
}

fn power_descriptor() -> FunctionDescriptor {
    descriptor(Spec {
        id: "linear_algebra.power_iteration",
        title: "Power iteration",
        summary: "Dominant eigenvalue and eigenvector of a square matrix.",
        description: "Iterates x <- A*x/||A*x|| from the uniform unit vector and estimates the dominant \
             eigenvalue with the Rayleigh quotient. Converges when the residual \
             ||A*x - lambda*x|| is within tolerance of max(1, |lambda|). Scientific mode only: \
             exact inputs are converted explicitly to float64 in that mode. A matrix whose \
             dominant eigenpair is not reachable from the uniform vector, or that does not \
             converge within max_iterations, is reported as non_convergence. Defaults: \
             tolerance 1e-12 and 1000 iterations.",
        params: vec![
            ParamDescriptor::required("matrix", "Square matrix.", matrix_schema(true)),
            ParamDescriptor::optional(
                "tolerance",
                "Residual tolerance; default 1e-12.",
                num_schema(),
            ),
            ParamDescriptor::optional(
                "max_iterations",
                "Iteration cap; default 1000, never above the context iteration limit.",
                ValueSchema::number(NumberKind::Integer),
            ),
        ],
        output: power_output_schema(),
        output_description: "Record with eigenvalue, eigenvector, iterations, converged, method.",
        modes: scientific_modes(),
        cost: CostClass::Iterative,
        method: "docs/methods/linear_algebra.md#power-iteration",
        examples: vec![Example::new(
            "dominant eigenvalue of a diagonal matrix",
            example_args(&[(
                "matrix",
                serde_json::json!({"kind": "matrix", "rows": 2, "cols": 2, "data": [2, 0, 0, 1]}),
            )]),
        )
        .with_contains("power_iteration")],
    })
}

pub(crate) fn invoke_power_iteration(
    args: &Args,
    ctx: &ExecContext,
) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "linear_algebra.power_iteration")?;
    let matrix = Matrix::from_value(args.require("matrix")?, ctx)?;
    if matrix.rows != matrix.cols {
        return Err(EngineError::malformed(
            "power_iteration requires a square matrix",
        ));
    }
    let tolerance = positive_tolerance(args, "tolerance", DEFAULT_TOLERANCE)?;
    let max_iterations = positive_count(args, "max_iterations", DEFAULT_MAX_ITERATIONS)?;
    if max_iterations > ctx.limits.max_iterations {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "linear_algebra.power_iteration requested {max_iterations} iterations, \
                 exceeding the limit of {}",
                ctx.limits.max_iterations
            ),
        ));
    }
    let n = matrix.rows;
    enforce_budget(
        "linear_algebra.power_iteration",
        (n as u64)
            .saturating_mul(n as u64)
            .saturating_mul(max_iterations),
        ctx,
    )?;
    let a = scientific_matrix_floats(&matrix, "power_iteration")?;
    let result = power_iteration(&a, n, tolerance, max_iterations, ctx)?;
    let value = Value::record([
        ("eigenvalue", float_value(result.eigenvalue)?),
        ("eigenvector", float_vector_value(&result.eigenvector)?),
        (
            "iterations",
            Value::Number(Number::integer(result.iterations)),
        ),
        ("converged", Value::Bool(true)),
        ("method", Value::text("power_iteration")),
    ]);
    Ok(Outcome::approximate(value))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub(crate) fn register(functions: &mut Vec<Arc<dyn Function>>) {
    functions.push(SimpleFunction::arc(
        eigen_descriptor(),
        invoke_eigen_symmetric,
    ));
    functions.push(SimpleFunction::arc(svd_descriptor(), invoke_svd));
    functions.push(SimpleFunction::arc(
        power_descriptor(),
        invoke_power_iteration,
    ));
}

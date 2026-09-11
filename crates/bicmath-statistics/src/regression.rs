//! Linear and logistic regression.
//!
//! Ordinary least squares and weighted least squares solve the normal
//! equations with partial pivoting and reject rank-deficient designs with
//! `IllConditioned`. Logistic regression uses iteratively reweighted least
//! squares and returns `NonConvergence` when the iteration limit is reached or
//! the weights collapse (complete separation). The design matrix `x` may be a
//! flat numeric array for simple regression or an array of equal-length rows.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, FunctionDescriptor, Outcome, ParamDescriptor, SimpleFunction,
    require_mode,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::limits::Limits;
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::common::*;
use crate::mathfn::{exp, ln, sqrt, student_t_sf};

// ---------------------------------------------------------------------------
// Matrix helpers
// ---------------------------------------------------------------------------

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter().zip(right.iter()).map(|(a, b)| a * b).sum()
}

fn mat_vec(matrix: &[Vec<f64>], vector: &[f64]) -> Vec<f64> {
    matrix.iter().map(|row| dot(row, vector)).collect()
}

/// Invert a square matrix with Gauss-Jordan elimination and partial pivoting.
///
/// A pivot smaller than `1e-12` times the largest entry of the input is
/// treated as a rank deficiency and reported as `IllConditioned`.
#[allow(clippy::needless_range_loop)]
fn invert_matrix(matrix: &[Vec<f64>]) -> Result<Vec<Vec<f64>>, EngineError> {
    let p = matrix.len();
    if p == 0 {
        return Err(EngineError::internal("normal equations are empty"));
    }
    let mut working = matrix.to_vec();
    let mut inverse = vec![vec![0.0f64; p]; p];
    for (index, row) in inverse.iter_mut().enumerate() {
        row[index] = 1.0;
    }
    let mut scale = 0.0f64;
    for row in matrix {
        for value in row {
            scale = scale.max(value.abs());
        }
    }
    if scale == 0.0 || !scale.is_finite() {
        return Err(EngineError::new(
            ErrorCode::IllConditioned,
            "the design matrix is rank deficient or numerically singular",
        ));
    }
    let tolerance = scale * 1e-12;
    for column in 0..p {
        let mut pivot = column;
        for row in (column + 1)..p {
            if working[row][column].abs() > working[pivot][column].abs() {
                pivot = row;
            }
        }
        if working[pivot][column].abs() <= tolerance {
            return Err(EngineError::new(
                ErrorCode::IllConditioned,
                "the design matrix is rank deficient or numerically singular",
            ));
        }
        working.swap(column, pivot);
        inverse.swap(column, pivot);
        let diagonal = working[column][column];
        for index in 0..p {
            working[column][index] /= diagonal;
            inverse[column][index] /= diagonal;
        }
        for row in 0..p {
            if row == column {
                continue;
            }
            let factor = working[row][column];
            if factor == 0.0 {
                continue;
            }
            for index in 0..p {
                working[row][index] -= factor * working[column][index];
                inverse[row][index] -= factor * inverse[column][index];
            }
        }
    }
    Ok(inverse)
}

fn float_array(values: &[f64]) -> Result<Value, EngineError> {
    values
        .iter()
        .map(|value| float_value(*value))
        .collect::<Result<Vec<_>, _>>()
        .map(array_value)
}

// ---------------------------------------------------------------------------
// Input parsing
// ---------------------------------------------------------------------------

fn number_vector(args: &Args, name: &str) -> Result<Vec<f64>, EngineError> {
    let numbers = collect_numbers(args, name)?;
    if numbers.is_empty() {
        return Err(insufficient(format!("{name} must not be empty")));
    }
    numbers
        .iter()
        .enumerate()
        .map(|(index, number)| {
            number_to_f64(number).map_err(|error| error.with_path(format!("{name}[{index}]")))
        })
        .collect()
}

fn parse_design(value: &Value, name: &str) -> Result<Vec<Vec<f64>>, EngineError> {
    let items = match value {
        Value::Array(items) => items,
        other => {
            return Err(EngineError::malformed(format!(
                "expected an array of numbers or an array of equal-length arrays at {name}, \
                 found {}",
                other.kind_name()
            ))
            .with_path(name.to_string()));
        }
    };
    if items.is_empty() {
        return Err(insufficient(format!("{name} must not be empty")));
    }
    if items.iter().all(|item| matches!(item, Value::Number(_))) {
        let mut rows = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let number = item
                .as_number()
                .map_err(|error| error.with_path(format!("{name}[{index}]")))?;
            let value = number_to_f64(number)
                .map_err(|error| error.with_path(format!("{name}[{index}]")))?;
            rows.push(vec![value]);
        }
        return Ok(rows);
    }
    if items.iter().all(|item| matches!(item, Value::Array(_))) {
        let mut rows = Vec::with_capacity(items.len());
        let mut width: Option<usize> = None;
        for (index, item) in items.iter().enumerate() {
            let row = item
                .as_array()
                .map_err(|error| error.with_path(format!("{name}[{index}]")))?;
            if row.is_empty() {
                return Err(
                    EngineError::domain(format!("row {index} of {name} is empty"))
                        .with_path(format!("{name}[{index}]")),
                );
            }
            match width {
                None => width = Some(row.len()),
                Some(expected) if expected != row.len() => {
                    return Err(EngineError::malformed(format!(
                        "ragged {name}: row {index} has {} entries, expected {expected}",
                        row.len()
                    ))
                    .with_path(format!("{name}[{index}]")));
                }
                Some(_) => {}
            }
            let mut values = Vec::with_capacity(row.len());
            for (column, cell) in row.iter().enumerate() {
                let number = cell
                    .as_number()
                    .map_err(|error| error.with_path(format!("{name}[{index}][{column}]")))?;
                values.push(
                    number_to_f64(number)
                        .map_err(|error| error.with_path(format!("{name}[{index}][{column}]")))?,
                );
            }
            rows.push(values);
        }
        return Ok(rows);
    }
    Err(EngineError::malformed(format!(
        "{name} must be a flat numeric array or an array of equal-length numeric arrays"
    ))
    .with_path(name.to_string()))
}

fn build_design(mut rows: Vec<Vec<f64>>, intercept: bool) -> Vec<Vec<f64>> {
    if intercept {
        for row in &mut rows {
            row.insert(0, 1.0);
        }
    }
    rows
}

fn check_design_limits(design: &[Vec<f64>], limits: &Limits) -> Result<(), EngineError> {
    let columns = design.first().map_or(0, Vec::len);
    let elements = design.len().saturating_mul(columns);
    if elements > limits.max_matrix_elements {
        return Err(EngineError::resource(format!(
            "design matrix has {elements} elements, exceeding the limit of {}",
            limits.max_matrix_elements
        )));
    }
    Ok(())
}

fn parse_weights(args: &Args, n: usize) -> Result<Vec<f64>, EngineError> {
    let weights = number_vector(args, "weights")?;
    if weights.len() != n {
        return Err(EngineError::malformed(format!(
            "weights has {} entries but y has {n} observations",
            weights.len()
        )));
    }
    let mut total = 0.0f64;
    for (index, weight) in weights.iter().enumerate() {
        if *weight < 0.0 {
            return Err(EngineError::domain("weights must be non-negative")
                .with_path(format!("weights[{index}]")));
        }
        total += weight;
    }
    if total <= 0.0 {
        return Err(EngineError::domain(
            "the total weight must be strictly positive",
        ));
    }
    Ok(weights)
}

fn parse_binary_vector(args: &Args, name: &str) -> Result<Vec<f64>, EngineError> {
    let values = number_vector(args, name)?;
    for (index, value) in values.iter().enumerate() {
        if *value != 0.0 && *value != 1.0 {
            return Err(
                EngineError::domain(format!("{name} must contain only 0 and 1 values"))
                    .with_path(format!("{name}[{index}]")),
            );
        }
    }
    Ok(values)
}

// ---------------------------------------------------------------------------
// Weighted least squares
// ---------------------------------------------------------------------------

struct LinearFit {
    coefficients: Vec<f64>,
    standard_errors: Vec<f64>,
    t_statistics: Vec<f64>,
    p_values: Vec<f64>,
    r_squared: f64,
    adjusted_r_squared: f64,
    residual_standard_error: f64,
    df_residual: f64,
    fitted: Vec<f64>,
    residuals: Vec<f64>,
}

fn weighted_fit(y: &[f64], design: &[Vec<f64>], weights: &[f64]) -> Result<LinearFit, EngineError> {
    let n = y.len();
    let p = design[0].len();
    if n <= p {
        return Err(insufficient(format!(
            "regression with {p} coefficients requires more than {p} observations"
        )));
    }
    let mut normal = vec![vec![0.0f64; p]; p];
    let mut rhs = vec![0.0f64; p];
    for (index, (row, weight)) in design.iter().zip(weights.iter()).enumerate() {
        if *weight == 0.0 {
            continue;
        }
        for (a, row_a) in row.iter().enumerate() {
            let weighted = weight * row_a;
            rhs[a] += weighted * y[index];
            for (b, row_b) in row.iter().enumerate() {
                normal[a][b] += weighted * row_b;
            }
        }
    }
    let inverse = invert_matrix(&normal)?;
    let coefficients = mat_vec(&inverse, &rhs);
    let fitted: Vec<f64> = design.iter().map(|row| dot(row, &coefficients)).collect();
    let residuals: Vec<f64> = y
        .iter()
        .zip(fitted.iter())
        .map(|(observed, predicted)| observed - predicted)
        .collect();
    let sse: f64 = residuals.iter().map(|value| value * value).sum();
    let mean_y = mean_f64(y);
    let sst: f64 = y
        .iter()
        .map(|value| (value - mean_y) * (value - mean_y))
        .sum();
    let df_residual = (n - p) as f64;
    let sigma_squared = sse / df_residual;
    let residual_standard_error = sqrt(sigma_squared);
    let r_squared = if sst > 0.0 {
        1.0 - sse / sst
    } else if sse == 0.0 {
        1.0
    } else {
        0.0
    };
    let adjusted_r_squared = 1.0 - (1.0 - r_squared) * (n as f64 - 1.0) / df_residual;
    let mut standard_errors = Vec::with_capacity(p);
    let mut t_statistics = Vec::with_capacity(p);
    let mut p_values = Vec::with_capacity(p);
    for (index, coefficient) in coefficients.iter().enumerate() {
        let variance = (sigma_squared * inverse[index][index]).max(0.0);
        let standard_error = sqrt(variance);
        let statistic = if standard_error > 0.0 {
            coefficient / standard_error
        } else if *coefficient == 0.0 {
            0.0
        } else {
            coefficient.signum() * f64::MAX
        };
        standard_errors.push(standard_error);
        t_statistics.push(statistic);
        p_values.push(2.0 * student_t_sf(statistic.abs(), df_residual)?);
    }
    Ok(LinearFit {
        coefficients,
        standard_errors,
        t_statistics,
        p_values,
        r_squared,
        adjusted_r_squared,
        residual_standard_error,
        df_residual,
        fitted,
        residuals,
    })
}

fn linear_output_schema() -> ValueSchema {
    record_schema(
        vec![
            field("coefficients", array_schema(float64_schema())),
            field("standard_errors", array_schema(float64_schema())),
            field("t_statistics", array_schema(float64_schema())),
            field("p_values", array_schema(float64_schema())),
            field("r_squared", float64_schema()),
            field("adjusted_r_squared", float64_schema()),
            field("residual_standard_error", float64_schema()),
            field("df_residual", float64_schema()),
            field("fitted", array_schema(float64_schema())),
            field("residuals", array_schema(float64_schema())),
            field("method", text_schema()),
        ],
        false,
    )
}

fn linear_fit_value(fit: &LinearFit, method: &str) -> Result<Value, EngineError> {
    Ok(record(vec![
        ("coefficients", float_array(&fit.coefficients)?),
        ("standard_errors", float_array(&fit.standard_errors)?),
        ("t_statistics", float_array(&fit.t_statistics)?),
        ("p_values", float_array(&fit.p_values)?),
        ("r_squared", float_value(fit.r_squared)?),
        ("adjusted_r_squared", float_value(fit.adjusted_r_squared)?),
        (
            "residual_standard_error",
            float_value(fit.residual_standard_error)?,
        ),
        ("df_residual", float_value(fit.df_residual)?),
        ("fitted", float_array(&fit.fitted)?),
        ("residuals", float_array(&fit.residuals)?),
        ("method", text(method)),
    ]))
}

fn regression_parameters(x_description: &str, intercept_description: &str) -> Vec<ParamDescriptor> {
    vec![
        ParamDescriptor::required("y", "Response values.", array_schema(any_number_schema())),
        ParamDescriptor::required("x", x_description, ValueSchema::Any),
        ParamDescriptor::optional("intercept", intercept_description, bool_schema()),
    ]
}

fn ols_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.ols",
        "statistics",
        "1.0.0",
        "Ordinary least squares",
        "Ordinary least squares regression with pivoted normal equations.",
    )
    .with_description(
        "x may be a flat numeric array (simple regression) or an array of equal-length rows \
         (one row per observation, one column per predictor). intercept defaults to true. The \
         normal equations are solved with partial pivoting; a rank-deficient or numerically \
         singular design is rejected with ill_conditioned. More observations than \
         coefficients are required. Returns coefficients, standard errors, t statistics, \
         two-sided t p-values, R^2, adjusted R^2, the residual standard error, the residual \
         degrees of freedom, fitted values, residuals, and method = \"ols\".",
    )
    .with_parameters(regression_parameters(
        "Flat predictor array or array of predictor rows.",
        "Include an intercept column; default true.",
    ))
    .with_output(linear_output_schema(), "Least-squares fit record.")
    .with_modes(inferential_modes())
    .with_cost(CostClass::Cubic)
    .with_method_ref("docs/methods/statistics.md#ols")
    .with_examples(vec![
        Example::new(
            "simple regression",
            example_args(&[
                ("y", serde_json::json!([1, 2, 3])),
                ("x", serde_json::json!([0, 1, 2])),
            ]),
        )
        .with_contains("ols"),
        Example::new(
            "rank deficient design",
            example_args(&[
                ("y", serde_json::json!([1, 2, 3])),
                ("x", serde_json::json!([[1], [1], [1]])),
            ]),
        )
        .with_error(ErrorCode::IllConditioned),
    ])
}

fn invoke_ols(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.ols")?;
    let y = number_vector(args, "y")?;
    let design = parse_design(args.require("x")?, "x")?;
    if design.len() != y.len() {
        return Err(EngineError::malformed(format!(
            "y has {} observations but x has {} rows",
            y.len(),
            design.len()
        )));
    }
    let intercept = args.optional_bool("intercept")?.unwrap_or(true);
    let design = build_design(design, intercept);
    check_design_limits(&design, &ctx.limits)?;
    let weights = vec![1.0f64; y.len()];
    let fit = weighted_fit(&y, &design, &weights)?;
    Ok(Outcome::approximate(linear_fit_value(&fit, "ols")?))
}

fn wls_descriptor() -> FunctionDescriptor {
    let mut parameters = regression_parameters(
        "Flat predictor array or array of predictor rows.",
        "Include an intercept column; default true.",
    );
    parameters.insert(
        2,
        ParamDescriptor::required(
            "weights",
            "Non-negative observation weights, one per observation.",
            array_schema(any_number_schema()),
        ),
    );
    FunctionDescriptor::new(
        "statistics.wls",
        "statistics",
        "1.0.0",
        "Weighted least squares",
        "Weighted least squares regression with pivoted normal equations.",
    )
    .with_description(
        "Minimizes sum(w_i (y_i - x_i b)^2) by solving the weighted normal equations with \
         partial pivoting. weights must be non-negative with a strictly positive total; a \
         zero weight drops the observation. The output shape matches statistics.ols with \
         method = \"wls\". A rank-deficient or numerically singular design is rejected with \
         ill_conditioned.",
    )
    .with_parameters(parameters)
    .with_output(linear_output_schema(), "Weighted least-squares fit record.")
    .with_modes(inferential_modes())
    .with_cost(CostClass::Cubic)
    .with_method_ref("docs/methods/statistics.md#wls")
    .with_examples(vec![
        Example::new(
            "equal weights match ordinary least squares",
            example_args(&[
                ("y", serde_json::json!([1, 2, 3])),
                ("x", serde_json::json!([0, 1, 2])),
                ("weights", serde_json::json!([1, 1, 1])),
            ]),
        )
        .with_contains("wls"),
        Example::new(
            "negative weight",
            example_args(&[
                ("y", serde_json::json!([1, 2, 3])),
                ("x", serde_json::json!([0, 1, 2])),
                ("weights", serde_json::json!([1, -1, 1])),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_wls(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.wls")?;
    let y = number_vector(args, "y")?;
    let design = parse_design(args.require("x")?, "x")?;
    if design.len() != y.len() {
        return Err(EngineError::malformed(format!(
            "y has {} observations but x has {} rows",
            y.len(),
            design.len()
        )));
    }
    let weights = parse_weights(args, y.len())?;
    let intercept = args.optional_bool("intercept")?.unwrap_or(true);
    let design = build_design(design, intercept);
    check_design_limits(&design, &ctx.limits)?;
    let fit = weighted_fit(&y, &design, &weights)?;
    Ok(Outcome::approximate(linear_fit_value(&fit, "wls")?))
}

// ---------------------------------------------------------------------------
// Logistic regression
// ---------------------------------------------------------------------------

fn sigmoid(value: f64) -> f64 {
    if value >= 0.0 {
        1.0 / (1.0 + exp(-value))
    } else {
        let exponential = exp(value);
        exponential / (1.0 + exponential)
    }
}

fn log_sigmoid(value: f64) -> f64 {
    if value >= 0.0 {
        -ln(1.0 + exp(-value))
    } else {
        value - ln(1.0 + exp(value))
    }
}

struct LogisticFit {
    coefficients: Vec<f64>,
    standard_errors: Vec<f64>,
    z_statistics: Vec<f64>,
    p_values: Vec<f64>,
    log_likelihood: f64,
    aic: f64,
    iterations: u64,
}

fn log_likelihood(y: &[f64], design: &[Vec<f64>], beta: &[f64]) -> f64 {
    design
        .iter()
        .zip(y.iter())
        .map(|(row, response)| {
            let eta = dot(row, beta);
            response * log_sigmoid(eta) + (1.0 - response) * log_sigmoid(-eta)
        })
        .sum()
}

fn logistic_fit(
    y: &[f64],
    design: &[Vec<f64>],
    tolerance: f64,
    max_iterations: u64,
) -> Result<LogisticFit, EngineError> {
    let n = y.len();
    let p = design[0].len();
    if n <= p {
        return Err(insufficient(format!(
            "logistic regression with {p} coefficients requires more than {p} observations"
        )));
    }
    let mut beta = vec![0.0f64; p];
    let mut iterations = 0u64;
    let mut converged = false;
    let mut previous_likelihood: Option<f64> = None;
    for iteration in 1..=max_iterations {
        iterations = iteration;
        let mut normal = vec![vec![0.0f64; p]; p];
        let mut rhs = vec![0.0f64; p];
        for (index, row) in design.iter().enumerate() {
            let eta = dot(row, &beta);
            let mu = sigmoid(eta);
            let weight = (mu * (1.0 - mu)).max(1e-12);
            let working = eta + (y[index] - mu) / weight;
            for (a, row_a) in row.iter().enumerate() {
                let weighted = weight * row_a;
                rhs[a] += weighted * working;
                for (b, row_b) in row.iter().enumerate() {
                    normal[a][b] += weighted * row_b;
                }
            }
        }
        let inverse = invert_matrix(&normal).map_err(|_| {
            EngineError::new(
                ErrorCode::NonConvergence,
                "logistic regression weights collapsed; the data may be completely separated",
            )
        })?;
        let next = mat_vec(&inverse, &rhs);
        let delta = next
            .iter()
            .zip(beta.iter())
            .map(|(new, old)| (new - old).abs())
            .fold(0.0f64, f64::max);
        beta = next;
        let likelihood = log_likelihood(y, design, &beta);
        let likelihood_change = previous_likelihood.map(|previous| (likelihood - previous).abs());
        if delta < tolerance
            || likelihood_change
                .is_some_and(|change| change <= tolerance * (1.0 + likelihood.abs()))
        {
            converged = true;
            break;
        }
        previous_likelihood = Some(likelihood);
    }
    if !converged {
        return Err(EngineError::new(
            ErrorCode::NonConvergence,
            format!("logistic regression did not converge within {max_iterations} iterations"),
        ));
    }
    let mut normal = vec![vec![0.0f64; p]; p];
    let log_likelihood = log_likelihood(y, design, &beta);
    for row in design.iter() {
        let mu = sigmoid(dot(row, &beta));
        let weight = (mu * (1.0 - mu)).max(1e-12);
        for (a, row_a) in row.iter().enumerate() {
            let weighted = weight * row_a;
            for (b, row_b) in row.iter().enumerate() {
                normal[a][b] += weighted * row_b;
            }
        }
    }
    let inverse = invert_matrix(&normal).map_err(|_| {
        EngineError::new(
            ErrorCode::NonConvergence,
            "logistic regression information matrix is singular",
        )
    })?;
    let mut standard_errors = Vec::with_capacity(p);
    let mut z_statistics = Vec::with_capacity(p);
    let mut p_values = Vec::with_capacity(p);
    for (index, coefficient) in beta.iter().enumerate() {
        let variance = inverse[index][index].max(0.0);
        let standard_error = sqrt(variance);
        let statistic = if standard_error > 0.0 {
            coefficient / standard_error
        } else if *coefficient == 0.0 {
            0.0
        } else {
            coefficient.signum() * f64::MAX
        };
        standard_errors.push(standard_error);
        z_statistics.push(statistic);
        p_values.push(2.0 * crate::mathfn::normal_sf(statistic.abs(), 0.0, 1.0)?);
    }
    let aic = -2.0 * log_likelihood + 2.0 * p as f64;
    Ok(LogisticFit {
        coefficients: beta,
        standard_errors,
        z_statistics,
        p_values,
        log_likelihood,
        aic,
        iterations,
    })
}

/// Fitted propensity probabilities from the logistic model of a binary response
/// on a design matrix that already contains any intercept column. This reuses
/// the IRLS fitter above so that the propensity model and the registered
/// logistic regression share one implementation.
pub(crate) fn logistic_probabilities(
    y: &[f64],
    design: &[Vec<f64>],
    tolerance: f64,
    max_iterations: u64,
) -> Result<Vec<f64>, EngineError> {
    let fit = logistic_fit(y, design, tolerance, max_iterations)?;
    Ok(design
        .iter()
        .map(|row| sigmoid(dot(row, &fit.coefficients)))
        .collect())
}

fn logistic_descriptor() -> FunctionDescriptor {
    FunctionDescriptor::new(
        "statistics.logistic_regression",
        "statistics",
        "1.0.0",
        "Logistic regression",
        "Binary logistic regression fitted with iteratively reweighted least squares.",
    )
    .with_description(
        "y must contain only 0 and 1 values. x may be a flat numeric array (simple \
         regression) or an array of equal-length rows; intercept defaults to true. Fitting \
         uses iteratively reweighted least squares with tolerance (default 1e-8) on the \
         maximum coefficient change and max_iterations (default 100). Complete or \
         quasi-complete separation and failure to converge within the iteration limit are \
         reported as non_convergence. Returns coefficients, standard errors, z statistics, \
         two-sided normal p-values, the log-likelihood, AIC, the iteration count, the \
         convergence flag, and method = \"irls_logistic\".",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "y",
            "Binary response values (0 or 1).",
            array_schema(any_number_schema()),
        ),
        ParamDescriptor::required(
            "x",
            "Flat predictor array or array of predictor rows.",
            ValueSchema::Any,
        ),
        ParamDescriptor::optional(
            "intercept",
            "Include an intercept column; default true.",
            bool_schema(),
        ),
        ParamDescriptor::optional(
            "tolerance",
            "Convergence tolerance on the maximum coefficient change; default 1e-8.",
            any_number_schema(),
        ),
        ParamDescriptor::optional(
            "max_iterations",
            "Maximum IRLS iterations; integer >= 1; default 100.",
            integer_schema(),
        ),
    ])
    .with_output(
        record_schema(
            vec![
                field("coefficients", array_schema(float64_schema())),
                field("standard_errors", array_schema(float64_schema())),
                field("z_statistics", array_schema(float64_schema())),
                field("p_values", array_schema(float64_schema())),
                field("log_likelihood", float64_schema()),
                field("aic", float64_schema()),
                field("iterations", integer_schema()),
                field("converged", bool_schema()),
                field("method", text_schema()),
            ],
            false,
        ),
        "Logistic regression fit record.",
    )
    .with_modes(inferential_modes())
    .with_cost(CostClass::Iterative)
    .with_method_ref("docs/methods/statistics.md#logistic_regression")
    .with_examples(vec![
        Example::new(
            "one dimensional separation",
            example_args(&[
                ("y", serde_json::json!([0, 1, 0, 1, 1])),
                ("x", serde_json::json!([-2, -1, 0, 1, 2])),
            ]),
        )
        .with_contains("irls_logistic"),
        Example::new(
            "non-binary response",
            example_args(&[
                ("y", serde_json::json!([0, 2, 1])),
                ("x", serde_json::json!([0, 1, 2])),
            ]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_logistic_regression(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_mode(ctx, &inferential_modes(), "statistics.logistic_regression")?;
    let y = parse_binary_vector(args, "y")?;
    let design = parse_design(args.require("x")?, "x")?;
    if design.len() != y.len() {
        return Err(EngineError::malformed(format!(
            "y has {} observations but x has {} rows",
            y.len(),
            design.len()
        )));
    }
    let intercept = args.optional_bool("intercept")?.unwrap_or(true);
    let tolerance = optional_f64_param(args, "tolerance")?.unwrap_or(1e-8);
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(EngineError::domain("tolerance must be strictly positive")
            .with_path("tolerance".to_string()));
    }
    let max_iterations = match args.optional_integer("max_iterations")? {
        None => 100,
        Some(value) => {
            let iterations = non_negative_u64(&value, "max_iterations")?;
            if iterations == 0 {
                return Err(EngineError::domain("max_iterations must be at least 1")
                    .with_path("max_iterations".to_string()));
            }
            iterations
        }
    };
    let design = build_design(design, intercept);
    check_design_limits(&design, &ctx.limits)?;
    let fit = logistic_fit(&y, &design, tolerance, max_iterations)?;
    let value = record(vec![
        ("coefficients", float_array(&fit.coefficients)?),
        ("standard_errors", float_array(&fit.standard_errors)?),
        ("z_statistics", float_array(&fit.z_statistics)?),
        ("p_values", float_array(&fit.p_values)?),
        ("log_likelihood", float_value(fit.log_likelihood)?),
        ("aic", float_value(fit.aic)?),
        ("iterations", integer_value(fit.iterations)),
        ("converged", bool_value(true)),
        ("method", text("irls_logistic")),
    ]);
    Ok(Outcome::approximate(value))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn functions() -> Vec<Arc<dyn bicmath_core::contract::Function>> {
    vec![
        SimpleFunction::arc(ols_descriptor(), invoke_ols),
        SimpleFunction::arc(wls_descriptor(), invoke_wls),
        SimpleFunction::arc(logistic_descriptor(), invoke_logistic_regression),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
        let module = crate::module();
        let function = module
            .functions
            .iter()
            .find(|f| f.descriptor().id == id)
            .expect("function exists");
        let ctx = ExecContext::scientific();
        let args_json = raw.as_object().expect("object args");
        let mut values = BTreeMap::new();
        for (name, value) in args_json {
            let param = function
                .descriptor()
                .parameter(name)
                .expect("parameter exists");
            values.insert(
                name.clone(),
                param
                    .schema
                    .coerce(value, name, &ctx.limits, true)
                    .expect("argument coerces"),
            );
        }
        function.invoke(&Args::new(values), &ctx)
    }

    fn record_of(outcome: &Outcome) -> &BTreeMap<String, Value> {
        match &outcome.value {
            Value::Record(fields) => fields,
            other => panic!("expected record, got {other:?}"),
        }
    }

    fn field_f64(fields: &BTreeMap<String, Value>, name: &str) -> f64 {
        match fields.get(name) {
            Some(Value::Number(number)) => number.to_f64().expect("number"),
            other => panic!("expected numeric field {name}, got {other:?}"),
        }
    }

    fn field_numbers(fields: &BTreeMap<String, Value>, name: &str) -> Vec<f64> {
        match fields.get(name) {
            Some(Value::Array(items)) => items
                .iter()
                .map(|item| item.as_number().expect("number").to_f64().expect("f64"))
                .collect(),
            other => panic!("expected array field {name}, got {other:?}"),
        }
    }

    fn close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected}, got {actual} (tolerance {tolerance})"
        );
    }

    fn close_slice(actual: &[f64], expected: &[f64], tolerance: f64) {
        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(expected.iter()) {
            close(*a, *e, tolerance);
        }
    }

    #[test]
    fn ols_recovers_an_exact_line() {
        let outcome = call(
            "statistics.ols",
            serde_json::json!({"y": [1, 2, 3], "x": [0, 1, 2]}),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close_slice(&field_numbers(fields, "coefficients"), &[1.0, 1.0], 1e-12);
        close(field_f64(fields, "r_squared"), 1.0, 1e-12);
        close(field_f64(fields, "adjusted_r_squared"), 1.0, 1e-12);
        close(field_f64(fields, "residual_standard_error"), 0.0, 1e-12);
        close(field_f64(fields, "df_residual"), 1.0, 0.0);
        close_slice(&field_numbers(fields, "fitted"), &[1.0, 2.0, 3.0], 1e-12);
        close_slice(&field_numbers(fields, "residuals"), &[0.0, 0.0, 0.0], 1e-12);
    }

    #[test]
    fn ols_matches_independent_reference_statistics() {
        // Provenance: Python 3.14 stdlib implementation of the normal
        // equations and of the Student-t upper tail for y = [1, 2, 1, 3],
        // x = [0, 1, 2, 3].
        let outcome = call(
            "statistics.ols",
            serde_json::json!({"y": [1, 2, 1, 3], "x": [0, 1, 2, 3]}),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close_slice(&field_numbers(fields, "coefficients"), &[1.0, 0.5], 1e-12);
        close_slice(
            &field_numbers(fields, "standard_errors"),
            &[0.724_568_837_309_471_9, 0.387_298_334_620_741_7],
            1e-12,
        );
        close_slice(
            &field_numbers(fields, "t_statistics"),
            &[1.380_131_118_684_708, 1.290_994_448_735_805_6],
            1e-12,
        );
        close_slice(
            &field_numbers(fields, "p_values"),
            &[0.301_569_704_230_421_9, 0.325_800_137_536_757_7],
            1e-9,
        );
        close(
            field_f64(fields, "r_squared"),
            0.454_545_454_545_454_6,
            1e-12,
        );
        close(
            field_f64(fields, "adjusted_r_squared"),
            0.181_818_181_818_181_88,
            1e-12,
        );
        close(
            field_f64(fields, "residual_standard_error"),
            0.866_025_403_784_438_6,
            1e-12,
        );
    }

    #[test]
    fn ols_handles_multiple_predictors() {
        // y = 1 + 2*x1 - x2 with x1 = [1, 2, 3, 4], x2 = [0, 1, 0, 1].
        let y: Vec<f64> = vec![3.0, 4.0, 7.0, 8.0];
        let x1 = [1.0, 2.0, 3.0, 4.0];
        let x2 = [0.0, 1.0, 0.0, 1.0];
        let x: Vec<Vec<f64>> = x1
            .iter()
            .zip(x2.iter())
            .map(|(a, b)| vec![*a, *b])
            .collect();
        let outcome = call("statistics.ols", serde_json::json!({"y": y, "x": x})).unwrap();
        let fields = record_of(&outcome);
        close_slice(
            &field_numbers(fields, "coefficients"),
            &[1.0, 2.0, -1.0],
            1e-9,
        );
        close(field_f64(fields, "r_squared"), 1.0, 1e-9);
    }

    #[test]
    fn wls_equals_ols_for_equal_weights() {
        let ols = call(
            "statistics.ols",
            serde_json::json!({"y": [1, 2, 3], "x": [0, 1, 2]}),
        )
        .unwrap();
        let wls = call(
            "statistics.wls",
            serde_json::json!({"y": [1, 2, 3], "x": [0, 1, 2], "weights": [1, 1, 1]}),
        )
        .unwrap();
        let ols_fields = record_of(&ols);
        let wls_fields = record_of(&wls);
        for name in [
            "coefficients",
            "standard_errors",
            "t_statistics",
            "p_values",
            "fitted",
            "residuals",
        ] {
            assert_eq!(
                ols_fields.get(name),
                wls_fields.get(name),
                "field {name} differs between ols and wls"
            );
        }
        close(
            field_f64(wls_fields, "r_squared"),
            field_f64(ols_fields, "r_squared"),
            0.0,
        );
        close(
            field_f64(wls_fields, "residual_standard_error"),
            field_f64(ols_fields, "residual_standard_error"),
            0.0,
        );
    }

    #[test]
    fn wls_reference_values() {
        // Provenance: Python 3.14 stdlib implementation of the weighted normal
        // equations for y = [1, 2, 3], x = [0, 1, 2], weights = [1, 2, 3]. The
        // line is exact, so the coefficients are 1 and 1.
        let outcome = call(
            "statistics.wls",
            serde_json::json!({
                "y": [1, 2, 3],
                "x": [0, 1, 2],
                "weights": [1, 2, 3]
            }),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close_slice(&field_numbers(fields, "coefficients"), &[1.0, 1.0], 1e-9);
        close(field_f64(fields, "r_squared"), 1.0, 1e-9);
    }

    #[test]
    fn ols_rejects_rank_deficient_designs() {
        let error = call(
            "statistics.ols",
            serde_json::json!({"y": [1, 2, 3], "x": [[1], [1], [1]]}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::IllConditioned);
    }

    #[test]
    fn logistic_reference_values() {
        // Provenance: Python 3.14 stdlib IRLS (same weight clamp) for
        // y = [0, 1, 0, 1, 1], x = [-2, -1, 0, 1, 2] with an intercept.
        let outcome = call(
            "statistics.logistic_regression",
            serde_json::json!({"y": [0, 1, 0, 1, 1], "x": [-2, -1, 0, 1, 2]}),
        )
        .unwrap();
        let fields = record_of(&outcome);
        close_slice(
            &field_numbers(fields, "coefficients"),
            &[0.622_690_065_433_757_8, 1.090_425_560_298_115_2],
            1e-6,
        );
        close(
            field_f64(fields, "log_likelihood"),
            -2.421_966_843_685_810_4,
            1e-6,
        );
        close(field_f64(fields, "aic"), 8.843_933_687_371_62, 1e-6);
        match fields.get("converged") {
            Some(Value::Bool(true)) => {}
            other => panic!("expected converged = true, got {other:?}"),
        }
    }

    #[test]
    fn logistic_separates_a_simple_dataset() {
        let outcome = call(
            "statistics.logistic_regression",
            serde_json::json!({"y": [0, 0, 1, 1], "x": [-1, -0.5, 0.5, 1]}),
        )
        .unwrap();
        let fields = record_of(&outcome);
        let coefficients = field_numbers(fields, "coefficients");
        assert!(coefficients[1] > 0.0, "expected a positive slope");
        let lower = sigmoid(coefficients[0] - coefficients[1]);
        let upper = sigmoid(coefficients[0] + coefficients[1] * 1.0);
        assert!(
            lower < 0.5 && upper > 0.5,
            "the fit must separate the groups"
        );
    }

    #[test]
    fn logistic_reports_non_convergence_for_a_tight_limit() {
        let error = call(
            "statistics.logistic_regression",
            serde_json::json!({
                "y": [0, 1, 0, 1, 1],
                "x": [-2, -1, 0, 1, 2],
                "max_iterations": 1
            }),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::NonConvergence);
    }

    #[test]
    fn logistic_rejects_non_binary_responses() {
        let error = call(
            "statistics.logistic_regression",
            serde_json::json!({"y": [0, 2, 1], "x": [0, 1, 2]}),
        )
        .unwrap_err();
        assert_eq!(error.code, ErrorCode::DomainViolation);
    }
}

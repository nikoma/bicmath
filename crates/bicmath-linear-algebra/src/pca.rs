//! Principal component analysis by singular value decomposition.
//!
//! Rows of the input matrix are observations and columns are variables. The
//! data are centered (and optionally standardized), decomposed with the
//! one-sided Jacobi SVD, and reported as principal components, explained
//! variance, and projected scores. Scientific mode only: exact inputs are
//! converted explicitly to binary64 in that mode.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction,
};
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::schema::{FieldSchema, ValueSchema};
use bicmath_core::value::Value;

use crate::decompositions::svd_sorted;
use crate::{
    Matrix, Spec, descriptor, example_args, float_vector_value, matrix_from_floats, matrix_schema,
    require_scientific, scientific_matrix_floats, scientific_modes,
};

const SVD_TOLERANCE: f64 = 1e-12;
const SVD_MAX_SWEEPS: u64 = 100;

fn pca_output_schema() -> ValueSchema {
    ValueSchema::Record {
        fields: vec![
            FieldSchema::required("means", ValueSchema::array(ValueSchema::float64()))
                .with_description("Column means, one per variable."),
            FieldSchema::required("stddevs", ValueSchema::array(ValueSchema::float64()))
                .with_description(
                    "Sample standard deviations (n-1 denominator), one per variable.",
                ),
            FieldSchema::required("eigenvalues", ValueSchema::array(ValueSchema::float64()))
                .with_description(
                    "Eigenvalues of the covariance or correlation matrix, descending.",
                ),
            FieldSchema::required(
                "explained_variance",
                ValueSchema::array(ValueSchema::float64()),
            )
            .with_description("Variance explained by each component."),
            FieldSchema::required(
                "explained_variance_ratio",
                ValueSchema::array(ValueSchema::float64()),
            )
            .with_description("Share of total variance explained by each component."),
            FieldSchema::required("components", matrix_schema(false)).with_description(
                "Principal axes as rows (k x p); component i is row i over the variables.",
            ),
            FieldSchema::required("scores", matrix_schema(false))
                .with_description("Projected data (n x k); scores = centered data * components^T."),
            FieldSchema::required("method", ValueSchema::text())
                .with_description("Always \"svd_pca\"."),
        ],
        allow_extra: false,
    }
}

fn pca_descriptor() -> FunctionDescriptor {
    descriptor(Spec {
        id: "linear_algebra.pca",
        title: "Principal component analysis",
        summary: "Principal components, explained variance, and scores from an observation matrix.",
        description:
            "Rows are observations and columns are variables. The data are centered and, when \
             standardize is true, divided by their sample standard deviations so the \
             decomposition uses the correlation matrix; otherwise the covariance matrix is \
             used. The centered data are decomposed with the one-sided Jacobi SVD. Eigenvalues \
             are the squared singular values divided by n-1 in descending order, components \
             are the principal axes as rows (k x p with k = min(n, p)), and scores are the \
             projections of the centered data onto those axes (n x k). Zero-variance variables \
             are left as zeros when standardizing. Scientific mode only: exact inputs are \
             converted explicitly to float64 in that mode.",
        params: vec![
            ParamDescriptor::required(
                "data",
                "Observation matrix: rows are observations, columns variables.",
                matrix_schema(false),
            ),
            ParamDescriptor::optional(
                "standardize",
                "When true, use the correlation matrix (divide each centered column by its \
                 sample standard deviation); default false.",
                ValueSchema::Bool,
            ),
        ],
        output: pca_output_schema(),
        output_description: "Record with means, stddevs, eigenvalues, explained_variance, \
                             explained_variance_ratio, components, scores, method.",
        modes: scientific_modes(),
        cost: CostClass::Cubic,
        method: "docs/methods/linear_algebra.md#pca",
        examples: vec![Example::new(
            "perfectly correlated 2D data",
            example_args(&[(
                "data",
                serde_json::json!({"kind": "matrix", "rows": 3, "cols": 2, "data": [1, 2, 2, 4, 3, 6]}),
            )]),
        )
        .with_contains("svd_pca")],
    })
}

pub(crate) fn invoke_pca(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    require_scientific(ctx, "linear_algebra.pca")?;
    let data = Matrix::from_value(args.require("data")?, ctx)?;
    let standardize = args.optional_bool("standardize")?.unwrap_or(false);
    let observations = data.rows;
    let variables = data.cols;
    if observations < 2 {
        return Err(EngineError::new(
            ErrorCode::InsufficientObservations,
            "pca requires at least two observations (rows)",
        ));
    }
    let components = observations.min(variables);
    let estimate = (observations as u64)
        .saturating_mul(variables as u64)
        .saturating_mul(components as u64);
    if estimate > ctx.limits.max_operations {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "linear_algebra.pca needs an estimated {estimate} operations, exceeding the \
                 limit of {}",
                ctx.limits.max_operations
            ),
        ));
    }
    let max_sweeps = SVD_MAX_SWEEPS.min(ctx.limits.max_iterations).max(1) as usize;
    let values = scientific_matrix_floats(&data, "pca")?;

    let mut means = vec![0.0f64; variables];
    for row in 0..observations {
        ctx.check()?;
        for column in 0..variables {
            means[column] += values[row * variables + column];
        }
    }
    for mean in &mut means {
        *mean /= observations as f64;
    }

    let mut centered = vec![0.0f64; observations * variables];
    for row in 0..observations {
        for column in 0..variables {
            centered[row * variables + column] = values[row * variables + column] - means[column];
        }
    }

    let mut stddevs = vec![0.0f64; variables];
    for column in 0..variables {
        let mut sum = 0.0f64;
        for row in 0..observations {
            let difference = centered[row * variables + column];
            sum += difference * difference;
        }
        stddevs[column] = (sum / (observations as f64 - 1.0)).sqrt();
    }

    if standardize {
        for row in 0..observations {
            for column in 0..variables {
                if stddevs[column] > 0.0 {
                    centered[row * variables + column] /= stddevs[column];
                } else {
                    centered[row * variables + column] = 0.0;
                }
            }
        }
    }

    let decomposition = svd_sorted(
        &centered,
        observations,
        variables,
        SVD_TOLERANCE,
        max_sweeps,
        ctx,
    )?;

    let mut eigenvalues = Vec::with_capacity(components);
    for singular in &decomposition.s {
        eigenvalues.push(singular * singular / (observations as f64 - 1.0));
    }
    let total: f64 = eigenvalues.iter().sum();
    let explained_variance_ratio: Vec<f64> = if total > 0.0 {
        eigenvalues.iter().map(|value| value / total).collect()
    } else {
        vec![0.0; components]
    };

    let mut component_matrix = vec![0.0f64; components * variables];
    for component in 0..components {
        for variable in 0..variables {
            component_matrix[component * variables + variable] =
                decomposition.v[variable * components + component];
        }
    }
    let mut scores = vec![0.0f64; observations * components];
    for row in 0..observations {
        for component in 0..components {
            scores[row * components + component] =
                decomposition.u[row * components + component] * decomposition.s[component];
        }
    }

    let value = Value::record([
        ("means", float_vector_value(&means)?),
        ("stddevs", float_vector_value(&stddevs)?),
        ("eigenvalues", float_vector_value(&eigenvalues)?),
        ("explained_variance", float_vector_value(&eigenvalues)?),
        (
            "explained_variance_ratio",
            float_vector_value(&explained_variance_ratio)?,
        ),
        (
            "components",
            matrix_from_floats(components, variables, component_matrix)?.to_value(),
        ),
        (
            "scores",
            matrix_from_floats(observations, components, scores)?.to_value(),
        ),
        ("method", Value::text("svd_pca")),
    ]);
    Ok(Outcome::approximate(value))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub(crate) fn register(functions: &mut Vec<Arc<dyn Function>>) {
    functions.push(SimpleFunction::arc(pca_descriptor(), invoke_pca));
}

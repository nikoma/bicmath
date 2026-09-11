# Linear Algebra module reference

Checked vectors and matrices, exact and scientific operations, decompositions, and solving.

- Module id: `linear_algebra`
- Version: 1.0.0
- Capabilities: exact_rational_linear_algebra, lu, qr, solve, least_squares, eigenvalues, svd, power_iteration, pca
- Supported modes: exact, auto, scientific
- Functions: 25

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="condition"></a>

## condition_number_estimate

1-norm condition number of a square float64 matrix.

Scientific mode only. Computes ||A||_1 * max_j ||A^{-1} e_j||_1. The result is an estimate of conditioning; a large value means a solve may lose accuracy.

- Module: `linear_algebra` (version 1.0.0)
- Modes: scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#condition

## Parameters

- `matrix` — Square float64 matrix. (matrix)

## Output

Record with condition_number, norm, method.

## Examples

### identity conditioning in scientific mode

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "1"
      }
    ]
  }
}
```

Expected: `{"type":"contains","text":"condition_number"}`


<a id="determinant"></a>

## determinant

Determinant of a square matrix.

Exact for integer/rational/decimal inputs using rational elimination with partial pivoting; float64 requires scientific mode and uses LU.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#determinant

## Parameters

- `matrix` — Square matrix. (matrix)

## Output

Determinant.

## Examples

### integer determinant

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"-2"}}`

### singular determinant

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"0"}}`


<a id="eigen-symmetric"></a>

## eigen_symmetric

Eigenvalues and orthonormal eigenvectors of a real symmetric matrix.

Cyclic Jacobi rotations diagonalize a real symmetric matrix. Eigenvalues are returned in ascending order and the matching orthonormal eigenvectors are the columns of the vectors matrix, so A*V = V*diag(values). Scientific mode only: exact inputs are converted explicitly to float64 in that mode. A matrix that is not symmetric within the tolerance is rejected. Non-convergence within max_sweeps is reported as non_convergence. Defaults: tolerance 1e-12 and 100 sweeps.

- Module: `linear_algebra` (version 1.0.0)
- Modes: scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#eigen-symmetric

## Parameters

- `matrix` — Real symmetric square matrix. (matrix)
- `tolerance` (optional) — Off-diagonal convergence tolerance relative to the Frobenius norm; default 1e-12. (number)
- `max_sweeps` (optional) — Maximum Jacobi sweeps; default 100, never above the context iteration limit. (integer)

## Output

Record with values, vectors, sweeps, converged, residual_norm, method.

## Examples

### symmetric 2x2 matrix

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      }
    ]
  }
}
```

Expected: `{"type":"contains","text":"jacobi_rotation"}`


<a id="constructors"></a>

## identity

n x n identity matrix.

Exact.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#constructors

## Parameters

- `n` — Matrix size. (integer)

## Output

Identity matrix.

## Examples

### 2x2 identity

```json
{
  "n": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"kind":"matrix","rows":2,"cols":2,"data":[{"kind":"integer","value":"1"},{"kind":"integer","value":"0"},{"kind":"integer","value":"0"},{"kind":"integer","value":"1"}]}}`


<a id="least-squares"></a>

## least_squares

QR-based least-squares solution (scientific mode).

Solves min ||A x - b||_2 for tall matrices using Householder QR. Rank-deficient or underdetermined systems are rejected.

- Module: `linear_algebra` (version 1.0.0)
- Modes: scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#least-squares

## Parameters

- `a` — Design matrix (rows >= cols). (matrix)
- `b` — Observation vector or matrix. (any value)

## Output

Record with solution, residual_norm, rank, method.

## Examples

### line fit in scientific mode

```json
{
  "a": {
    "kind": "matrix",
    "rows": 3,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      }
    ]
  },
  "b": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "3"
    }
  ]
}
```

Expected: `{"type":"contains","text":"solution"}`


<a id="lu"></a>

## lu

Pivoted LU factorization with partial pivoting.

Returns L, U, the row permutation, and the permutation sign such that P*A = L*U. Exact rational elimination for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#lu

## Parameters

- `matrix` — Square matrix. (matrix)

## Output

Record with l, u, p, sign, and method.

## Examples

### LU of a 2x2 matrix

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "3"
      }
    ]
  }
}
```

Expected: `{"type":"contains","text":"method"}`


<a id="matrix-arithmetic"></a>

## matrix_add

Elementwise matrix addition.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#matrix-arithmetic

## Parameters

- `a` — First matrix. (matrix)
- `b` — Second matrix. (matrix)

## Output

Elementwise result.

## Examples

### 2x2 matrices

```json
{
  "a": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  },
  "b": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  }
}
```

Expected: `{"type":"value","value":{"kind":"matrix","rows":2,"cols":2,"data":[{"kind":"integer","value":"2"},{"kind":"integer","value":"4"},{"kind":"integer","value":"6"},{"kind":"integer","value":"8"}]}}`


<a id="matmul"></a>

## matrix_multiply

Multiply two conformable matrices.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#matmul

## Parameters

- `a` — Left matrix. (matrix)
- `b` — Right matrix. (matrix)

## Output

Product matrix.

## Examples

### 2x2 multiplication

```json
{
  "a": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  },
  "b": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "5"
      },
      {
        "kind": "integer",
        "value": "6"
      },
      {
        "kind": "integer",
        "value": "7"
      },
      {
        "kind": "integer",
        "value": "8"
      }
    ]
  }
}
```

Expected: `{"type":"value","value":{"kind":"matrix","rows":2,"cols":2,"data":[{"kind":"integer","value":"19"},{"kind":"integer","value":"22"},{"kind":"integer","value":"43"},{"kind":"integer","value":"50"}]}}`


<a id="matrix-scale"></a>

## matrix_scale

Multiply every matrix element by a scalar.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#matrix-scale

## Parameters

- `matrix` — Input matrix. (matrix)
- `scalar` — Scalar multiplier. (number)

## Output

Scaled matrix.

## Examples

### scale a matrix

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  },
  "scalar": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"kind":"matrix","rows":2,"cols":2,"data":[{"kind":"integer","value":"2"},{"kind":"integer","value":"4"},{"kind":"integer","value":"6"},{"kind":"integer","value":"8"}]}}`


<a id="matrix-arithmetic"></a>

## matrix_sub

Elementwise matrix subtraction.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#matrix-arithmetic

## Parameters

- `a` — First matrix. (matrix)
- `b` — Second matrix. (matrix)

## Output

Elementwise result.

## Examples

### 2x2 matrices

```json
{
  "a": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  },
  "b": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  }
}
```

Expected: `{"type":"value","value":{"kind":"matrix","rows":2,"cols":2,"data":[{"kind":"integer","value":"0"},{"kind":"integer","value":"0"},{"kind":"integer","value":"0"},{"kind":"integer","value":"0"}]}}`


<a id="transpose"></a>

## matrix_transpose

Transpose a matrix.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#transpose

## Parameters

- `matrix` — Input matrix. (matrix)

## Output

Transposed matrix.

## Examples

### transpose a 2x2 matrix

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  }
}
```

Expected: `{"type":"value","value":{"kind":"matrix","rows":2,"cols":2,"data":[{"kind":"integer","value":"1"},{"kind":"integer","value":"3"},{"kind":"integer","value":"2"},{"kind":"integer","value":"4"}]}}`


<a id="matvec"></a>

## matrix_vector_multiply

Multiply a matrix by a column vector.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Quadratic
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#matvec

## Parameters

- `matrix` — Matrix. (matrix)
- `vector` — Vector. (array of number)

## Output

Result vector.

## Examples

### 2x2 times vector

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  },
  "vector": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ]
}
```

Expected: `{"type":"value","value":[{"kind":"integer","value":"3"},{"kind":"integer","value":"7"}]}`


<a id="constructors"></a>

## ones

Matrix of exact ones.

Exact.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#constructors

## Parameters

- `rows` — Row count. (integer)
- `cols` — Column count. (integer)

## Output

Ones matrix.

## Examples

### 2x2 ones

```json
{
  "cols": {
    "kind": "integer",
    "value": "2"
  },
  "rows": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"kind":"matrix","rows":2,"cols":2,"data":[{"kind":"integer","value":"1"},{"kind":"integer","value":"1"},{"kind":"integer","value":"1"},{"kind":"integer","value":"1"}]}}`


<a id="pca"></a>

## pca

Principal components, explained variance, and scores from an observation matrix.

Rows are observations and columns are variables. The data are centered and, when standardize is true, divided by their sample standard deviations so the decomposition uses the correlation matrix; otherwise the covariance matrix is used. The centered data are decomposed with the one-sided Jacobi SVD. Eigenvalues are the squared singular values divided by n-1 in descending order, components are the principal axes as rows (k x p with k = min(n, p)), and scores are the projections of the centered data onto those axes (n x k). Zero-variance variables are left as zeros when standardizing. Scientific mode only: exact inputs are converted explicitly to float64 in that mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#pca

## Parameters

- `data` — Observation matrix: rows are observations, columns variables. (matrix)
- `standardize` (optional) — When true, use the correlation matrix (divide each centered column by its sample standard deviation); default false. (boolean)

## Output

Record with means, stddevs, eigenvalues, explained_variance, explained_variance_ratio, components, scores, method.

## Examples

### perfectly correlated 2D data

```json
{
  "data": {
    "kind": "matrix",
    "rows": 3,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "4"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "6"
      }
    ]
  }
}
```

Expected: `{"type":"contains","text":"svd_pca"}`


<a id="power-iteration"></a>

## power_iteration

Dominant eigenvalue and eigenvector of a square matrix.

Iterates x <- A*x/||A*x|| from the uniform unit vector and estimates the dominant eigenvalue with the Rayleigh quotient. Converges when the residual ||A*x - lambda*x|| is within tolerance of max(1, |lambda|). Scientific mode only: exact inputs are converted explicitly to float64 in that mode. A matrix whose dominant eigenpair is not reachable from the uniform vector, or that does not converge within max_iterations, is reported as non_convergence. Defaults: tolerance 1e-12 and 1000 iterations.

- Module: `linear_algebra` (version 1.0.0)
- Modes: scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#power-iteration

## Parameters

- `matrix` — Square matrix. (matrix)
- `tolerance` (optional) — Residual tolerance; default 1e-12. (number)
- `max_iterations` (optional) — Iteration cap; default 1000, never above the context iteration limit. (integer)

## Output

Record with eigenvalue, eigenvector, iterations, converged, method.

## Examples

### dominant eigenvalue of a diagonal matrix

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "1"
      }
    ]
  }
}
```

Expected: `{"type":"contains","text":"power_iteration"}`


<a id="qr"></a>

## qr

Householder QR factorization (scientific mode).

Returns reduced Q and R with Q orthonormal and Q*R = A. Scientific mode and float64 inputs only; exact inputs must be converted explicitly.

- Module: `linear_algebra` (version 1.0.0)
- Modes: scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#qr

## Parameters

- `matrix` — Matrix with rows >= cols. (matrix)

## Output

Record with q, r, and method.

## Examples

### QR requires scientific mode

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  }
}
```

Expected: `{"type":"error","code":"unsupported_numeric_mode"}`


<a id="solve"></a>

## solve

Solve A x = b for vector or matrix b.

Exact rational Gaussian elimination with partial pivoting for exact inputs; float64 requires scientific mode. Returns the solution, residual infinity norm, method, and pivots. Singular systems are rejected; the inverse is never formed explicitly.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Cubic
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#solve

## Parameters

- `a` — Square coefficient matrix. (matrix)
- `b` — Right-hand side vector or matrix. (any value)
- `tolerance` (optional) — Relative pivot tolerance for the scientific path (default 1e-12). (number)

## Output

Record with solution, residual_norm, method.

## Examples

### exact 2x2 solve

```json
{
  "a": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "3"
      }
    ]
  },
  "b": [
    {
      "kind": "integer",
      "value": "5"
    },
    {
      "kind": "integer",
      "value": "10"
    }
  ]
}
```

Expected: `{"type":"contains","text":"solution"}`

### singular system

```json
{
  "a": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  },
  "b": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ]
}
```

Expected: `{"type":"error","code":"singular_matrix"}`


<a id="svd"></a>

## svd

One-sided Jacobi SVD with singular values in descending order.

Computes the economy SVD A = U*diag(s)*V^T with U (m x k), V (n x k), and k = min(m, n). Columns of U and V are orthonormal; zero singular values yield zero columns of U. Scientific mode only: exact inputs are converted explicitly to float64 in that mode. Non-convergence within max_sweeps is reported as non_convergence. Defaults: tolerance 1e-12 and 100 sweeps.

- Module: `linear_algebra` (version 1.0.0)
- Modes: scientific
- Cost: Iterative
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#svd

## Parameters

- `matrix` — Matrix to decompose. (matrix)
- `tolerance` (optional) — Column-orthogonality tolerance; default 1e-12. (number)
- `max_sweeps` (optional) — Maximum Jacobi sweeps; default 100, never above the context iteration limit. (integer)

## Output

Record with u, s (descending), v, sweeps, converged, method.

## Examples

### 3x2 matrix

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 3,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      },
      {
        "kind": "integer",
        "value": "5"
      },
      {
        "kind": "integer",
        "value": "6"
      }
    ]
  }
}
```

Expected: `{"type":"contains","text":"one_sided_jacobi"}`


<a id="trace"></a>

## trace

Sum of diagonal elements of a square matrix.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#trace

## Parameters

- `matrix` — Square matrix. (matrix)

## Output

Trace.

## Examples

### trace

```json
{
  "matrix": {
    "kind": "matrix",
    "rows": 2,
    "cols": 2,
    "data": [
      {
        "kind": "integer",
        "value": "1"
      },
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "3"
      },
      {
        "kind": "integer",
        "value": "4"
      }
    ]
  }
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"5"}}`


<a id="vector-arithmetic"></a>

## vector_add

Elementwise vector addition.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#vector-arithmetic

## Parameters

- `a` — First vector. (array of number)
- `b` — Second vector. (array of number)

## Output

Elementwise result.

## Examples

### small vectors

```json
{
  "a": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ],
  "b": [
    {
      "kind": "integer",
      "value": "3"
    },
    {
      "kind": "integer",
      "value": "4"
    }
  ]
}
```

Expected: `{"type":"value","value":[{"kind":"integer","value":"4"},{"kind":"integer","value":"6"}]}`


<a id="dot"></a>

## vector_dot

Inner product of two equal-length vectors.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#dot

## Parameters

- `a` — First vector. (array of number)
- `b` — Second vector. (array of number)

## Output

Scalar dot product.

## Examples

### dot product

```json
{
  "a": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "3"
    }
  ],
  "b": [
    {
      "kind": "integer",
      "value": "4"
    },
    {
      "kind": "integer",
      "value": "5"
    },
    {
      "kind": "integer",
      "value": "6"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"32"}}`


<a id="norm"></a>

## vector_norm

1, 2, or infinity norm of a vector.

The 2-norm is exact when the sum of squares has an exact square root; otherwise exact mode errors, auto mode returns an approximate decimal, and scientific mode returns float64.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#norm

## Parameters

- `vector` — Input vector. (array of number)
- `order` (optional) — Norm order: "2" (default), "1", or "inf". (one of ["1", "2", "inf"])

## Output

Norm value.

## Examples

### 3-4-5 triangle

```json
{
  "vector": [
    {
      "kind": "integer",
      "value": "3"
    },
    {
      "kind": "integer",
      "value": "4"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"5"}}`


<a id="vector-scale"></a>

## vector_scale

Multiply every vector element by a scalar.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#vector-scale

## Parameters

- `vector` — Input vector. (array of number)
- `scalar` — Scalar multiplier. (number)

## Output

Scaled vector.

## Examples

### scale a vector

```json
{
  "scalar": {
    "kind": "integer",
    "value": "2"
  },
  "vector": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "3"
    }
  ]
}
```

Expected: `{"type":"value","value":[{"kind":"integer","value":"2"},{"kind":"integer","value":"4"},{"kind":"integer","value":"6"}]}`


<a id="vector-arithmetic"></a>

## vector_sub

Elementwise vector subtraction.

Exact for exact inputs; float64 requires scientific mode.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#vector-arithmetic

## Parameters

- `a` — First vector. (array of number)
- `b` — Second vector. (array of number)

## Output

Elementwise result.

## Examples

### small vectors

```json
{
  "a": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ],
  "b": [
    {
      "kind": "integer",
      "value": "3"
    },
    {
      "kind": "integer",
      "value": "4"
    }
  ]
}
```

Expected: `{"type":"value","value":[{"kind":"integer","value":"-2"},{"kind":"integer","value":"-2"}]}`


<a id="constructors"></a>

## zeros

Matrix of exact zeros.

Exact.

- Module: `linear_algebra` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/linear_algebra.md#constructors

## Parameters

- `rows` — Row count. (integer)
- `cols` — Column count. (integer)

## Output

Zero matrix.

## Examples

### 2x3 zeros

```json
{
  "cols": {
    "kind": "integer",
    "value": "3"
  },
  "rows": {
    "kind": "integer",
    "value": "2"
  }
}
```

Expected: `{"type":"value","value":{"kind":"matrix","rows":2,"cols":3,"data":[{"kind":"integer","value":"0"},{"kind":"integer","value":"0"},{"kind":"integer","value":"0"},{"kind":"integer","value":"0"},{"kind":"integer","value":"0"},{"kind":"integer","value":"0"}]}}`



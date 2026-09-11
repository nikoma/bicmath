# Geometry module reference

Planar geometry, triangle solving, spherical geodesy, and 3D quaternion rotations.

- Module id: `geometry`
- Version: 1.0.0
- Capabilities: exact_planar_geometry, triangle_solving, spherical_geodesy, quaternion_rotation
- Supported modes: exact, auto, scientific
- Functions: 23

This file is generated from the live registry by `bicmath docs`; the function schemas, domains, and examples are the same ones the engine validates against at runtime.

<a id="circle"></a>

## circle

Area and circumference of a circle.

Returns `area = pi r^2` and `circumference = 2 pi r`. Because pi is irrational the results are approximate: decimal approximations in auto mode and float64 in scientific mode, with an error estimate for the binary64 value of pi. Exact mode is rejected.

- Module: `geometry` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#circle

## Parameters

- `radius` — Non-negative radius. (number)

## Output

Area and circumference.

## Examples

### unit circle

```json
{
  "radius": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"contains","text":"circumference"}`


<a id="destination-point"></a>

## destination_point

Point reached by travelling a distance along a great-circle bearing.

Starting from `origin` (`[longitude, latitude]` in degrees), travel `distance_km` along the initial great-circle bearing `bearing_degrees`. The result is a spherical approximation on the given radius (default 6371.0088 km); exact mode is rejected. Longitudes are normalized to [-180, 180).

- Module: `geometry` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#destination-point

## Parameters

- `origin` — Origin point `[longitude, latitude]` in degrees. (array of number)
- `bearing_degrees` — Initial bearing in degrees. (number)
- `distance_km` — Distance to travel in kilometres. (number)
- `radius` (optional) — Sphere radius in kilometres. (number)

## Output

Destination `[longitude, latitude]` with method metadata.

## Examples

### travel north from London

```json
{
  "bearing_degrees": {
    "kind": "integer",
    "value": "0"
  },
  "distance_km": {
    "kind": "integer",
    "value": "100"
  },
  "origin": [
    {
      "kind": "decimal",
      "value": "-0.1276"
    },
    {
      "kind": "decimal",
      "value": "51.5074"
    }
  ]
}
```

Expected: `{"type":"contains","text":"destination_point_sphere"}`


<a id="distance-2d"></a>

## distance_2d

Euclidean distance between two 2D points.

Points are arrays of exact numbers. The distance is returned exactly when the squared distance is a perfect rational square; otherwise auto mode returns a decimal approximation, scientific mode returns float64, and exact mode is an error. Float64 inputs require scientific mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#distance-2d

## Parameters

- `p` — First point. (array of number)
- `q` — Second point. (array of number)

## Output

Distance between p and q, always non-negative.

## Examples

### 3-4-5 distance

```json
{
  "p": [
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "integer",
      "value": "0"
    }
  ],
  "q": [
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


<a id="distance-3d"></a>

## distance_3d

Euclidean distance between two 3D points.

Points are arrays of exact numbers. The distance is returned exactly when the squared distance is a perfect rational square; otherwise auto mode returns a decimal approximation, scientific mode returns float64, and exact mode is an error. Float64 inputs require scientific mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#distance-3d

## Parameters

- `p` — First point. (array of number)
- `q` — Second point. (array of number)

## Output

Distance between p and q, always non-negative.

## Examples

### 2-3-6 distance

```json
{
  "p": [
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
      "value": "0"
    }
  ],
  "q": [
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
      "value": "6"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"7"}}`


<a id="euler-to-quaternion"></a>

## euler_to_quaternion

Convert intrinsic Euler angles to a quaternion.

Angles are in radians and name rotations about the x (roll), y (pitch), and z (yaw) axes. `order` lists the axes in intrinsic application order and defaults to `zyx` (yaw, then pitch, then roll). The six Tait-Bryan orders are supported. The identity (all angles zero) is exact; other inputs are approximate because sine and cosine are transcendental.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#euler-to-quaternion

## Parameters

- `roll` — Rotation about x, in radians. (number)
- `pitch` — Rotation about y, in radians. (number)
- `yaw` — Rotation about z, in radians. (number)
- `order` (optional) — Intrinsic axis order. (one of ["xyz", "xzy", "yxz", "yzx", "zxy", "zyx"])

## Output

Quaternion `[w, x, y, z]`.

## Examples

### identity

```json
{
  "pitch": {
    "kind": "integer",
    "value": "0"
  },
  "roll": {
    "kind": "integer",
    "value": "0"
  },
  "yaw": {
    "kind": "integer",
    "value": "0"
  }
}
```

Expected: `{"type":"value","value":[{"kind":"integer","value":"1"},{"kind":"integer","value":"0"},{"kind":"integer","value":"0"},{"kind":"integer","value":"0"}]}`


<a id="haversine"></a>

## haversine

Great-circle distance between two points on a sphere.

Points are `[longitude, latitude]` in degrees. The haversine formula is evaluated in binary64 on a sphere whose radius defaults to 6371.0088 km (mean Earth radius). The result is labelled approximate: the spherical model ignores ellipsoid flattening. Exact mode is rejected.

- Module: `geometry` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#haversine

## Parameters

- `point_a` — First point `[longitude, latitude]` in degrees. (array of number)
- `point_b` — Second point `[longitude, latitude]` in degrees. (array of number)
- `radius` (optional) — Sphere radius in kilometres. (number)

## Output

Spherical distance in kilometres with method metadata.

## Examples

### London to Paris

```json
{
  "point_a": [
    {
      "kind": "decimal",
      "value": "-0.1276"
    },
    {
      "kind": "decimal",
      "value": "51.5074"
    }
  ],
  "point_b": [
    {
      "kind": "decimal",
      "value": "2.3522"
    },
    {
      "kind": "decimal",
      "value": "48.8566"
    }
  ]
}
```

Expected: `{"type":"contains","text":"haversine_sphere"}`


<a id="initial-bearing"></a>

## initial_bearing

Initial great-circle bearing from one point to another on a sphere.

Points are `[longitude, latitude]` in degrees. The bearing is returned in degrees, normalized to [0, 360). The result is approximate; exact mode is rejected.

- Module: `geometry` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#initial-bearing

## Parameters

- `a` — Starting point `[longitude, latitude]` in degrees. (array of number)
- `b` — Destination point `[longitude, latitude]` in degrees. (array of number)

## Output

Initial bearing in degrees with method metadata.

## Examples

### London to Paris bearing

```json
{
  "a": [
    {
      "kind": "decimal",
      "value": "-0.1276"
    },
    {
      "kind": "decimal",
      "value": "51.5074"
    }
  ],
  "b": [
    {
      "kind": "decimal",
      "value": "2.3522"
    },
    {
      "kind": "decimal",
      "value": "48.8566"
    }
  ]
}
```

Expected: `{"type":"contains","text":"initial_bearing_sphere"}`


<a id="line-intersection-2d"></a>

## line_intersection_2d

Intersection of two infinite 2D lines.

Each line is given by two distinct points. The intersection is an exact rational pair when the inputs are exact. Parallel lines return a null intersection; coincident lines set `coincident` true. Float64 inputs require scientific mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#line-intersection-2d

## Parameters

- `a1` — First point of line A. (array of number)
- `a2` — Second point of line A. (array of number)
- `b1` — First point of line B. (array of number)
- `b2` — Second point of line B. (array of number)

## Output

Intersection `[x, y]` or null, plus parallel and coincident flags.

## Examples

### diagonals of a square

```json
{
  "a1": [
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "integer",
      "value": "0"
    }
  ],
  "a2": [
    {
      "kind": "integer",
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ],
  "b1": [
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "integer",
      "value": "2"
    }
  ],
  "b2": [
    {
      "kind": "integer",
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "0"
    }
  ]
}
```

Expected: `{"type":"value","value":{"coincident":false,"intersection":[{"kind":"integer","value":"1"},{"kind":"integer","value":"1"}],"parallel":false}}`


<a id="point-in-polygon"></a>

## point_in_polygon

Exact ray-casting point-in-polygon test.

A horizontal ray to the right is cast from the point and crossings are counted with exact rational arithmetic. Points exactly on the boundary count as inside. Float64 inputs require scientific mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#point-in-polygon

## Parameters

- `point` — Query point `[x, y]`. (array of number)
- `vertices` — Polygon vertices in order. (array of array of number)

## Output

True when the point lies inside or on the boundary.

## Examples

### centre of a square

```json
{
  "point": [
    {
      "kind": "integer",
      "value": "1"
    },
    {
      "kind": "integer",
      "value": "1"
    }
  ],
  "vertices": [
    [
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "2"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "2"
      }
    ]
  ]
}
```

Expected: `{"type":"value","value":true}`


<a id="point-line-distance-2d"></a>

## point_line_distance_2d

Perpendicular distance from a point to the infinite line through two points.

Computes `|cross(end - start, point - start)| / |end - start|`, the perpendicular distance to the infinite line. Exact when the denominator is a perfect rational square; otherwise auto mode returns a decimal approximation and exact mode is an error. The two line points must be distinct.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#point-line-distance-2d

## Parameters

- `point` — Query point `[x, y]`. (array of number)
- `line_start` — First point of the line. (array of number)
- `line_end` — Second point of the line. (array of number)

## Output

Non-negative perpendicular distance.

## Examples

### point above a horizontal line

```json
{
  "line_end": [
    {
      "kind": "integer",
      "value": "10"
    },
    {
      "kind": "integer",
      "value": "0"
    }
  ],
  "line_start": [
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "integer",
      "value": "0"
    }
  ],
  "point": [
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "integer",
      "value": "5"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"5"}}`


<a id="polygon-area"></a>

## polygon_area

Signed shoelace area of a simple polygon.

Vertices are `[x, y]` pairs in order. The signed area is positive for counter-clockwise winding and exact for exact inputs (including lattice polygons); float64 inputs require scientific mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#polygon-area

## Parameters

- `vertices` — Polygon vertices in order. (array of array of number)

## Output

Signed polygon area.

## Examples

### 4x3 rectangle area

```json
{
  "vertices": [
    [
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "4"
      },
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "4"
      },
      {
        "kind": "integer",
        "value": "3"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "3"
      }
    ]
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"12"}}`


<a id="polygon-centroid"></a>

## polygon_centroid

Area centroid of a simple polygon via the shoelace formula.

The centroid is `(sum (x_i + x_{i+1}) * cross_i / (6 A), ...)` where `A` is the signed area. Degenerate polygons with zero area are a domain error. Exact for exact inputs.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#polygon-centroid

## Parameters

- `vertices` — Polygon vertices in order. (array of array of number)

## Output

Area centroid coordinates.

## Examples

### unit square centroid

```json
{
  "vertices": [
    [
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "2"
      },
      {
        "kind": "integer",
        "value": "2"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "2"
      }
    ]
  ]
}
```

Expected: `{"type":"value","value":{"x":{"kind":"integer","value":"1"},"y":{"kind":"integer","value":"1"}}}`


<a id="polygon-perimeter"></a>

## polygon_perimeter

Perimeter of a polygon given its vertices in order.

Each edge length is exact when its squared length is a perfect rational square. Edges that require a square root are summed as decimal approximations in auto mode and as float64 in scientific mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Linear
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#polygon-perimeter

## Parameters

- `vertices` — Polygon vertices in order. (array of array of number)

## Output

Perimeter of the polygon.

## Examples

### 4x3 rectangle perimeter

```json
{
  "vertices": [
    [
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "4"
      },
      {
        "kind": "integer",
        "value": "0"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "4"
      },
      {
        "kind": "integer",
        "value": "3"
      }
    ],
    [
      {
        "kind": "integer",
        "value": "0"
      },
      {
        "kind": "integer",
        "value": "3"
      }
    ]
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"14"}}`


<a id="quaternion-conjugate"></a>

## quaternion_conjugate

Conjugate of a quaternion: `[w, -x, -y, -z]`.

Exact for exact inputs; float64 inputs require scientific mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#quaternion-conjugate

## Parameters

- `q` — Quaternion `[w, x, y, z]`. (array of number)

## Output

Conjugate of q.

## Examples

### conjugate

```json
{
  "q": [
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
```

Expected: `{"type":"value","value":[{"kind":"integer","value":"1"},{"kind":"integer","value":"-2"},{"kind":"integer","value":"-3"},{"kind":"integer","value":"-4"}]}`


<a id="quaternion-multiply"></a>

## quaternion_multiply

Hamilton product of two quaternions.

Quaternions are `[w, x, y, z]` with the scalar part first. The product is exact for exact inputs. Float64 inputs require scientific mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#quaternion-multiply

## Parameters

- `a` — Left quaternion `[w, x, y, z]`. (array of number)
- `b` — Right quaternion `[w, x, y, z]`. (array of number)

## Output

Hamilton product a * b.

## Examples

### identity product

```json
{
  "a": [
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
      "value": "0"
    }
  ],
  "b": [
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
      "value": "2"
    },
    {
      "kind": "integer",
      "value": "3"
    }
  ]
}
```

Expected: `{"type":"value","value":[{"kind":"integer","value":"0"},{"kind":"integer","value":"1"},{"kind":"integer","value":"2"},{"kind":"integer","value":"3"}]}`


<a id="quaternion-norm"></a>

## quaternion_norm

Euclidean norm of a quaternion.

The squared norm is computed exactly; the norm is exact when that square is a perfect rational square, a decimal approximation in auto mode otherwise, and an error in exact mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#quaternion-norm

## Parameters

- `q` — Quaternion `[w, x, y, z]`. (array of number)

## Output

Non-negative norm.

## Examples

### 3-4-5 norm

```json
{
  "q": [
    {
      "kind": "integer",
      "value": "3"
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
      "value": "4"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"5"}}`


<a id="quaternion-normalize"></a>

## quaternion_normalize

Unit quaternion in the same direction.

Divides by the norm. Exact when the squared norm is a perfect rational square; otherwise components are decimal approximations in auto mode and float64 in scientific mode. The zero quaternion is a domain error.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#quaternion-normalize

## Parameters

- `q` — Non-zero quaternion `[w, x, y, z]`. (array of number)

## Output

Unit quaternion.

## Examples

### normalize 3-4-5

```json
{
  "q": [
    {
      "kind": "integer",
      "value": "3"
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
      "value": "4"
    }
  ]
}
```

Expected: `{"type":"value","value":[{"kind":"rational","numerator":"3","denominator":"5"},{"kind":"integer","value":"0"},{"kind":"integer","value":"0"},{"kind":"rational","numerator":"4","denominator":"5"}]}`


<a id="quaternion-rotate"></a>

## quaternion_rotate

Rotate a 3D vector by a quaternion.

Computes `(q * (0, v) * conjugate(q)) / |q|^2`, which rotates `v` by the rotation the quaternion represents and does not require `q` to be unit length. Exact for exact inputs; the zero quaternion is a domain error.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#quaternion-rotate

## Parameters

- `q` — Rotation quaternion `[w, x, y, z]`. (array of number)
- `vector` — Vector to rotate `[x, y, z]`. (array of number)

## Output

Rotated vector.

## Examples

### identity rotation

```json
{
  "q": [
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
      "value": "0"
    }
  ],
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

Expected: `{"type":"value","value":[{"kind":"integer","value":"1"},{"kind":"integer","value":"2"},{"kind":"integer","value":"3"}]}`


<a id="quaternion-to-euler"></a>

## quaternion_to_euler

Convert a quaternion to intrinsic Euler angles in radians.

The quaternion is normalized before extraction. Angles are returned in radians as `roll` (x), `pitch` (y), and `yaw` (z). `order` lists the axes in intrinsic application order and defaults to `zyx`. Gimbal-lock cases set the third angle to zero. Always approximate; exact mode is rejected.

- Module: `geometry` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#quaternion-to-euler

## Parameters

- `q` — Non-zero quaternion `[w, x, y, z]`. (array of number)
- `order` (optional) — Intrinsic axis order. (one of ["xyz", "xzy", "yxz", "yzx", "zxy", "zyx"])

## Output

Intrinsic Euler angles in radians.

## Examples

### identity quaternion

```json
{
  "q": [
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
      "value": "0"
    }
  ]
}
```

Expected: `{"type":"contains","text":"roll"}`


<a id="sphere"></a>

## sphere

Volume and surface area of a sphere.

Returns `volume = (4/3) pi r^3` and `surface_area = 4 pi r^2`. Because pi is irrational the results are approximate: decimal approximations in auto mode and float64 in scientific mode, with an error estimate for the binary64 value of pi. Exact mode is rejected.

- Module: `geometry` (version 1.0.0)
- Modes: auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#sphere

## Parameters

- `radius` — Non-negative radius. (number)

## Output

Volume and surface area.

## Examples

### unit sphere

```json
{
  "radius": {
    "kind": "integer",
    "value": "1"
  }
}
```

Expected: `{"type":"contains","text":"surface_area"}`


<a id="squared-distance-2d"></a>

## squared_distance_2d

Exact squared Euclidean distance between two points in 2 dimensions.

The sum of squared coordinate differences is computed exactly for exact inputs. Float64 inputs require scientific mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#squared-distance-2d

## Parameters

- `p` — First point. (array of number)
- `q` — Second point. (array of number)

## Output

Exact squared distance.

## Examples

### 3-4-5 squared distance

```json
{
  "p": [
    {
      "kind": "integer",
      "value": "0"
    },
    {
      "kind": "integer",
      "value": "0"
    }
  ],
  "q": [
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

Expected: `{"type":"value","value":{"kind":"integer","value":"25"}}`


<a id="squared-distance-3d"></a>

## squared_distance_3d

Exact squared Euclidean distance between two points in 3 dimensions.

The sum of squared coordinate differences is computed exactly for exact inputs. Float64 inputs require scientific mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#squared-distance-3d

## Parameters

- `p` — First point. (array of number)
- `q` — Second point. (array of number)

## Output

Exact squared distance.

## Examples

### 2-3-6 squared distance

```json
{
  "p": [
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
      "value": "0"
    }
  ],
  "q": [
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
      "value": "6"
    }
  ]
}
```

Expected: `{"type":"value","value":{"kind":"integer","value":"49"}}`


<a id="triangle-solve"></a>

## triangle_solve

Solve a triangle from any three of its sides and angles.

Supply exactly three of a, b, c, angle_a, angle_b, angle_c inside `givens`. Supported configurations are SSS, SAS, ASA/AAS, and SSA. Angles are in `angle_unit` (default degrees) and must lie strictly between 0 and 180 degrees (0 and pi radians). The triangle inequality is validated. Heron's formula supplies the area. An ambiguous SSA configuration — two distinct valid triangles — is rejected with a domain error instead of silently picking one; `solutions_count` is therefore always 1 and `valid` always true for a successful result, while invalid inputs return an error rather than `valid: false`. Exact inputs yield exact sides, area, and perimeter where the algebra allows; angles that require inverse trigonometry are decimal approximations in auto mode, float64 in scientific mode, and an error in exact mode.

- Module: `geometry` (version 1.0.0)
- Modes: exact, auto, scientific
- Cost: Constant
- Determinism: Deterministic
- Method reference: docs/methods/geometry.md#triangle-solve

## Parameters

- `givens` — Record with exactly three of a, b, c, angle_a, angle_b, angle_c. (record with fields a, b, c, angle_a, angle_b, angle_c)
- `angle_unit` (optional) — Angle unit for the given and returned angles. (one of ["degrees", "radians"])

## Output

Solved sides, angles, area, perimeter, method, and solution count.

## Examples

### SSS 3-4-5

```json
{
  "givens": {
    "a": {
      "kind": "integer",
      "value": "3"
    },
    "b": {
      "kind": "integer",
      "value": "4"
    },
    "c": {
      "kind": "integer",
      "value": "5"
    }
  }
}
```

Expected: `{"type":"contains","text":"SSS"}`

### invalid triangle

```json
{
  "givens": {
    "a": {
      "kind": "integer",
      "value": "1"
    },
    "b": {
      "kind": "integer",
      "value": "2"
    },
    "c": {
      "kind": "integer",
      "value": "10"
    }
  }
}
```

Expected: `{"type":"error","code":"domain_violation"}`



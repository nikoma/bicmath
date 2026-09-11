//! Planar geometry: distances, polygons, point classification, and line
//! intersections. All planar operations are exact for exact rational inputs.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Function, FunctionDescriptor, Outcome, ParamDescriptor, SimpleFunction,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::common::{
    all_modes, descriptor, field, number_schema, record_schema, value_example, vector_schema,
    vertices_schema,
};
use crate::scalar::{Scalar, any_float, parse_vector, sqrt_to_value};

fn parse_vertices(
    args: &Args,
    name: &str,
    ctx: &ExecContext,
) -> Result<Vec<Vec<Scalar>>, EngineError> {
    let items = args.array(name)?;
    if items.len() < 3 {
        return Err(EngineError::malformed(format!(
            "{name} must contain at least three vertices, found {}",
            items.len()
        ))
        .with_path(name.to_string()));
    }
    if items.len() > ctx.limits.max_array_len {
        return Err(EngineError::new(
            ErrorCode::ResourceLimit,
            format!(
                "polygon has {} vertices, exceeding the limit of {}",
                items.len(),
                ctx.limits.max_array_len
            ),
        ));
    }
    let mut out = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let vertex = item
            .as_array()
            .map_err(|error| error.with_path(format!("{name}[{index}]")))?;
        if vertex.len() != 2 {
            return Err(EngineError::malformed(format!(
                "vertex {name}[{index}] must have exactly two coordinates, found {}",
                vertex.len()
            ))
            .with_path(format!("{name}[{index}]")));
        }
        let mut point = Vec::with_capacity(2);
        for (axis, coordinate) in vertex.iter().enumerate() {
            point.push(Scalar::from_value(
                coordinate,
                &format!("{name}[{index}][{axis}]"),
                ctx,
            )?);
        }
        out.push(point);
    }
    Ok(out)
}

fn squared_distance(p: &[Scalar], q: &[Scalar]) -> Scalar {
    let mut total = Scalar::zero();
    for (left, right) in p.iter().zip(q.iter()) {
        let delta = left.sub(right);
        total = total.add(&delta.mul(&delta));
    }
    total
}

fn cross(a: &[Scalar], b: &[Scalar]) -> Scalar {
    a[0].mul(&b[1]).sub(&a[1].mul(&b[0]))
}

fn exactness(approximate: bool) -> Exactness {
    if approximate {
        Exactness::Approximate
    } else {
        Exactness::Exact
    }
}

fn distance_impl(args: &Args, dimension: usize, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let p = parse_vector(args, "p", dimension, ctx)?;
    let q = parse_vector(args, "q", dimension, ctx)?;
    let squared = squared_distance(&p, &q);
    let (value, approximate) = sqrt_to_value(&squared, ctx)?;
    Ok(Outcome::new(value, exactness(approximate)))
}

fn squared_distance_impl(
    args: &Args,
    dimension: usize,
    ctx: &ExecContext,
) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let p = parse_vector(args, "p", dimension, ctx)?;
    let q = parse_vector(args, "q", dimension, ctx)?;
    let squared = squared_distance(&p, &q);
    let approximate = squared.is_float();
    Ok(Outcome::new(squared.to_value(ctx)?, exactness(approximate)))
}

fn distance_descriptor(
    id: &str,
    dimension: usize,
    title: &str,
    method: &str,
) -> FunctionDescriptor {
    descriptor(
        id,
        title,
        &format!("Euclidean distance between two {dimension}D points."),
        method,
    )
    .with_description(
        "Points are arrays of exact numbers. The distance is returned exactly when the \
         squared distance is a perfect rational square; otherwise auto mode returns a \
         decimal approximation, scientific mode returns float64, and exact mode is an error. \
         Float64 inputs require scientific mode.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("p", "First point.", vector_schema(dimension)),
        ParamDescriptor::required("q", "Second point.", vector_schema(dimension)),
    ])
    .with_output(
        number_schema(),
        "Distance between p and q, always non-negative.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
}

fn squared_distance_descriptor(
    id: &str,
    dimension: usize,
    title: &str,
    method: &str,
) -> FunctionDescriptor {
    descriptor(
        id,
        title,
        &format!("Exact squared Euclidean distance between two points in {dimension} dimensions."),
        method,
    )
    .with_description(
        "The sum of squared coordinate differences is computed exactly for exact inputs. \
         Float64 inputs require scientific mode.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("p", "First point.", vector_schema(dimension)),
        ParamDescriptor::required("q", "Second point.", vector_schema(dimension)),
    ])
    .with_output(number_schema(), "Exact squared distance.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
}

pub(crate) fn distance_2d_descriptor() -> FunctionDescriptor {
    distance_descriptor("geometry.distance_2d", 2, "2D distance", "distance-2d").with_examples(
        vec![value_example(
            "3-4-5 distance",
            &[
                ("p", serde_json::json!([0, 0])),
                ("q", serde_json::json!([3, 4])),
            ],
            serde_json::json!(5),
        )],
    )
}

pub(crate) fn distance_3d_descriptor() -> FunctionDescriptor {
    distance_descriptor("geometry.distance_3d", 3, "3D distance", "distance-3d").with_examples(
        vec![value_example(
            "2-3-6 distance",
            &[
                ("p", serde_json::json!([0, 0, 0])),
                ("q", serde_json::json!([2, 3, 6])),
            ],
            serde_json::json!(7),
        )],
    )
}

pub(crate) fn squared_distance_2d_descriptor() -> FunctionDescriptor {
    squared_distance_descriptor(
        "geometry.squared_distance_2d",
        2,
        "Squared 2D distance",
        "squared-distance-2d",
    )
    .with_examples(vec![value_example(
        "3-4-5 squared distance",
        &[
            ("p", serde_json::json!([0, 0])),
            ("q", serde_json::json!([3, 4])),
        ],
        serde_json::json!(25),
    )])
}

pub(crate) fn squared_distance_3d_descriptor() -> FunctionDescriptor {
    squared_distance_descriptor(
        "geometry.squared_distance_3d",
        3,
        "Squared 3D distance",
        "squared-distance-3d",
    )
    .with_examples(vec![value_example(
        "2-3-6 squared distance",
        &[
            ("p", serde_json::json!([0, 0, 0])),
            ("q", serde_json::json!([2, 3, 6])),
        ],
        serde_json::json!(49),
    )])
}

fn invoke_distance_2d(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    distance_impl(args, 2, ctx)
}

fn invoke_distance_3d(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    distance_impl(args, 3, ctx)
}

fn invoke_squared_distance_2d(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    squared_distance_impl(args, 2, ctx)
}

fn invoke_squared_distance_3d(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    squared_distance_impl(args, 3, ctx)
}

fn shoelace_double(vertices: &[Vec<Scalar>]) -> Scalar {
    let mut total = Scalar::zero();
    for index in 0..vertices.len() {
        let current = &vertices[index];
        let next = &vertices[(index + 1) % vertices.len()];
        total = total.add(&cross(current, next));
    }
    total
}

fn polygon_area_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.polygon_area",
        "Polygon area",
        "Signed shoelace area of a simple polygon.",
        "polygon-area",
    )
    .with_description(
        "Vertices are `[x, y]` pairs in order. The signed area is positive for \
         counter-clockwise winding and exact for exact inputs (including lattice \
         polygons); float64 inputs require scientific mode.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "vertices",
        "Polygon vertices in order.",
        vertices_schema(),
    )])
    .with_output(number_schema(), "Signed polygon area.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_examples(vec![value_example(
        "4x3 rectangle area",
        &[(
            "vertices",
            serde_json::json!([[0, 0], [4, 0], [4, 3], [0, 3]]),
        )],
        serde_json::json!(12),
    )])
}

fn polygon_perimeter_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.polygon_perimeter",
        "Polygon perimeter",
        "Perimeter of a polygon given its vertices in order.",
        "polygon-perimeter",
    )
    .with_description(
        "Each edge length is exact when its squared length is a perfect rational square. \
         Edges that require a square root are summed as decimal approximations in auto \
         mode and as float64 in scientific mode.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "vertices",
        "Polygon vertices in order.",
        vertices_schema(),
    )])
    .with_output(number_schema(), "Perimeter of the polygon.")
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_examples(vec![value_example(
        "4x3 rectangle perimeter",
        &[(
            "vertices",
            serde_json::json!([[0, 0], [4, 0], [4, 3], [0, 3]]),
        )],
        serde_json::json!(14),
    )])
}

fn polygon_centroid_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.polygon_centroid",
        "Polygon centroid",
        "Area centroid of a simple polygon via the shoelace formula.",
        "polygon-centroid",
    )
    .with_description(
        "The centroid is `(sum (x_i + x_{i+1}) * cross_i / (6 A), ...)` where `A` is the \
         signed area. Degenerate polygons with zero area are a domain error. Exact for \
         exact inputs.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "vertices",
        "Polygon vertices in order.",
        vertices_schema(),
    )])
    .with_output(
        record_schema(vec![
            field("x", number_schema()),
            field("y", number_schema()),
        ]),
        "Area centroid coordinates.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_examples(vec![value_example(
        "unit square centroid",
        &[(
            "vertices",
            serde_json::json!([[0, 0], [2, 0], [2, 2], [0, 2]]),
        )],
        serde_json::json!({"x": 1, "y": 1}),
    )])
}

fn point_in_polygon_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.point_in_polygon",
        "Point in polygon",
        "Exact ray-casting point-in-polygon test.",
        "point-in-polygon",
    )
    .with_description(
        "A horizontal ray to the right is cast from the point and crossings are counted \
         with exact rational arithmetic. Points exactly on the boundary count as inside. \
         Float64 inputs require scientific mode.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("point", "Query point `[x, y]`.", vector_schema(2)),
        ParamDescriptor::required("vertices", "Polygon vertices in order.", vertices_schema()),
    ])
    .with_output(
        ValueSchema::Bool,
        "True when the point lies inside or on the boundary.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Linear)
    .with_examples(vec![value_example(
        "centre of a square",
        &[
            ("point", serde_json::json!([1, 1])),
            (
                "vertices",
                serde_json::json!([[0, 0], [2, 0], [2, 2], [0, 2]]),
            ),
        ],
        serde_json::json!(true),
    )])
}

fn point_line_distance_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.point_line_distance_2d",
        "Point-line distance",
        "Perpendicular distance from a point to the infinite line through two points.",
        "point-line-distance-2d",
    )
    .with_description(
        "Computes `|cross(end - start, point - start)| / |end - start|`, the perpendicular \
         distance to the infinite line. Exact when the denominator is a perfect rational \
         square; otherwise auto mode returns a decimal approximation and exact mode is an \
         error. The two line points must be distinct.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("point", "Query point `[x, y]`.", vector_schema(2)),
        ParamDescriptor::required("line_start", "First point of the line.", vector_schema(2)),
        ParamDescriptor::required("line_end", "Second point of the line.", vector_schema(2)),
    ])
    .with_output(number_schema(), "Non-negative perpendicular distance.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![value_example(
        "point above a horizontal line",
        &[
            ("point", serde_json::json!([0, 5])),
            ("line_start", serde_json::json!([0, 0])),
            ("line_end", serde_json::json!([10, 0])),
        ],
        serde_json::json!(5),
    )])
}

fn line_intersection_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.line_intersection_2d",
        "Line intersection",
        "Intersection of two infinite 2D lines.",
        "line-intersection-2d",
    )
    .with_description(
        "Each line is given by two distinct points. The intersection is an exact rational \
         pair when the inputs are exact. Parallel lines return a null intersection; \
         coincident lines set `coincident` true. Float64 inputs require scientific mode.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a1", "First point of line A.", vector_schema(2)),
        ParamDescriptor::required("a2", "Second point of line A.", vector_schema(2)),
        ParamDescriptor::required("b1", "First point of line B.", vector_schema(2)),
        ParamDescriptor::required("b2", "Second point of line B.", vector_schema(2)),
    ])
    .with_output(
        record_schema(vec![
            field("intersection", ValueSchema::Any),
            field("parallel", ValueSchema::Bool),
            field("coincident", ValueSchema::Bool),
        ]),
        "Intersection `[x, y]` or null, plus parallel and coincident flags.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![value_example(
        "diagonals of a square",
        &[
            ("a1", serde_json::json!([0, 0])),
            ("a2", serde_json::json!([2, 2])),
            ("b1", serde_json::json!([0, 2])),
            ("b2", serde_json::json!([2, 0])),
        ],
        serde_json::json!({"intersection": [1, 1], "parallel": false, "coincident": false}),
    )])
}

fn invoke_polygon_area(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let vertices = parse_vertices(args, "vertices", ctx)?;
    let area = shoelace_double(&vertices).mul(&Scalar::half());
    let approximate = area.is_float();
    Ok(Outcome::new(area.to_value(ctx)?, exactness(approximate)))
}

fn invoke_polygon_perimeter(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let vertices = parse_vertices(args, "vertices", ctx)?;
    let mut total = bicmath_core::number::Number::integer(num_bigint::BigInt::from(0));
    let mut approximate = false;
    for index in 0..vertices.len() {
        ctx.check()?;
        let current = &vertices[index];
        let next = &vertices[(index + 1) % vertices.len()];
        let squared = squared_distance(current, next);
        let (value, approx) = sqrt_to_value(&squared, ctx)?;
        let number = value
            .as_number()
            .map_err(|_| EngineError::internal("edge length is not a number"))?
            .clone();
        total = total.add(&number, &ctx.numeric, &ctx.limits)?.value;
        approximate |= approx;
    }
    Ok(Outcome::new(Value::Number(total), exactness(approximate)))
}

fn invoke_polygon_centroid(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let vertices = parse_vertices(args, "vertices", ctx)?;
    let double_area = shoelace_double(&vertices);
    if double_area.is_zero() {
        return Err(EngineError::domain(
            "polygon centroid is undefined for a degenerate polygon with zero area",
        ));
    }
    let mut sum_x = Scalar::zero();
    let mut sum_y = Scalar::zero();
    for index in 0..vertices.len() {
        ctx.check()?;
        let current = &vertices[index];
        let next = &vertices[(index + 1) % vertices.len()];
        let edge_cross = cross(current, next);
        sum_x = sum_x.add(&current[0].add(&next[0]).mul(&edge_cross));
        sum_y = sum_y.add(&current[1].add(&next[1]).mul(&edge_cross));
    }
    let divisor = double_area.mul(&Scalar::exact_i64(3));
    let x = sum_x.div(&divisor)?;
    let y = sum_y.div(&divisor)?;
    let approximate = x.is_float() || y.is_float();
    let value = Value::record([("x", x.to_value(ctx)?), ("y", y.to_value(ctx)?)]);
    Ok(Outcome::new(value, exactness(approximate)))
}

fn point_on_segment(point: &[Scalar], start: &[Scalar], end: &[Scalar]) -> bool {
    let direction = [end[0].sub(&start[0]), end[1].sub(&start[1])];
    let offset = [point[0].sub(&start[0]), point[1].sub(&start[1])];
    if !cross(&direction, &offset).is_zero() {
        return false;
    }
    let within = |value: &Scalar, a: &Scalar, b: &Scalar| {
        let low = a.cmp(value);
        let high = b.cmp(value);
        (low != std::cmp::Ordering::Greater) && (high != std::cmp::Ordering::Less)
    };
    within(&point[0], &start[0], &end[0]) && within(&point[1], &start[1], &end[1])
}

fn invoke_point_in_polygon(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let point = parse_vector(args, "point", 2, ctx)?;
    let vertices = parse_vertices(args, "vertices", ctx)?;
    let mut inside = false;
    for index in 0..vertices.len() {
        ctx.check()?;
        let current = &vertices[index];
        let next = &vertices[(index + 1) % vertices.len()];
        if point_on_segment(&point, current, next) {
            return Ok(Outcome::exact(Value::Bool(true)));
        }
        let crosses = (current[1].cmp(&point[1]) == std::cmp::Ordering::Greater)
            != (next[1].cmp(&point[1]) == std::cmp::Ordering::Greater);
        if crosses {
            let denominator = next[1].sub(&current[1]);
            let x_intersection = current[0].add(
                &point[1]
                    .sub(&current[1])
                    .mul(&next[0].sub(&current[0]))
                    .div(&denominator)?,
            );
            if point[0].cmp(&x_intersection) == std::cmp::Ordering::Less {
                inside = !inside;
            }
        }
    }
    Ok(Outcome::exact(Value::Bool(inside)))
}

fn invoke_point_line_distance(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let point = parse_vector(args, "point", 2, ctx)?;
    let start = parse_vector(args, "line_start", 2, ctx)?;
    let end = parse_vector(args, "line_end", 2, ctx)?;
    let direction = [end[0].sub(&start[0]), end[1].sub(&start[1])];
    if direction[0].is_zero() && direction[1].is_zero() {
        return Err(EngineError::domain(
            "point_line_distance_2d requires two distinct line points",
        ));
    }
    let offset = [point[0].sub(&start[0]), point[1].sub(&start[1])];
    let perpendicular = cross(&direction, &offset);
    let length_squared = direction[0]
        .mul(&direction[0])
        .add(&direction[1].mul(&direction[1]));
    let distance_squared = perpendicular.mul(&perpendicular).div(&length_squared)?;
    let (value, approximate) = sqrt_to_value(&distance_squared, ctx)?;
    Ok(Outcome::new(value, exactness(approximate)))
}

fn invoke_line_intersection(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a1 = parse_vector(args, "a1", 2, ctx)?;
    let a2 = parse_vector(args, "a2", 2, ctx)?;
    let b1 = parse_vector(args, "b1", 2, ctx)?;
    let b2 = parse_vector(args, "b2", 2, ctx)?;
    let approximate_inputs = any_float(&[
        a1[0].clone(),
        a1[1].clone(),
        a2[0].clone(),
        a2[1].clone(),
        b1[0].clone(),
        b1[1].clone(),
        b2[0].clone(),
        b2[1].clone(),
    ]);
    let direction_a = [a2[0].sub(&a1[0]), a2[1].sub(&a1[1])];
    let direction_b = [b2[0].sub(&b1[0]), b2[1].sub(&b1[1])];
    if (direction_a[0].is_zero() && direction_a[1].is_zero())
        || (direction_b[0].is_zero() && direction_b[1].is_zero())
    {
        return Err(EngineError::domain(
            "line_intersection_2d requires two distinct points per line",
        ));
    }
    let denominator = cross(&direction_a, &direction_b);
    let offset = [b1[0].sub(&a1[0]), b1[1].sub(&a1[1])];
    let parallel = denominator.is_zero();
    let coincident = parallel && cross(&direction_a, &offset).is_zero();
    let (intersection, approximate) = if parallel {
        (Value::Null, approximate_inputs)
    } else {
        let t = cross(&offset, &direction_b).div(&denominator)?;
        let x = a1[0].add(&t.mul(&direction_a[0]));
        let y = a1[1].add(&t.mul(&direction_a[1]));
        let approximate = x.is_float() || y.is_float();
        (
            Value::Array(vec![x.to_value(ctx)?, y.to_value(ctx)?]),
            approximate,
        )
    };
    let value = Value::record([
        ("intersection", intersection),
        ("parallel", Value::Bool(parallel)),
        ("coincident", Value::Bool(coincident)),
    ]);
    Ok(Outcome::new(value, exactness(approximate)))
}

pub(crate) fn register(functions: &mut Vec<Arc<dyn Function>>) {
    functions.push(SimpleFunction::arc(
        distance_2d_descriptor(),
        invoke_distance_2d,
    ));
    functions.push(SimpleFunction::arc(
        distance_3d_descriptor(),
        invoke_distance_3d,
    ));
    functions.push(SimpleFunction::arc(
        squared_distance_2d_descriptor(),
        invoke_squared_distance_2d,
    ));
    functions.push(SimpleFunction::arc(
        squared_distance_3d_descriptor(),
        invoke_squared_distance_3d,
    ));
    functions.push(SimpleFunction::arc(
        polygon_area_descriptor(),
        invoke_polygon_area,
    ));
    functions.push(SimpleFunction::arc(
        polygon_perimeter_descriptor(),
        invoke_polygon_perimeter,
    ));
    functions.push(SimpleFunction::arc(
        polygon_centroid_descriptor(),
        invoke_polygon_centroid,
    ));
    functions.push(SimpleFunction::arc(
        point_in_polygon_descriptor(),
        invoke_point_in_polygon,
    ));
    functions.push(SimpleFunction::arc(
        point_line_distance_descriptor(),
        invoke_point_line_distance,
    ));
    functions.push(SimpleFunction::arc(
        line_intersection_descriptor(),
        invoke_line_intersection,
    ));
}

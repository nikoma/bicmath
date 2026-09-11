//! 3D rotations represented as quaternions.
//!
//! Quaternions are `[w, x, y, z]` arrays with the scalar part first. Hamilton
//! products, conjugation, and squared norms are exact for exact rational
//! inputs; normalizing and the transcendental parts of Euler conversion use
//! binary64.
//!
//! Euler angles are given in radians. The default order `zyx` is intrinsic:
//! the rotations are applied about the body axes z (yaw), then y (pitch), then
//! x (roll). All six Tait-Bryan orders are supported; proper Euler orders with
//! a repeated axis are not supported in this release.

use std::sync::Arc;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::EngineError;
use bicmath_core::value::Value;

use crate::common::{
    all_modes, approximate_modes, descriptor, enum_schema, field, number_schema, record_schema,
    value_example, vector_schema,
};
use crate::f64math;
use crate::scalar::{AngleUnit, Scalar, any_float, ensure_finite, parse_vector, sqrt_to_value};

type Quat = [Scalar; 4];

fn parse_quat(args: &Args, name: &str, ctx: &ExecContext) -> Result<Quat, EngineError> {
    let values = parse_vector(args, name, 4, ctx)?;
    let mut out: Quat = [
        Scalar::zero(),
        Scalar::zero(),
        Scalar::zero(),
        Scalar::zero(),
    ];
    for (slot, value) in out.iter_mut().zip(values) {
        *slot = value;
    }
    Ok(out)
}

fn quat_mul(a: &Quat, b: &Quat) -> Quat {
    let w = a[0]
        .mul(&b[0])
        .sub(&a[1].mul(&b[1]))
        .sub(&a[2].mul(&b[2]))
        .sub(&a[3].mul(&b[3]));
    let x = a[0]
        .mul(&b[1])
        .add(&a[1].mul(&b[0]))
        .add(&a[2].mul(&b[3]))
        .sub(&a[3].mul(&b[2]));
    let y = a[0]
        .mul(&b[2])
        .sub(&a[1].mul(&b[3]))
        .add(&a[2].mul(&b[0]))
        .add(&a[3].mul(&b[1]));
    let z = a[0]
        .mul(&b[3])
        .add(&a[1].mul(&b[2]))
        .sub(&a[2].mul(&b[1]))
        .add(&a[3].mul(&b[0]));
    [w, x, y, z]
}

fn quat_conjugate(q: &Quat) -> Quat {
    [q[0].clone(), q[1].neg(), q[2].neg(), q[3].neg()]
}

fn quat_squared_norm(q: &Quat) -> Scalar {
    q.iter()
        .fold(Scalar::zero(), |total, value| total.add(&value.mul(value)))
}

fn quat_to_value(q: &Quat, ctx: &ExecContext) -> Result<Value, EngineError> {
    let mut items = Vec::with_capacity(4);
    for value in q {
        items.push(value.to_value(ctx)?);
    }
    Ok(Value::Array(items))
}

fn order_axes(order: &str) -> Result<[usize; 3], EngineError> {
    match order {
        "xyz" => Ok([0, 1, 2]),
        "xzy" => Ok([0, 2, 1]),
        "yxz" => Ok([1, 0, 2]),
        "yzx" => Ok([1, 2, 0]),
        "zxy" => Ok([2, 0, 1]),
        "zyx" => Ok([2, 1, 0]),
        other => Err(EngineError::domain(format!(
            "unknown Euler order {other:?}; expected one of xyz, xzy, yxz, yzx, zxy, zyx \
             (Tait-Bryan intrinsic orders)"
        ))),
    }
}

fn axis_quaternion(axis: usize, angle: &Scalar) -> Quat {
    let half = angle.mul(&Scalar::half());
    let sine = half.sin(AngleUnit::Radians);
    let cosine = half.cos(AngleUnit::Radians);
    let mut out: Quat = [
        Scalar::zero(),
        Scalar::zero(),
        Scalar::zero(),
        Scalar::zero(),
    ];
    out[0] = cosine;
    out[axis + 1] = sine;
    out
}

fn quaternion_multiply_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.quaternion_multiply",
        "Quaternion multiply",
        "Hamilton product of two quaternions.",
        "quaternion-multiply",
    )
    .with_description(
        "Quaternions are `[w, x, y, z]` with the scalar part first. The product is exact for \
         exact inputs. Float64 inputs require scientific mode.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("a", "Left quaternion `[w, x, y, z]`.", vector_schema(4)),
        ParamDescriptor::required("b", "Right quaternion `[w, x, y, z]`.", vector_schema(4)),
    ])
    .with_output(vector_schema(4), "Hamilton product a * b.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![value_example(
        "identity product",
        &[
            ("a", serde_json::json!([1, 0, 0, 0])),
            ("b", serde_json::json!([0, 1, 2, 3])),
        ],
        serde_json::json!([0, 1, 2, 3]),
    )])
}

fn quaternion_conjugate_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.quaternion_conjugate",
        "Quaternion conjugate",
        "Conjugate of a quaternion: `[w, -x, -y, -z]`.",
        "quaternion-conjugate",
    )
    .with_description("Exact for exact inputs; float64 inputs require scientific mode.")
    .with_parameters(vec![ParamDescriptor::required(
        "q",
        "Quaternion `[w, x, y, z]`.",
        vector_schema(4),
    )])
    .with_output(vector_schema(4), "Conjugate of q.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![value_example(
        "conjugate",
        &[("q", serde_json::json!([1, 2, 3, 4]))],
        serde_json::json!([1, -2, -3, -4]),
    )])
}

fn quaternion_norm_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.quaternion_norm",
        "Quaternion norm",
        "Euclidean norm of a quaternion.",
        "quaternion-norm",
    )
    .with_description(
        "The squared norm is computed exactly; the norm is exact when that square is a \
         perfect rational square, a decimal approximation in auto mode otherwise, and an \
         error in exact mode.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "q",
        "Quaternion `[w, x, y, z]`.",
        vector_schema(4),
    )])
    .with_output(number_schema(), "Non-negative norm.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![value_example(
        "3-4-5 norm",
        &[("q", serde_json::json!([3, 0, 0, 4]))],
        serde_json::json!(5),
    )])
}

fn quaternion_normalize_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.quaternion_normalize",
        "Quaternion normalize",
        "Unit quaternion in the same direction.",
        "quaternion-normalize",
    )
    .with_description(
        "Divides by the norm. Exact when the squared norm is a perfect rational square; \
         otherwise components are decimal approximations in auto mode and float64 in \
         scientific mode. The zero quaternion is a domain error.",
    )
    .with_parameters(vec![ParamDescriptor::required(
        "q",
        "Non-zero quaternion `[w, x, y, z]`.",
        vector_schema(4),
    )])
    .with_output(vector_schema(4), "Unit quaternion.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![value_example(
        "normalize 3-4-5",
        &[("q", serde_json::json!([3, 0, 0, 4]))],
        serde_json::json!([
            {"kind": "rational", "numerator": "3", "denominator": "5"},
            0,
            0,
            {"kind": "rational", "numerator": "4", "denominator": "5"}
        ]),
    )])
}

fn quaternion_rotate_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.quaternion_rotate",
        "Quaternion rotate",
        "Rotate a 3D vector by a quaternion.",
        "quaternion-rotate",
    )
    .with_description(
        "Computes `(q * (0, v) * conjugate(q)) / |q|^2`, which rotates `v` by the rotation \
         the quaternion represents and does not require `q` to be unit length. Exact for \
         exact inputs; the zero quaternion is a domain error.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("q", "Rotation quaternion `[w, x, y, z]`.", vector_schema(4)),
        ParamDescriptor::required("vector", "Vector to rotate `[x, y, z]`.", vector_schema(3)),
    ])
    .with_output(vector_schema(3), "Rotated vector.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![value_example(
        "identity rotation",
        &[
            ("q", serde_json::json!([1, 0, 0, 0])),
            ("vector", serde_json::json!([1, 2, 3])),
        ],
        serde_json::json!([1, 2, 3]),
    )])
}

fn euler_to_quaternion_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.euler_to_quaternion",
        "Euler to quaternion",
        "Convert intrinsic Euler angles to a quaternion.",
        "euler-to-quaternion",
    )
    .with_description(
        "Angles are in radians and name rotations about the x (roll), y (pitch), and \
         z (yaw) axes. `order` lists the axes in intrinsic application order and defaults \
         to `zyx` (yaw, then pitch, then roll). The six Tait-Bryan orders are supported. \
         The identity (all angles zero) is exact; other inputs are approximate because \
         sine and cosine are transcendental.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("roll", "Rotation about x, in radians.", number_schema()),
        ParamDescriptor::required("pitch", "Rotation about y, in radians.", number_schema()),
        ParamDescriptor::required("yaw", "Rotation about z, in radians.", number_schema()),
        ParamDescriptor::optional(
            "order",
            "Intrinsic axis order.",
            enum_schema(&["xyz", "xzy", "yxz", "yzx", "zxy", "zyx"]),
        ),
    ])
    .with_output(vector_schema(4), "Quaternion `[w, x, y, z]`.")
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![value_example(
        "identity",
        &[
            ("roll", serde_json::json!(0)),
            ("pitch", serde_json::json!(0)),
            ("yaw", serde_json::json!(0)),
        ],
        serde_json::json!([1, 0, 0, 0]),
    )])
}

fn quaternion_to_euler_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.quaternion_to_euler",
        "Quaternion to Euler",
        "Convert a quaternion to intrinsic Euler angles in radians.",
        "quaternion-to-euler",
    )
    .with_description(
        "The quaternion is normalized before extraction. Angles are returned in radians as \
         `roll` (x), `pitch` (y), and `yaw` (z). `order` lists the axes in intrinsic \
         application order and defaults to `zyx`. Gimbal-lock cases set the third angle to \
         zero. Always approximate; exact mode is rejected.",
    )
    .with_parameters(vec![
        ParamDescriptor::required("q", "Non-zero quaternion `[w, x, y, z]`.", vector_schema(4)),
        ParamDescriptor::optional(
            "order",
            "Intrinsic axis order.",
            enum_schema(&["xyz", "xzy", "yxz", "yzx", "zxy", "zyx"]),
        ),
    ])
    .with_output(
        record_schema(vec![
            field("roll", number_schema()),
            field("pitch", number_schema()),
            field("yaw", number_schema()),
        ]),
        "Intrinsic Euler angles in radians.",
    )
    .with_modes(approximate_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![
        Example::new(
            "identity quaternion",
            crate::common::example_args(&[("q", serde_json::json!([1, 0, 0, 0]))]),
        )
        .with_contains("roll"),
    ])
}

fn invoke_quaternion_multiply(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let a = parse_quat(args, "a", ctx)?;
    let b = parse_quat(args, "b", ctx)?;
    let product = quat_mul(&a, &b);
    let approximate = any_float(&a) || any_float(&b);
    Ok(Outcome::new(
        quat_to_value(&product, ctx)?,
        if approximate {
            Exactness::Approximate
        } else {
            Exactness::Exact
        },
    ))
}

fn invoke_quaternion_conjugate(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let q = parse_quat(args, "q", ctx)?;
    let approximate = any_float(&q);
    Ok(Outcome::new(
        quat_to_value(&quat_conjugate(&q), ctx)?,
        if approximate {
            Exactness::Approximate
        } else {
            Exactness::Exact
        },
    ))
}

fn invoke_quaternion_norm(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let q = parse_quat(args, "q", ctx)?;
    let squared = quat_squared_norm(&q);
    let (value, approximate) = sqrt_to_value(&squared, ctx)?;
    Ok(Outcome::new(
        value,
        if approximate {
            Exactness::Approximate
        } else {
            Exactness::Exact
        },
    ))
}

fn invoke_quaternion_normalize(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let q = parse_quat(args, "q", ctx)?;
    let squared = quat_squared_norm(&q);
    if squared.is_zero() {
        return Err(EngineError::domain(
            "the zero quaternion has no normalized form",
        ));
    }
    let norm = squared.sqrt()?;
    let mut normalized: Quat = [
        Scalar::zero(),
        Scalar::zero(),
        Scalar::zero(),
        Scalar::zero(),
    ];
    for (slot, value) in normalized.iter_mut().zip(q.iter()) {
        *slot = value.div(&norm)?;
    }
    let approximate = any_float(&normalized);
    Ok(Outcome::new(
        quat_to_value(&normalized, ctx)?,
        if approximate {
            Exactness::Approximate
        } else {
            Exactness::Exact
        },
    ))
}

fn invoke_quaternion_rotate(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let q = parse_quat(args, "q", ctx)?;
    let vector = parse_vector(args, "vector", 3, ctx)?;
    let squared = quat_squared_norm(&q);
    if squared.is_zero() {
        return Err(EngineError::domain("cannot rotate by the zero quaternion"));
    }
    let pure: Quat = [
        Scalar::zero(),
        vector[0].clone(),
        vector[1].clone(),
        vector[2].clone(),
    ];
    let rotated = quat_mul(&quat_mul(&q, &pure), &quat_conjugate(&q));
    let mut output = Vec::with_capacity(3);
    let mut approximate = squared.is_float();
    for value in rotated.iter().skip(1) {
        let component = value.div(&squared)?;
        approximate |= component.is_float();
        output.push(component);
    }
    let mut items = Vec::with_capacity(3);
    for value in &output {
        items.push(value.to_value(ctx)?);
    }
    Ok(Outcome::new(
        Value::Array(items),
        if approximate {
            Exactness::Approximate
        } else {
            Exactness::Exact
        },
    ))
}

fn invoke_euler_to_quaternion(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let roll = Scalar::from_value(args.require("roll")?, "roll", ctx)?;
    let pitch = Scalar::from_value(args.require("pitch")?, "pitch", ctx)?;
    let yaw = Scalar::from_value(args.require("yaw")?, "yaw", ctx)?;
    ensure_finite(&roll, "roll")?;
    ensure_finite(&pitch, "pitch")?;
    ensure_finite(&yaw, "yaw")?;
    let order = match args.optional_text("order")? {
        Some(text) => order_axes(text)?,
        None => order_axes("zyx")?,
    };
    let angles = [roll, pitch, yaw];
    let mut quaternion: Quat = [
        Scalar::one(),
        Scalar::zero(),
        Scalar::zero(),
        Scalar::zero(),
    ];
    for axis in order {
        let axis_rotation = axis_quaternion(axis, &angles[axis]);
        quaternion = quat_mul(&quaternion, &axis_rotation);
    }
    let approximate = any_float(&quaternion);
    Ok(Outcome::new(
        quat_to_value(&quaternion, ctx)?,
        if approximate {
            Exactness::Approximate
        } else {
            Exactness::Exact
        },
    ))
}

fn quat_mul_f64(a: &[f64; 4], b: &[f64; 4]) -> [f64; 4] {
    [
        a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
        a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2],
        a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1],
        a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0],
    ]
}

fn quat_conjugate_f64(q: &[f64; 4]) -> [f64; 4] {
    [q[0], -q[1], -q[2], -q[3]]
}

fn unit_quaternion(q: &Quat) -> Result<[f64; 4], EngineError> {
    let raw = [q[0].to_f64(), q[1].to_f64(), q[2].to_f64(), q[3].to_f64()];
    let squared: f64 = raw.iter().map(|value| value * value).sum();
    if !squared.is_finite() || squared <= 0.0 {
        return Err(EngineError::domain(
            "quaternion_to_euler requires a non-zero finite quaternion",
        ));
    }
    let inverse = 1.0 / f64math::sqrt(squared);
    Ok([
        raw[0] * inverse,
        raw[1] * inverse,
        raw[2] * inverse,
        raw[3] * inverse,
    ])
}

fn extract_euler(quaternion: &[f64; 4], order: [usize; 3]) -> [f64; 3] {
    let [w, x, y, z] = *quaternion;
    let matrix = [
        [
            1.0 - 2.0 * (y * y + z * z),
            2.0 * (x * y - w * z),
            2.0 * (x * z + w * y),
        ],
        [
            2.0 * (x * y + w * z),
            1.0 - 2.0 * (x * x + z * z),
            2.0 * (y * z - w * x),
        ],
        [
            2.0 * (x * z - w * y),
            2.0 * (y * z + w * x),
            1.0 - 2.0 * (x * x + y * y),
        ],
    ];
    let [i, j, k] = order;
    let even = matches!(order, [0, 1, 2] | [1, 2, 0] | [2, 0, 1]);
    let middle = if even { matrix[i][k] } else { -matrix[i][k] };
    let beta = f64math::asin(middle.clamp(-1.0, 1.0));
    let (alpha, gamma);
    if middle.abs() >= 1.0 - 1e-12 {
        gamma = 0.0;
        let axis_rotation = {
            let half = beta / 2.0;
            let mut axis: [f64; 4] = [f64math::cos(half), 0.0, 0.0, 0.0];
            axis[j + 1] = f64math::sin(half);
            axis
        };
        let residual = quat_mul_f64(quaternion, &quat_conjugate_f64(&axis_rotation));
        alpha = 2.0 * f64math::atan2(residual[j + 1], residual[0]);
    } else {
        match order {
            [0, 1, 2] => {
                alpha = f64math::atan2(-matrix[1][2], matrix[2][2]);
                gamma = f64math::atan2(-matrix[0][1], matrix[0][0]);
            }
            [0, 2, 1] => {
                alpha = f64math::atan2(matrix[2][1], matrix[1][1]);
                gamma = f64math::atan2(matrix[0][2], matrix[0][0]);
            }
            [1, 0, 2] => {
                alpha = f64math::atan2(matrix[0][2], matrix[2][2]);
                gamma = f64math::atan2(matrix[1][0], matrix[1][1]);
            }
            [1, 2, 0] => {
                alpha = f64math::atan2(-matrix[2][0], matrix[0][0]);
                gamma = f64math::atan2(-matrix[1][2], matrix[1][1]);
            }
            [2, 0, 1] => {
                alpha = f64math::atan2(-matrix[0][1], matrix[1][1]);
                gamma = f64math::atan2(-matrix[2][0], matrix[2][2]);
            }
            [2, 1, 0] => {
                alpha = f64math::atan2(matrix[1][0], matrix[0][0]);
                gamma = f64math::atan2(matrix[2][1], matrix[2][2]);
            }
            _ => unreachable!("order_axes only produces distinct-axis permutations"),
        }
    }
    let mut angles = [0.0f64; 3];
    angles[i] = alpha;
    angles[j] = beta;
    angles[k] = gamma;
    angles
}

fn invoke_quaternion_to_euler(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let q = parse_quat(args, "q", ctx)?;
    let order = match args.optional_text("order")? {
        Some(text) => order_axes(text)?,
        None => order_axes("zyx")?,
    };
    let unit = unit_quaternion(&q)?;
    let angles = extract_euler(&unit, order);
    let value = Value::record([
        ("roll", Scalar::Float(angles[0]).to_value(ctx)?),
        ("pitch", Scalar::Float(angles[1]).to_value(ctx)?),
        ("yaw", Scalar::Float(angles[2]).to_value(ctx)?),
    ]);
    Ok(Outcome::approximate(value))
}

pub(crate) fn register(functions: &mut Vec<Arc<dyn Function>>) {
    functions.push(SimpleFunction::arc(
        quaternion_multiply_descriptor(),
        invoke_quaternion_multiply,
    ));
    functions.push(SimpleFunction::arc(
        quaternion_conjugate_descriptor(),
        invoke_quaternion_conjugate,
    ));
    functions.push(SimpleFunction::arc(
        quaternion_norm_descriptor(),
        invoke_quaternion_norm,
    ));
    functions.push(SimpleFunction::arc(
        quaternion_normalize_descriptor(),
        invoke_quaternion_normalize,
    ));
    functions.push(SimpleFunction::arc(
        quaternion_rotate_descriptor(),
        invoke_quaternion_rotate,
    ));
    functions.push(SimpleFunction::arc(
        euler_to_quaternion_descriptor(),
        invoke_euler_to_quaternion,
    ));
    functions.push(SimpleFunction::arc(
        quaternion_to_euler_descriptor(),
        invoke_quaternion_to_euler,
    ));
}

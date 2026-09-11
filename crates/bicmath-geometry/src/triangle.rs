//! Triangle solving from any three of sides and angles.
//!
//! Supported configurations are SSS, SAS, ASA/AAS, and SSA. An ambiguous SSA
//! configuration (two distinct valid triangles) is rejected with a domain error
//! rather than silently choosing one solution.

use std::sync::Arc;

use num_bigint::BigInt;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{
    Args, CostClass, Example, Function, FunctionDescriptor, Outcome, ParamDescriptor,
    SimpleFunction,
};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::schema::ValueSchema;
use bicmath_core::value::Value;

use crate::common::{
    all_modes, descriptor, enum_schema, field, number_schema, optional_field, record_schema,
};
use crate::scalar::{AngleUnit, Scalar, any_float, clamp_non_negative, ensure_finite};

const ANGLE_EPSILON: f64 = 1e-9;

#[derive(Default)]
struct Givens {
    sides: [Option<Scalar>; 3],
    angles: [Option<Scalar>; 3],
}

impl Givens {
    fn count(&self) -> usize {
        self.sides.iter().filter(|value| value.is_some()).count()
            + self.angles.iter().filter(|value| value.is_some()).count()
    }
}

struct Solution {
    sides: [Scalar; 3],
    angles: [Scalar; 3],
    area: Scalar,
    perimeter: Scalar,
    method: &'static str,
}

impl Solution {
    fn approximate(&self) -> bool {
        any_float(&self.sides)
            || any_float(&self.angles)
            || self.area.is_float()
            || self.perimeter.is_float()
    }
}

fn parse_givens(args: &Args, ctx: &ExecContext) -> Result<(Givens, AngleUnit), EngineError> {
    let record = args.record("givens")?;
    let mut givens = Givens::default();
    for (key, value) in record {
        let path = format!("givens.{key}");
        match key.as_str() {
            "a" => givens.sides[0] = Some(Scalar::from_value(value, &path, ctx)?),
            "b" => givens.sides[1] = Some(Scalar::from_value(value, &path, ctx)?),
            "c" => givens.sides[2] = Some(Scalar::from_value(value, &path, ctx)?),
            "angle_a" => givens.angles[0] = Some(Scalar::from_value(value, &path, ctx)?),
            "angle_b" => givens.angles[1] = Some(Scalar::from_value(value, &path, ctx)?),
            "angle_c" => givens.angles[2] = Some(Scalar::from_value(value, &path, ctx)?),
            other => {
                return Err(EngineError::malformed(format!(
                    "unknown triangle field {other:?}; expected a, b, c, angle_a, angle_b, or angle_c"
                ))
                .with_path(path));
            }
        }
    }
    if givens.count() != 3 {
        return Err(EngineError::malformed(format!(
            "triangle_solve requires exactly three of a, b, c, angle_a, angle_b, angle_c; found {}",
            givens.count()
        ))
        .with_path("givens"));
    }
    let unit = match args.optional_text("angle_unit")? {
        Some(text) => AngleUnit::parse(text)?,
        None => AngleUnit::Degrees,
    };
    Ok((givens, unit))
}

fn validate_sides(givens: &Givens) -> Result<(), EngineError> {
    let names = ["a", "b", "c"];
    for (index, side) in givens.sides.iter().enumerate() {
        if let Some(side) = side {
            ensure_finite(side, &format!("side {}", names[index]))?;
            if !side.is_zero() && side.is_negative() {
                return Err(EngineError::domain(format!(
                    "side {} must be positive",
                    names[index]
                )));
            }
            if side.is_zero() {
                return Err(EngineError::domain(format!(
                    "side {} must be positive",
                    names[index]
                )));
            }
        }
    }
    Ok(())
}

fn validate_angles(givens: &Givens, unit: AngleUnit) -> Result<(), EngineError> {
    let names = ["angle_a", "angle_b", "angle_c"];
    let straight = unit.straight();
    for (index, angle) in givens.angles.iter().enumerate() {
        if let Some(angle) = angle {
            ensure_finite(angle, names[index])?;
            let value = angle.to_f64();
            if !(value > 0.0 && value < straight) {
                return Err(EngineError::domain(format!(
                    "{} must lie strictly between 0 and {}",
                    names[index], straight
                )));
            }
        }
    }
    Ok(())
}

fn law_cos_angle(x: &Scalar, y: &Scalar, opposite: &Scalar, unit: AngleUnit) -> Scalar {
    let numerator = x.mul(x).add(&y.mul(y)).sub(&opposite.mul(opposite));
    let denominator = Scalar::exact_i64(2).mul(x).mul(y);
    match numerator.div(&denominator) {
        // Binary64 roundoff can push a valid triangle's cosine just outside
        // [-1, 1]; clamp only the approximate value so `acos` stays real.
        Ok(Scalar::Float(cosine)) => Scalar::Float(cosine.clamp(-1.0, 1.0)).acos(unit),
        Ok(exact) => exact.acos(unit),
        Err(_) => Scalar::Float(f64::NAN),
    }
}

fn sas_side(
    x: &Scalar,
    y: &Scalar,
    included: &Scalar,
    unit: AngleUnit,
) -> Result<Scalar, EngineError> {
    let cosine = included.cos(unit);
    let squared = x
        .mul(x)
        .add(&y.mul(y))
        .sub(&Scalar::exact_i64(2).mul(x).mul(y).mul(&cosine));
    clamp_non_negative(squared).sqrt()
}

fn heron_area(a: &Scalar, b: &Scalar, c: &Scalar) -> Result<Scalar, EngineError> {
    let semiperimeter = a.add(b).add(c).div(&Scalar::exact_i64(2))?;
    let product = semiperimeter
        .mul(&semiperimeter.sub(a))
        .mul(&semiperimeter.sub(b))
        .mul(&semiperimeter.sub(c));
    clamp_non_negative(product).sqrt()
}

fn check_triangle_inequality(a: &Scalar, b: &Scalar, c: &Scalar) -> Result<(), EngineError> {
    let strict = a.add(b).cmp(c) == std::cmp::Ordering::Greater
        && a.add(c).cmp(b) == std::cmp::Ordering::Greater
        && b.add(c).cmp(a) == std::cmp::Ordering::Greater;
    if strict {
        Ok(())
    } else {
        Err(EngineError::domain(
            "the three sides do not satisfy the triangle inequality: each side must be \
             positive and strictly less than the sum of the other two",
        ))
    }
}

fn solve_sss(a: &Scalar, b: &Scalar, c: &Scalar, unit: AngleUnit) -> Result<Solution, EngineError> {
    check_triangle_inequality(a, b, c)?;
    let area = heron_area(a, b, c)?;
    Ok(Solution {
        sides: [a.clone(), b.clone(), c.clone()],
        angles: [
            law_cos_angle(b, c, a, unit),
            law_cos_angle(a, c, b, unit),
            law_cos_angle(a, b, c, unit),
        ],
        area,
        perimeter: a.add(b).add(c),
        method: "SSS",
    })
}

fn solve_sas(
    side_names: (usize, usize),
    included_name: usize,
    x: &Scalar,
    y: &Scalar,
    included: &Scalar,
    unit: AngleUnit,
) -> Result<Solution, EngineError> {
    let (first, second) = side_names;
    let missing = 3 - first - second;
    let opposite = sas_side(x, y, included, unit)?;
    let mut sides: [Scalar; 3] = [Scalar::zero(), Scalar::zero(), Scalar::zero()];
    sides[first] = x.clone();
    sides[second] = y.clone();
    sides[missing] = opposite;
    let mut angles: [Scalar; 3] = [Scalar::zero(), Scalar::zero(), Scalar::zero()];
    angles[included_name] = included.clone();
    for index in 0..3 {
        if index == included_name {
            continue;
        }
        let others: Vec<usize> = (0..3).filter(|value| *value != index).collect();
        angles[index] = law_cos_angle(&sides[others[0]], &sides[others[1]], &sides[index], unit);
    }
    let area = Scalar::half().mul(x).mul(y).mul(&included.sin(unit));
    let perimeter = x.add(y).add(&sides[missing]);
    Ok(Solution {
        sides,
        angles,
        area,
        perimeter,
        method: "SAS",
    })
}

fn supplement(angle: &Scalar, unit: AngleUnit) -> Scalar {
    match angle {
        Scalar::Exact(value) if unit == AngleUnit::Degrees => {
            Scalar::exact_i64(180).sub(&Scalar::Exact(value.clone()))
        }
        _ => Scalar::Float(unit.straight() - angle.to_f64()),
    }
}

fn solve_angles_and_side(
    side_index: usize,
    side: &Scalar,
    givens: &Givens,
    unit: AngleUnit,
) -> Result<Solution, EngineError> {
    let given_angles: Vec<usize> = (0..3)
        .filter(|index| givens.angles[*index].is_some())
        .collect();
    let missing = 3 - given_angles[0] - given_angles[1];
    let mut angles: [Scalar; 3] = [Scalar::zero(), Scalar::zero(), Scalar::zero()];
    for index in given_angles.iter().copied() {
        angles[index] = givens.angles[index]
            .clone()
            .ok_or_else(|| EngineError::internal("angle was counted as given"))?;
    }
    let sum = angles[given_angles[0]].add(&angles[given_angles[1]]);
    let third = supplement(&sum, unit);
    if third.to_f64() <= 0.0 {
        return Err(EngineError::domain(
            "the two given angles sum to a straight angle or more; no triangle exists",
        ));
    }
    angles[missing] = third;
    let sine_opposite = angles[side_index].sin(unit);
    if sine_opposite.is_zero() {
        return Err(EngineError::domain(
            "the angle opposite the given side must be non-zero",
        ));
    }
    let scale = side.div(&sine_opposite)?;
    let mut sides: [Scalar; 3] = [Scalar::zero(), Scalar::zero(), Scalar::zero()];
    for index in 0..3 {
        if index == side_index {
            sides[index] = side.clone();
        } else {
            sides[index] = scale.mul(&angles[index].sin(unit));
        }
    }
    let area = Scalar::half()
        .mul(&sides[0])
        .mul(&sides[1])
        .mul(&angles[2].sin(unit));
    let perimeter = sides[0].add(&sides[1]).add(&sides[2]);
    let method = if givens.angles[side_index].is_some() {
        "AAS"
    } else {
        "ASA"
    };
    Ok(Solution {
        sides,
        angles,
        area,
        perimeter,
        method,
    })
}

fn solve_ssa(givens: &Givens, unit: AngleUnit) -> Result<Solution, EngineError> {
    let side_indices: Vec<usize> = (0..3)
        .filter(|index| givens.sides[*index].is_some())
        .collect();
    let angle_index = (0..3)
        .find(|index| givens.angles[*index].is_some())
        .ok_or_else(|| EngineError::internal("SSA requires one given angle"))?;
    let other_index = if side_indices[0] == angle_index {
        side_indices[1]
    } else {
        side_indices[0]
    };
    let opposite_side = givens.sides[angle_index]
        .clone()
        .ok_or_else(|| EngineError::internal("SSA angle has no matching side"))?;
    let adjacent_side = givens.sides[other_index]
        .clone()
        .ok_or_else(|| EngineError::internal("SSA requires two given sides"))?;
    let given_angle = givens.angles[angle_index]
        .clone()
        .ok_or_else(|| EngineError::internal("SSA requires a given angle"))?;
    let sine_other = adjacent_side
        .mul(&given_angle.sin(unit))
        .div(&opposite_side)?;
    let sine_value = sine_other.to_f64();
    if !(0.0..=1.0 + ANGLE_EPSILON).contains(&sine_value) {
        return Err(EngineError::domain(
            "no triangle exists for the given two sides and non-included angle",
        ));
    }
    let clamped = if sine_value > 1.0 {
        Scalar::one()
    } else {
        sine_other
    };
    let primary = clamped.asin(unit);
    let alternate = supplement(&primary, unit);
    let straight = unit.straight();
    let primary_valid = given_angle.to_f64() + primary.to_f64() < straight - ANGLE_EPSILON;
    let alternate_valid = !alternate.equals(&primary)
        && given_angle.to_f64() + alternate.to_f64() < straight - ANGLE_EPSILON;
    let solution_angle = match (primary_valid, alternate_valid) {
        (true, true) => {
            return Err(EngineError::domain(
                "the SSA configuration is ambiguous: two distinct triangles satisfy the \
                 given two sides and non-included angle; provide an additional constraint",
            ));
        }
        (false, false) => {
            return Err(EngineError::domain(
                "no triangle exists for the given two sides and non-included angle",
            ));
        }
        (true, false) => primary,
        (false, true) => alternate,
    };
    let missing = 3 - angle_index - other_index;
    let remaining = supplement(&given_angle.add(&solution_angle), unit);
    if remaining.to_f64() <= 0.0 {
        return Err(EngineError::domain(
            "no triangle exists for the given two sides and non-included angle",
        ));
    }
    let mut sides: [Scalar; 3] = [Scalar::zero(), Scalar::zero(), Scalar::zero()];
    sides[angle_index] = opposite_side.clone();
    sides[other_index] = adjacent_side.clone();
    sides[missing] = opposite_side
        .mul(&remaining.sin(unit))
        .div(&given_angle.sin(unit))?;
    let mut angles: [Scalar; 3] = [Scalar::zero(), Scalar::zero(), Scalar::zero()];
    angles[angle_index] = given_angle;
    angles[other_index] = solution_angle;
    angles[missing] = remaining;
    let area = Scalar::half()
        .mul(&sides[angle_index])
        .mul(&sides[other_index])
        .mul(&angles[missing].sin(unit));
    let perimeter = sides[0].add(&sides[1]).add(&sides[2]);
    Ok(Solution {
        sides,
        angles,
        area,
        perimeter,
        method: "SSA",
    })
}

fn solve(givens: &Givens, unit: AngleUnit) -> Result<Solution, EngineError> {
    validate_sides(givens)?;
    validate_angles(givens, unit)?;
    let side_count = givens.sides.iter().filter(|value| value.is_some()).count();
    let angle_count = givens.angles.iter().filter(|value| value.is_some()).count();
    match (side_count, angle_count) {
        (3, 0) => {
            let a = givens.sides[0]
                .clone()
                .ok_or_else(|| EngineError::internal("side a"))?;
            let b = givens.sides[1]
                .clone()
                .ok_or_else(|| EngineError::internal("side b"))?;
            let c = givens.sides[2]
                .clone()
                .ok_or_else(|| EngineError::internal("side c"))?;
            solve_sss(&a, &b, &c, unit)
        }
        (2, 1) => {
            let side_indices: Vec<usize> = (0..3)
                .filter(|index| givens.sides[*index].is_some())
                .collect();
            let angle_index = (0..3)
                .find(|index| givens.angles[*index].is_some())
                .ok_or_else(|| EngineError::internal("one angle"))?;
            if side_indices.contains(&angle_index) {
                solve_ssa(givens, unit)
            } else {
                let x = givens.sides[side_indices[0]]
                    .clone()
                    .ok_or_else(|| EngineError::internal("first side"))?;
                let y = givens.sides[side_indices[1]]
                    .clone()
                    .ok_or_else(|| EngineError::internal("second side"))?;
                let included = givens.angles[angle_index]
                    .clone()
                    .ok_or_else(|| EngineError::internal("included angle"))?;
                solve_sas(
                    (side_indices[0], side_indices[1]),
                    angle_index,
                    &x,
                    &y,
                    &included,
                    unit,
                )
            }
        }
        (1, 2) => {
            let side_index = (0..3)
                .find(|index| givens.sides[*index].is_some())
                .ok_or_else(|| EngineError::internal("one side"))?;
            let side = givens.sides[side_index]
                .clone()
                .ok_or_else(|| EngineError::internal("side"))?;
            solve_angles_and_side(side_index, &side, givens, unit)
        }
        (0, 3) => Err(EngineError::domain(
            "three angles determine a triangle's shape but not its size; provide at least one side",
        )),
        _ => Err(EngineError::malformed(
            "triangle_solve could not classify the given combination",
        )),
    }
}

fn solution_value(solution: &Solution, ctx: &ExecContext) -> Result<Value, EngineError> {
    let sides = Value::record([
        ("a", solution.sides[0].to_value(ctx)?),
        ("b", solution.sides[1].to_value(ctx)?),
        ("c", solution.sides[2].to_value(ctx)?),
    ]);
    let angles = Value::record([
        ("angle_a", solution.angles[0].to_value(ctx)?),
        ("angle_b", solution.angles[1].to_value(ctx)?),
        ("angle_c", solution.angles[2].to_value(ctx)?),
    ]);
    Ok(Value::record([
        ("sides", sides),
        ("angles", angles),
        ("area", solution.area.to_value(ctx)?),
        ("perimeter", solution.perimeter.to_value(ctx)?),
        ("valid", Value::Bool(true)),
        ("method", Value::text(solution.method)),
        (
            "solutions_count",
            Value::Number(bicmath_core::number::Number::Integer(BigInt::from(1u32))),
        ),
    ]))
}

fn triangle_solve_descriptor() -> FunctionDescriptor {
    descriptor(
        "geometry.triangle_solve",
        "Solve a triangle",
        "Solve a triangle from any three of its sides and angles.",
        "triangle-solve",
    )
    .with_description(
        "Supply exactly three of a, b, c, angle_a, angle_b, angle_c inside `givens`. \
         Supported configurations are SSS, SAS, ASA/AAS, and SSA. Angles are in `angle_unit` \
         (default degrees) and must lie strictly between 0 and 180 degrees (0 and pi radians). \
         The triangle inequality is validated. Heron's formula supplies the area. An \
         ambiguous SSA configuration — two distinct valid triangles — is rejected with a \
         domain error instead of silently picking one; `solutions_count` is therefore always \
         1 and `valid` always true for a successful result, while invalid inputs return an \
         error rather than `valid: false`. Exact inputs yield exact sides, area, and \
         perimeter where the algebra allows; angles that require inverse trigonometry are \
         decimal approximations in auto mode, float64 in scientific mode, and an error in \
         exact mode.",
    )
    .with_parameters(vec![
        ParamDescriptor::required(
            "givens",
            "Record with exactly three of a, b, c, angle_a, angle_b, angle_c.",
            record_schema(vec![
                optional_field("a", number_schema()),
                optional_field("b", number_schema()),
                optional_field("c", number_schema()),
                optional_field("angle_a", number_schema()),
                optional_field("angle_b", number_schema()),
                optional_field("angle_c", number_schema()),
            ]),
        ),
        ParamDescriptor::optional(
            "angle_unit",
            "Angle unit for the given and returned angles.",
            enum_schema(&["degrees", "radians"]),
        ),
    ])
    .with_output(
        record_schema(vec![
            field(
                "sides",
                record_schema(vec![
                    field("a", number_schema()),
                    field("b", number_schema()),
                    field("c", number_schema()),
                ]),
            ),
            field(
                "angles",
                record_schema(vec![
                    field("angle_a", number_schema()),
                    field("angle_b", number_schema()),
                    field("angle_c", number_schema()),
                ]),
            ),
            field("area", number_schema()),
            field("perimeter", number_schema()),
            field("valid", ValueSchema::Bool),
            field("method", ValueSchema::text()),
            field(
                "solutions_count",
                ValueSchema::number(bicmath_core::schema::NumberKind::Integer),
            ),
        ]),
        "Solved sides, angles, area, perimeter, method, and solution count.",
    )
    .with_modes(all_modes())
    .with_cost(CostClass::Constant)
    .with_examples(vec![
        Example::new(
            "SSS 3-4-5",
            crate::common::example_args(&[("givens", serde_json::json!({"a": 3, "b": 4, "c": 5}))]),
        )
        .with_contains("SSS"),
        Example::new(
            "invalid triangle",
            crate::common::example_args(&[(
                "givens",
                serde_json::json!({"a": 1, "b": 2, "c": 10}),
            )]),
        )
        .with_error(ErrorCode::DomainViolation),
    ])
}

fn invoke_triangle_solve(args: &Args, ctx: &ExecContext) -> Result<Outcome, EngineError> {
    ctx.check()?;
    let (givens, unit) = parse_givens(args, ctx)?;
    let solution = solve(&givens, unit)?;
    ctx.check()?;
    let value = solution_value(&solution, ctx)?;
    let approximate = solution.approximate();
    Ok(Outcome::new(
        value,
        if approximate {
            Exactness::Approximate
        } else {
            Exactness::Exact
        },
    ))
}

pub(crate) fn register(functions: &mut Vec<Arc<dyn Function>>) {
    functions.push(SimpleFunction::arc(
        triangle_solve_descriptor(),
        invoke_triangle_solve,
    ));
}

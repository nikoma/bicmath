//! Behavioural tests for the geometry module.

use std::collections::BTreeMap;

use bicmath_core::context::ExecContext;
use bicmath_core::contract::{Args, ExampleExpectation, Outcome};
use bicmath_core::envelope::Exactness;
use bicmath_core::error::{EngineError, ErrorCode};
use bicmath_core::number::{Number, NumericMode};
use bicmath_core::value::Value;

fn call_with(id: &str, raw: serde_json::Value, mode: NumericMode) -> Result<Outcome, EngineError> {
    let module = crate::module();
    let function = module
        .functions
        .iter()
        .find(|function| function.descriptor().id == id)
        .unwrap_or_else(|| panic!("function {id} exists"));
    let mut ctx = ExecContext::conservative();
    ctx.numeric.mode = mode;
    let mut values = BTreeMap::new();
    for (name, value) in raw.as_object().expect("object arguments") {
        let param = function
            .descriptor()
            .parameter(name)
            .unwrap_or_else(|| panic!("parameter {name} of {id} exists"));
        values.insert(
            name.clone(),
            param
                .schema
                .coerce(value, name, &ctx.limits, true)
                .unwrap_or_else(|error| panic!("argument {name} coerces: {error}")),
        );
    }
    function.invoke(&Args::new(values), &ctx)
}

fn call(id: &str, raw: serde_json::Value) -> Result<Outcome, EngineError> {
    call_with(id, raw, NumericMode::Auto)
}

fn number(value: &Value) -> &Number {
    value.as_number().expect("number")
}

fn as_f64(value: &Value) -> f64 {
    number(value).to_f64().expect("float64 conversion")
}

fn field<'a>(value: &'a Value, name: &str) -> &'a Value {
    value
        .as_record()
        .expect("record")
        .get(name)
        .unwrap_or_else(|| panic!("field {name} exists"))
}

fn text(value: &Value) -> &str {
    value.as_text().expect("text")
}

// ---------------------------------------------------------------------------
// Distances
// ---------------------------------------------------------------------------

#[test]
fn distance_2d_is_exact_for_a_pythagorean_triple() {
    let outcome = call(
        "geometry.distance_2d",
        serde_json::json!({"p": [0, 0], "q": [3, 4]}),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Exact);
    assert_eq!(number(&outcome.value).to_string(), "5");
}

#[test]
fn distance_3d_is_exact_for_a_pythagorean_quadruple() {
    let outcome = call(
        "geometry.distance_3d",
        serde_json::json!({"p": [0, 0, 0], "q": [2, 3, 6]}),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Exact);
    assert_eq!(number(&outcome.value).to_string(), "7");
}

#[test]
fn squared_distance_is_exact() {
    let outcome = call(
        "geometry.squared_distance_3d",
        serde_json::json!({"p": [1, 2, 3], "q": [4, 6, 15]}),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Exact);
    assert_eq!(number(&outcome.value).to_string(), "169");
    let distance = call(
        "geometry.distance_3d",
        serde_json::json!({"p": [1, 2, 3], "q": [4, 6, 15]}),
    )
    .unwrap();
    assert_eq!(number(&distance.value).to_string(), "13");
}

#[test]
fn distance_approximates_irrational_roots_and_exact_mode_errors() {
    let outcome = call(
        "geometry.distance_2d",
        serde_json::json!({"p": [0, 0], "q": [1, 1]}),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Approximate);
    assert!((as_f64(&outcome.value) - std::f64::consts::SQRT_2).abs() < 1e-15);
    let error = call_with(
        "geometry.distance_2d",
        serde_json::json!({"p": [0, 0], "q": [1, 1]}),
        NumericMode::Exact,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);
}

#[test]
fn float64_inputs_require_scientific_mode() {
    let error = call(
        "geometry.distance_2d",
        serde_json::json!({
            "p": [{"kind": "float64", "value": "1.5"}, 0],
            "q": [0, 0]
        }),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);
    let outcome = call_with(
        "geometry.distance_2d",
        serde_json::json!({
            "p": [{"kind": "float64", "value": "1.5"}, 0],
            "q": [0, 0]
        }),
        NumericMode::Scientific,
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Approximate);
    assert!(matches!(outcome.value, Value::Number(Number::Float64(_))));
}

// ---------------------------------------------------------------------------
// Triangle solving
// ---------------------------------------------------------------------------

#[test]
fn triangle_solve_right_angle_between_legs_gives_exact_hypotenuse() {
    let outcome = call(
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"a": 3, "b": 4, "angle_c": 90}}),
    )
    .unwrap();
    assert_eq!(text(field(&outcome.value, "method")), "SAS");
    let sides = field(&outcome.value, "sides");
    assert_eq!(number(field(sides, "a")).to_string(), "3");
    assert_eq!(number(field(sides, "b")).to_string(), "4");
    assert_eq!(number(field(sides, "c")).to_string(), "5");
    assert_eq!(number(field(&outcome.value, "area")).to_string(), "6");
    assert_eq!(number(field(&outcome.value, "perimeter")).to_string(), "12");
    let angles = field(&outcome.value, "angles");
    assert_eq!(number(field(angles, "angle_c")).to_string(), "90");
}

#[test]
fn triangle_solve_sss_3_4_5_has_exact_area() {
    let outcome = call(
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"a": 3, "b": 4, "c": 5}}),
    )
    .unwrap();
    assert_eq!(text(field(&outcome.value, "method")), "SSS");
    assert_eq!(number(field(&outcome.value, "area")).to_string(), "6");
    assert_eq!(
        number(field(field(&outcome.value, "angles"), "angle_c")).to_string(),
        "90"
    );
}

#[test]
fn triangle_solve_equilateral_area_is_sqrt_three() {
    let outcome = call(
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"a": 2, "b": 2, "c": 2}}),
    )
    .unwrap();
    let area = as_f64(field(&outcome.value, "area"));
    assert!((area - 3.0f64.sqrt()).abs() < 1e-9, "area was {area}");
    assert!((area - 1.732_050_8).abs() < 1e-6);
    let angles = field(&outcome.value, "angles");
    assert_eq!(number(field(angles, "angle_a")).to_string(), "60");
    assert_eq!(number(field(angles, "angle_b")).to_string(), "60");
    assert_eq!(number(field(angles, "angle_c")).to_string(), "60");
}

#[test]
fn triangle_solve_rejects_an_impossible_triangle() {
    let error = call(
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"a": 1, "b": 2, "c": 10}}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::DomainViolation);
    assert!(error.message.contains("triangle inequality"));
}

#[test]
fn triangle_solve_detects_ambiguous_ssa() {
    let error = call(
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"a": 10, "b": 12, "angle_a": 30}}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::DomainViolation);
    assert!(
        error.message.contains("ambiguous"),
        "message: {}",
        error.message
    );
}

#[test]
fn triangle_solve_single_solution_ssa() {
    let outcome = call(
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"a": 7, "b": 5, "angle_a": 30}}),
    )
    .unwrap();
    assert_eq!(text(field(&outcome.value, "method")), "SSA");
    assert_eq!(
        number(field(field(&outcome.value, "sides"), "a")).to_string(),
        "7"
    );
}

#[test]
fn triangle_solve_asa_and_aas() {
    let asa = call(
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"c": 2, "angle_a": 30, "angle_b": 60}}),
    )
    .unwrap();
    assert_eq!(text(field(&asa.value, "method")), "ASA");
    assert_eq!(
        number(field(field(&asa.value, "sides"), "a")).to_string(),
        "1"
    );

    let aas = call(
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"a": 2, "angle_a": 90, "angle_b": 30}}),
    )
    .unwrap();
    assert_eq!(text(field(&aas.value, "method")), "AAS");
    assert_eq!(
        number(field(field(&aas.value, "sides"), "b")).to_string(),
        "1"
    );
}

#[test]
fn triangle_solve_supports_radians() {
    let outcome = call(
        "geometry.triangle_solve",
        serde_json::json!({
            "givens": {"a": 3, "b": 4, "angle_c": "1.5707963267948966"},
            "angle_unit": "radians"
        }),
    )
    .unwrap();
    let c = as_f64(field(field(&outcome.value, "sides"), "c"));
    assert!((c - 5.0).abs() < 1e-9, "hypotenuse was {c}");
}

#[test]
fn triangle_solve_requires_exactly_three_givens() {
    let error = call(
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"a": 3, "b": 4}}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::MalformedInput);
}

#[test]
fn triangle_solve_rejects_three_angles() {
    let error = call(
        "geometry.triangle_solve",
        serde_json::json!({"givens": {"angle_a": 30, "angle_b": 60, "angle_c": 90}}),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::DomainViolation);
}

// ---------------------------------------------------------------------------
// Polygons and lines
// ---------------------------------------------------------------------------

#[test]
fn polygon_area_is_exact_and_signed() {
    let counter_clockwise = call(
        "geometry.polygon_area",
        serde_json::json!({
            "vertices": [[-0.5, -0.5], [1.5, -0.5], [1.5, 1.5], [-0.5, 1.5]]
        }),
    )
    .unwrap();
    assert_eq!(counter_clockwise.exactness, Exactness::Exact);
    assert_eq!(number(&counter_clockwise.value).to_string(), "4");
    let clockwise = call(
        "geometry.polygon_area",
        serde_json::json!({"vertices": [[0, 0], [0, 1], [1, 1], [1, 0]]}),
    )
    .unwrap();
    assert_eq!(number(&clockwise.value).to_string(), "-1");
}

#[test]
fn polygon_centroid_is_exact() {
    let outcome = call(
        "geometry.polygon_centroid",
        serde_json::json!({
            "vertices": [[-0.5, -0.5], [1.5, -0.5], [1.5, 1.5], [-0.5, 1.5]]
        }),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Exact);
    assert!((as_f64(field(&outcome.value, "x")) - 0.5).abs() < 1e-15);
    assert!((as_f64(field(&outcome.value, "y")) - 0.5).abs() < 1e-15);
}

#[test]
fn polygon_perimeter_sums_exact_edges() {
    let outcome = call(
        "geometry.polygon_perimeter",
        serde_json::json!({"vertices": [[0, 0], [3, 0], [0, 4]]}),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Exact);
    assert_eq!(number(&outcome.value).to_string(), "12");
}

#[test]
fn point_in_polygon_handles_inside_outside_and_boundary() {
    let vertices = serde_json::json!([[0, 0], [2, 0], [2, 2], [0, 2]]);
    let inside = call(
        "geometry.point_in_polygon",
        serde_json::json!({"point": [1, 1], "vertices": vertices}),
    )
    .unwrap();
    assert_eq!(inside.value, Value::Bool(true));
    let outside = call(
        "geometry.point_in_polygon",
        serde_json::json!({"point": [3, 3], "vertices": vertices}),
    )
    .unwrap();
    assert_eq!(outside.value, Value::Bool(false));
    let boundary = call(
        "geometry.point_in_polygon",
        serde_json::json!({"point": [0, 1], "vertices": vertices}),
    )
    .unwrap();
    assert_eq!(boundary.value, Value::Bool(true));
}

#[test]
fn point_line_distance_is_exact_for_a_perpendicular_foot() {
    let outcome = call(
        "geometry.point_line_distance_2d",
        serde_json::json!({
            "point": [0, 5],
            "line_start": [0, 0],
            "line_end": [10, 0]
        }),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Exact);
    assert_eq!(number(&outcome.value).to_string(), "5");
    let diagonal = call(
        "geometry.point_line_distance_2d",
        serde_json::json!({
            "point": [1, 0],
            "line_start": [0, 0],
            "line_end": [1, 1]
        }),
    )
    .unwrap();
    assert!((as_f64(&diagonal.value) - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-15);
}

#[test]
fn line_intersection_is_exact_and_flags_parallel_lines() {
    let crossing = call(
        "geometry.line_intersection_2d",
        serde_json::json!({
            "a1": [0, 0], "a2": [2, 2],
            "b1": [0, 2], "b2": [2, 0]
        }),
    )
    .unwrap();
    assert_eq!(crossing.exactness, Exactness::Exact);
    assert_eq!(field(&crossing.value, "parallel"), &Value::Bool(false));
    assert_eq!(field(&crossing.value, "coincident"), &Value::Bool(false));
    let intersection = field(&crossing.value, "intersection").as_array().unwrap();
    assert_eq!(number(&intersection[0]).to_string(), "1");
    assert_eq!(number(&intersection[1]).to_string(), "1");

    let parallel = call(
        "geometry.line_intersection_2d",
        serde_json::json!({
            "a1": [0, 0], "a2": [2, 0],
            "b1": [0, 1], "b2": [2, 1]
        }),
    )
    .unwrap();
    assert_eq!(field(&parallel.value, "parallel"), &Value::Bool(true));
    assert_eq!(field(&parallel.value, "coincident"), &Value::Bool(false));
    assert_eq!(field(&parallel.value, "intersection"), &Value::Null);

    let coincident = call(
        "geometry.line_intersection_2d",
        serde_json::json!({
            "a1": [0, 0], "a2": [2, 0],
            "b1": [1, 0], "b2": [3, 0]
        }),
    )
    .unwrap();
    assert_eq!(field(&coincident.value, "parallel"), &Value::Bool(true));
    assert_eq!(field(&coincident.value, "coincident"), &Value::Bool(true));
}

// ---------------------------------------------------------------------------
// Geodesy
// ---------------------------------------------------------------------------

fn london() -> serde_json::Value {
    serde_json::json!([-0.1276, 51.5074])
}

fn paris() -> serde_json::Value {
    serde_json::json!([2.3522, 48.8566])
}

#[test]
fn haversine_london_paris_is_within_one_percent() {
    let outcome = call(
        "geometry.haversine",
        serde_json::json!({"point_a": london(), "point_b": paris()}),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Approximate);
    assert_eq!(text(field(&outcome.value, "method")), "haversine_sphere");
    assert_eq!(field(&outcome.value, "approximate"), &Value::Bool(true));
    assert_eq!(
        number(field(&outcome.value, "radius_km")).to_string(),
        "6371.0088"
    );
    let distance = as_f64(field(&outcome.value, "distance_km"));
    assert!(
        (distance - 343.5).abs() / 343.5 < 0.01,
        "distance was {distance}"
    );
}

#[test]
fn haversine_accepts_a_custom_radius() {
    let outcome = call(
        "geometry.haversine",
        serde_json::json!({"point_a": [0, 0], "point_b": [90, 0], "radius": 1}),
    )
    .unwrap();
    let distance = as_f64(field(&outcome.value, "distance_km"));
    assert!((distance - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    assert_eq!(number(field(&outcome.value, "radius_km")).to_string(), "1");
}

#[test]
fn bearing_and_destination_round_trip_to_paris() {
    let bearing = call(
        "geometry.initial_bearing",
        serde_json::json!({"a": london(), "b": paris()}),
    )
    .unwrap();
    let bearing_degrees = as_f64(field(&bearing.value, "bearing_degrees"));
    assert!((0.0..360.0).contains(&bearing_degrees));

    let distance = call(
        "geometry.haversine",
        serde_json::json!({"point_a": london(), "point_b": paris()}),
    )
    .unwrap();
    let distance_km = as_f64(field(&distance.value, "distance_km"));

    let destination = call(
        "geometry.destination_point",
        serde_json::json!({
            "origin": london(),
            "bearing_degrees": bearing_degrees,
            "distance_km": distance_km
        }),
    )
    .unwrap();
    let point = field(&destination.value, "point").as_array().unwrap();
    let longitude = as_f64(&point[0]);
    let latitude = as_f64(&point[1]);
    assert!((longitude - 2.3522).abs() < 1e-6, "longitude {longitude}");
    assert!((latitude - 48.8566).abs() < 1e-6, "latitude {latitude}");
}

#[test]
fn geodesy_rejects_exact_mode_and_out_of_range_coordinates() {
    let error = call_with(
        "geometry.haversine",
        serde_json::json!({"point_a": london(), "point_b": paris()}),
        NumericMode::Exact,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsupportedNumericMode);

    let range = call(
        "geometry.haversine",
        serde_json::json!({"point_a": [200, 0], "point_b": paris()}),
    )
    .unwrap_err();
    assert_eq!(range.code, ErrorCode::DomainViolation);

    let negative = call(
        "geometry.destination_point",
        serde_json::json!({
            "origin": london(),
            "bearing_degrees": 0,
            "distance_km": -1
        }),
    )
    .unwrap_err();
    assert_eq!(negative.code, ErrorCode::DomainViolation);
}

// ---------------------------------------------------------------------------
// Quaternions
// ---------------------------------------------------------------------------

fn array(value: &Value) -> Vec<f64> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(as_f64)
        .collect()
}

#[test]
fn quaternion_multiply_by_identity_is_exact() {
    let outcome = call(
        "geometry.quaternion_multiply",
        serde_json::json!({"a": [1, 0, 0, 0], "b": [0, 1, 2, 3]}),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Exact);
    assert_eq!(array(&outcome.value), vec![0.0, 1.0, 2.0, 3.0]);
}

#[test]
fn quaternion_conjugate_and_norm_are_exact() {
    let conjugate = call(
        "geometry.quaternion_conjugate",
        serde_json::json!({"q": [1, 2, 3, 4]}),
    )
    .unwrap();
    assert_eq!(array(&conjugate.value), vec![1.0, -2.0, -3.0, -4.0]);
    let norm = call(
        "geometry.quaternion_norm",
        serde_json::json!({"q": [3, 0, 0, 4]}),
    )
    .unwrap();
    assert_eq!(norm.exactness, Exactness::Exact);
    assert_eq!(number(&norm.value).to_string(), "5");
}

#[test]
fn quaternion_normalize_is_exact_for_a_pythagorean_norm() {
    let outcome = call(
        "geometry.quaternion_normalize",
        serde_json::json!({"q": [3, 0, 0, 4]}),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Exact);
    assert_eq!(
        array(&outcome.value),
        vec![0.6, 0.0, 0.0, 0.8],
        "components were {:?}",
        outcome.value
    );
    let zero = call(
        "geometry.quaternion_normalize",
        serde_json::json!({"q": [0, 0, 0, 0]}),
    )
    .unwrap_err();
    assert_eq!(zero.code, ErrorCode::DomainViolation);
}

#[test]
fn quaternion_rotate_90_degrees_about_z_maps_x_to_y() {
    let s = "0.7071067811865476";
    let outcome = call(
        "geometry.quaternion_rotate",
        serde_json::json!({
            "q": [s, 0, 0, s],
            "vector": [1, 0, 0]
        }),
    )
    .unwrap();
    let rotated = array(&outcome.value);
    let expected = [0.0, 1.0, 0.0];
    for (index, value) in rotated.iter().enumerate() {
        assert!(
            (value - expected[index]).abs() < 1e-12,
            "component {index} was {value}"
        );
    }
}

#[test]
fn quaternion_rotate_identity_is_exact() {
    let outcome = call(
        "geometry.quaternion_rotate",
        serde_json::json!({"q": [1, 0, 0, 0], "vector": [1, 2, 3]}),
    )
    .unwrap();
    assert_eq!(outcome.exactness, Exactness::Exact);
    assert_eq!(array(&outcome.value), vec![1.0, 2.0, 3.0]);
}

#[test]
fn euler_to_quaternion_and_back_round_trips_for_all_orders() {
    let orders = ["xyz", "xzy", "yxz", "yzx", "zxy", "zyx"];
    let (roll, pitch, yaw) = (0.3, -0.4, 0.5);
    for order in orders {
        let forward = call(
            "geometry.euler_to_quaternion",
            serde_json::json!({
                "roll": roll,
                "pitch": pitch,
                "yaw": yaw,
                "order": order
            }),
        )
        .unwrap();
        let quaternion = serde_json::to_value(&forward.value).unwrap();
        let backward = call(
            "geometry.quaternion_to_euler",
            serde_json::json!({"q": quaternion, "order": order}),
        )
        .unwrap();
        let recovered = [
            as_f64(field(&backward.value, "roll")),
            as_f64(field(&backward.value, "pitch")),
            as_f64(field(&backward.value, "yaw")),
        ];
        let expected = [roll, pitch, yaw];
        for axis in 0..3 {
            assert!(
                (recovered[axis] - expected[axis]).abs() < 1e-9,
                "order {order} axis {axis}: {} vs {}",
                recovered[axis],
                expected[axis]
            );
        }
    }
}

#[test]
fn quaternion_to_euler_handles_gimbal_lock() {
    let forward = call(
        "geometry.euler_to_quaternion",
        serde_json::json!({
            "roll": 0.2,
            "pitch": std::f64::consts::FRAC_PI_2,
            "yaw": 0.3
        }),
    )
    .unwrap();
    let quaternion = serde_json::to_value(&forward.value).unwrap();
    let backward = call(
        "geometry.quaternion_to_euler",
        serde_json::json!({"q": quaternion}),
    )
    .unwrap();
    let pitch = as_f64(field(&backward.value, "pitch"));
    assert!(
        (pitch - std::f64::consts::FRAC_PI_2).abs() < 1e-9,
        "pitch was {pitch}"
    );
    assert!(as_f64(field(&backward.value, "roll")).is_finite());
    assert!(as_f64(field(&backward.value, "yaw")).is_finite());
}

// ---------------------------------------------------------------------------
// Solids
// ---------------------------------------------------------------------------

#[test]
fn circle_and_sphere_values_and_error_estimates() {
    let circle = call("geometry.circle", serde_json::json!({"radius": 2})).unwrap();
    assert_eq!(circle.exactness, Exactness::Approximate);
    assert!(circle.error_estimate.is_some());
    let area = as_f64(field(&circle.value, "area"));
    let circumference = as_f64(field(&circle.value, "circumference"));
    assert!((area - 4.0 * std::f64::consts::PI).abs() < 1e-12);
    assert!((circumference - 4.0 * std::f64::consts::PI).abs() < 1e-12);

    let sphere = call("geometry.sphere", serde_json::json!({"radius": 3})).unwrap();
    assert!(sphere.error_estimate.is_some());
    let volume = as_f64(field(&sphere.value, "volume"));
    let surface = as_f64(field(&sphere.value, "surface_area"));
    assert!((volume - 36.0 * std::f64::consts::PI).abs() < 1e-12);
    assert!((surface - 36.0 * std::f64::consts::PI).abs() < 1e-12);

    let negative = call("geometry.circle", serde_json::json!({"radius": -1})).unwrap_err();
    assert_eq!(negative.code, ErrorCode::DomainViolation);
    let exact = call_with(
        "geometry.circle",
        serde_json::json!({"radius": 1}),
        NumericMode::Exact,
    )
    .unwrap_err();
    assert_eq!(exact.code, ErrorCode::UnsupportedNumericMode);
}

// ---------------------------------------------------------------------------
// Contract and documentation
// ---------------------------------------------------------------------------

#[test]
fn module_metadata_and_ids_are_consistent() {
    let module = crate::module();
    assert_eq!(module.descriptor.id, "geometry");
    assert_eq!(module.descriptor.version, "1.0.0");
    assert!(!module.functions.is_empty());
    for function in &module.functions {
        let descriptor = function.descriptor();
        assert!(
            descriptor.id.starts_with("geometry."),
            "unexpected id {}",
            descriptor.id
        );
        assert_eq!(descriptor.module, "geometry");
        assert_eq!(descriptor.version, "1.0.0");
        assert!(
            !descriptor.examples.is_empty(),
            "{} has no examples",
            descriptor.id
        );
    }
}

#[test]
fn every_declared_example_executes_with_its_expectation() {
    for function in crate::module().functions {
        let descriptor = function.descriptor();
        for example in &descriptor.examples {
            let raw = serde_json::to_value(&example.arguments).expect("example serializes");
            let is_error_example = matches!(example.expected, Some(ExampleExpectation::Error(_)));
            let modes: Vec<NumericMode> = if is_error_example {
                descriptor.modes.first().copied().into_iter().collect()
            } else {
                descriptor.modes.clone()
            };
            let mut outcome = None;
            let mut last_error = None;
            for mode in &modes {
                match call_with(&descriptor.id, raw.clone(), *mode) {
                    Ok(value) => {
                        outcome = Some(value);
                        break;
                    }
                    Err(error) => last_error = Some(error),
                }
            }
            match (outcome, &example.expected) {
                (Some(outcome), Some(ExampleExpectation::Value(expected))) => {
                    assert_eq!(
                        &outcome.value, expected,
                        "{} example {:?}",
                        descriptor.id, example.title
                    );
                }
                (Some(outcome), Some(ExampleExpectation::Contains(needle))) => {
                    let rendered = serde_json::to_string(&outcome.value).unwrap_or_default();
                    assert!(
                        rendered.contains(needle),
                        "{} example {:?}: {rendered} does not contain {needle:?}",
                        descriptor.id,
                        example.title
                    );
                }
                (Some(_), Some(ExampleExpectation::Error(expected))) => {
                    panic!(
                        "{} example {:?}: expected error {expected:?}, call succeeded",
                        descriptor.id, example.title
                    );
                }
                (Some(_), None) => {}
                (None, Some(ExampleExpectation::Error(expected))) => {
                    let error = last_error.expect("an error was recorded");
                    assert_eq!(
                        error.code, *expected,
                        "{} example {:?}: {}",
                        descriptor.id, example.title, error.message
                    );
                }
                (None, _) => {
                    panic!(
                        "{} example {:?} failed: {:?}",
                        descriptor.id, example.title, last_error
                    );
                }
            }
        }
    }
}

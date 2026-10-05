//! Golden coordinate cases for ADR-0002.
//!
//! The fixture is `tests/fixtures/coordinate-contract.v1.json` at the
//! repository root. It is the human-reviewable statement of the coordinate
//! contract: the ops, the mapped points, and the exported SVG matrix, all in
//! one place, independent of the Rust type that implements them.
//!
//! A case's `ops` list runs from the outermost node to the innermost local
//! transform, so `ops[0]` is the ancestor. This mirrors a document tree: a leaf
//! point is mapped by its own transform and then by every ancestor's.
//!
//! When a case is changed, the change is a decision about the coordinate
//! contract, not a test detail. Update the fixture deliberately and say why.

use std::f64::consts::PI;
use std::path::PathBuf;

use serde_json::Value;
use swotvibe_core::{GeometryError, Transform};

const FIXTURE: &str = "coordinate-contract.v1.json";

fn fixture_path() -> PathBuf {
    // `CARGO_MANIFEST_DIR` is `crates/core`; the fixture lives at the
    // repository root so the format crate and tools can read the same file.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(FIXTURE)
}

fn load() -> Value {
    let path = fixture_path();
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not valid JSON: {error}", path.display()))
}

fn number(value: &Value, context: &str) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| panic!("{context} should be a number, found {value}"))
}

fn pair(value: &Value, context: &str) -> (f64, f64) {
    let items = value
        .as_array()
        .unwrap_or_else(|| panic!("{context} should be an array, found {value}"));
    assert_eq!(items.len(), 2, "{context} should hold two numbers");
    (number(&items[0], context), number(&items[1], context))
}

/// Builds one op. Unknown ops panic so a typo cannot pass unnoticed.
fn op_to_transform(op: &Value) -> Transform {
    let items = op
        .as_array()
        .unwrap_or_else(|| panic!("an op should be an array, found {op}"));
    let name = items[0]
        .as_str()
        .unwrap_or_else(|| panic!("an op name should be a string, found {}", items[0]));
    let args: Vec<f64> = items[1..]
        .iter()
        .map(|value| number(value, "op argument"))
        .collect();
    match (name, args.as_slice()) {
        ("translate", [x, y]) => Transform::translate(*x, *y),
        ("scale", [x, y]) => Transform::scale(*x, *y),
        // The fixture states rotation in degrees because a reader can check it
        // against the screen; the model stores radians.
        ("rotate", [degrees]) => Transform::rotate(degrees * PI / 180.0),
        ("matrix", [a, b, c, d, e, f]) => Transform::new([*a, *b, *c, *d, *e, *f]),
        _ => panic!("unknown op `{name}` with {} arguments", args.len()),
    }
    .unwrap_or_else(|error| panic!("op `{name}` is not a legal transform: {error}"))
}

/// Composes an op list from the outermost entry to the innermost.
fn compose(ops: &Value) -> Transform {
    let items = ops
        .as_array()
        .unwrap_or_else(|| panic!("`ops` should be an array, found {ops}"));
    items
        .iter()
        .map(op_to_transform)
        .reduce(|outer, inner| {
            outer
                .compose(&inner)
                .expect("a composition of fixture transforms should stay finite")
        })
        .unwrap_or(Transform::IDENTITY)
}

fn case_names(fixture: &Value) -> Vec<String> {
    fixture["cases"]
        .as_array()
        .expect("`cases` should be an array")
        .iter()
        .map(|case| case["name"].as_str().expect("a case needs a name").into())
        .collect()
}

#[test]
fn the_fixture_declares_every_required_case() {
    let names = case_names(&load());
    for required in [
        "identity",
        "translate",
        "scale",
        "rotate_quarter_turn_is_clockwise_on_screen",
        "nested_translate_rotate_scale",
        "nested_four_levels",
        "nested_exact_matrices",
        "mirrored_scale_flips_orientation",
        "degenerate_scale_has_no_inverse",
    ] {
        assert!(
            names.iter().any(|name| name == required),
            "the fixture no longer covers `{required}`"
        );
    }
}

#[test]
fn every_case_maps_its_points_as_the_fixture_states() {
    let fixture = load();
    let tolerance = number(&fixture["tolerance"], "tolerance");
    for case in fixture["cases"].as_array().expect("cases") {
        let name = case["name"].as_str().expect("a case needs a name");
        let transform = compose(&case["ops"]);
        for point in case["points"].as_array().expect("points") {
            let (x, y) = pair(&point["in"], "point.in");
            let (expected_x, expected_y) = pair(&point["out"], "point.out");
            let (actual_x, actual_y) = transform.apply(x, y);
            assert!(
                (actual_x - expected_x).abs() < tolerance
                    && (actual_y - expected_y).abs() < tolerance,
                "{name}: ({x}, {y}) mapped to ({actual_x}, {actual_y}), expected ({expected_x}, {expected_y})"
            );
        }
    }
}

#[test]
fn every_case_that_states_coefficients_matches_them() {
    let fixture = load();
    let tolerance = number(&fixture["tolerance"], "tolerance");
    for case in fixture["cases"].as_array().expect("cases") {
        let Some(stated) = case.get("coefficients").and_then(Value::as_array) else {
            continue;
        };
        let name = case["name"].as_str().expect("a case needs a name");
        assert_eq!(stated.len(), 6, "{name}: a transform has six coefficients");
        let actual = compose(&case["ops"]).coefficients();
        for (index, entry) in stated.iter().enumerate() {
            let expected = number(entry, "coefficient");
            assert!(
                (actual[index] - expected).abs() < tolerance,
                "{name}: coefficient {index} is {}, expected {expected}",
                actual[index]
            );
        }
    }
}

/// Parses the numbers out of an SVG `matrix(...)` value.
///
/// The comparison is numeric rather than textual so a rotated transform — whose
/// coefficients are rounded like `6.123233995736766e-17` — is compared with the
/// same tolerance as everything else. A case that also sets `svgExact` asserts
/// the literal text, which is what pins the export format itself.
fn svg_numbers(svg: &str) -> Vec<f64> {
    let inner = svg
        .strip_prefix("matrix(")
        .and_then(|rest| rest.strip_suffix(')'))
        .unwrap_or_else(|| panic!("`{svg}` is not an SVG matrix value"));
    inner
        .split_whitespace()
        .map(|part| {
            part.parse::<f64>()
                .unwrap_or_else(|_| panic!("`{part}` in `{svg}` is not a number"))
        })
        .collect()
}

#[test]
fn every_case_that_states_an_svg_matrix_matches_the_export() {
    let fixture = load();
    let tolerance = number(&fixture["tolerance"], "tolerance");
    for case in fixture["cases"].as_array().expect("cases") {
        let Some(stated) = case.get("svg").and_then(Value::as_str) else {
            continue;
        };
        let name = case["name"].as_str().expect("a case needs a name");
        let transform = compose(&case["ops"]);
        let exported = transform.to_svg_matrix();

        let expected = svg_numbers(stated);
        let actual = svg_numbers(&exported);
        assert_eq!(expected.len(), 6, "{name}: an SVG matrix has six numbers");
        for (index, value) in expected.iter().enumerate() {
            assert!(
                (actual[index] - value).abs() < tolerance,
                "{name}: SVG coefficient {index} is {}, expected {value}",
                actual[index]
            );
        }

        if case["svgExact"] == Value::Bool(true) {
            assert_eq!(exported, stated, "{name}: the export text changed");
        }
    }
}

#[test]
fn a_nested_case_is_equivalent_to_its_regrouped_op_list() {
    let fixture = load();
    let tolerance = number(&fixture["tolerance"], "tolerance");
    for case in fixture["cases"].as_array().expect("cases") {
        let Some(alternatives) = case.get("equivalentOps").and_then(Value::as_array) else {
            continue;
        };
        let name = case["name"].as_str().expect("a case needs a name");
        let primary = compose(&case["ops"]);
        for alternative in alternatives {
            let other = compose(alternative);
            for (x, y) in [(0.0, 0.0), (1.0, 1.0), (-3.0, 2.5), (100.0, -40.0)] {
                let expected = primary.apply(x, y);
                let actual = other.apply(x, y);
                assert!(
                    (actual.0 - expected.0).abs() < tolerance
                        && (actual.1 - expected.1).abs() < tolerance,
                    "{name}: regrouped composition maps ({x}, {y}) differently"
                );
            }
        }
    }
}

#[test]
fn every_invertible_case_round_trips_and_every_singular_one_is_refused() {
    let fixture = load();
    let mut checked_invertible = 0;
    let mut checked_singular = 0;

    for case in fixture["cases"].as_array().expect("cases") {
        let name = case["name"].as_str().expect("a case needs a name");
        let transform = compose(&case["ops"]);
        let invertible = case["invertible"] != Value::Bool(false);
        if !invertible {
            checked_singular += 1;
            assert_eq!(
                transform.invert(),
                Err(GeometryError::Singular),
                "{name}: a degenerate transform must not invert"
            );
            continue;
        }
        checked_invertible += 1;
        let inverse = transform
            .invert()
            .unwrap_or_else(|error| panic!("{name}: expected an inverse, found {error}"));
        let points: Vec<(f64, f64)> = match case.get("invertiblePoints") {
            Some(values) => values
                .as_array()
                .expect("invertiblePoints")
                .iter()
                .map(|value| pair(value, "invertiblePoints"))
                .collect(),
            None => vec![(0.0, 0.0), (3.0, -4.0), (100.0, 250.0)],
        };
        for (x, y) in points {
            let mapped = transform.apply(x, y);
            let back = inverse.apply(mapped.0, mapped.1);
            assert!(
                (back.0 - x).abs() < 1e-6 && (back.1 - y).abs() < 1e-6,
                "{name}: ({x}, {y}) did not round-trip, returned ({}, {})",
                back.0,
                back.1
            );
        }
    }

    assert!(checked_invertible >= 8);
    assert!(checked_singular >= 1);
}

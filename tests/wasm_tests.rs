//! WASM binding tests
//!
//! Tests for WebAssembly bindings. Run with:
//! ```bash
//! wasm-pack test --node -- --features wasm
//! ```

#![cfg(all(feature = "wasm", target_arch = "wasm32"))]

use js_sys::{Object, Reflect};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

// Import the WASM functions
use ries_rs::{init, list_presets, version, wasm_search as search};

fn search_options(level: u32, max_matches: u32) -> JsValue {
    let options = Object::new();
    Reflect::set(
        &options,
        &JsValue::from_str("level"),
        &JsValue::from_f64(level.into()),
    )
    .expect("level should be set");
    Reflect::set(
        &options,
        &JsValue::from_str("maxMatches"),
        &JsValue::from_f64(max_matches.into()),
    )
    .expect("maxMatches should be set");
    options.into()
}

#[wasm_bindgen_test]
fn test_wasm_init() {
    // Test that init doesn't panic
    init();
}

#[wasm_bindgen_test]
fn test_wasm_version() {
    // Test that version returns a non-empty string
    let v = version();
    assert!(!v.is_empty());
    assert!(v.contains('.'));
}

#[wasm_bindgen_test]
fn test_wasm_search_rejects_non_finite_target() {
    for bad_target in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let result = search(bad_target, None);
        assert!(
            result.is_err(),
            "search should reject non-finite target {bad_target}"
        );
    }
}

#[wasm_bindgen_test]
fn test_wasm_basic_search() {
    // Test that basic search works through WASM bindings
    let result = search(2.0, None).expect("search should succeed");
    assert!(!result.is_empty(), "search should return results for 2.0");
}

#[wasm_bindgen_test]
fn test_wasm_search_with_options() {
    // Match the public JS API by passing a plain options object.
    let limited = search(2.0, Some(search_options(3, 1)))
        .expect("search with limited options should succeed");
    let expanded = search(2.0, Some(search_options(3, 5)))
        .expect("search with expanded options should succeed");

    assert_eq!(limited.len(), 1, "maxMatches=1 should cap the result count");
    assert!(
        expanded.len() > limited.len(),
        "raising maxMatches should allow more results"
    );
}

fn website_options() -> JsValue {
    let options = Object::new();
    Reflect::set(
        &options,
        &JsValue::from_str("level"),
        &JsValue::from_f64(3.0),
    )
    .unwrap();
    Reflect::set(
        &options,
        &JsValue::from_str("maxMatches"),
        &JsValue::from_f64(50.0),
    )
    .unwrap();
    Reflect::set(
        &options,
        &JsValue::from_str("rankingMode"),
        &JsValue::from_str("parity"),
    )
    .unwrap();
    options.into()
}

#[wasm_bindgen_test]
fn test_wasm_website_parity_page_leads_with_plain_forms() {
    // The browser calls search(target, {level: 3, maxMatches: 50, rankingMode: 'parity'}).
    let options = website_options();
    let two_and_a_half = search(2.5, Some(options.clone())).expect("2.5");
    assert!(two_and_a_half.len() <= 50);
    let first = &two_and_a_half[0];
    assert!(first.is_exact);
    assert!(first.lhs_postfix == "2x*" || first.lhs_postfix == "x2*");
    assert_eq!(first.rhs_postfix, "5");
    assert!(two_and_a_half
        .iter()
        .any(|m| m.is_exact && m.lhs_postfix == "x" && m.rhs_postfix == "52/"));

    let pi = search(std::f64::consts::PI, Some(options.clone())).expect("pi");
    assert!(pi[0].is_exact);
    assert_eq!(pi[0].lhs_postfix, "x");
    assert_eq!(pi[0].rhs_postfix, "p");

    let zero = search(0.0, Some(options)).expect("0");
    assert!(zero[0].is_exact);
    assert_eq!(zero[0].lhs, "x");
    assert_eq!(zero[0].rhs, "0");
}

#[wasm_bindgen_test]
fn test_wasm_search_result_properties() {
    // Test that search results have expected properties
    let result = search(1.618033988, None).expect("search should succeed");
    assert!(
        !result.is_empty(),
        "search should return results for golden ratio"
    );

    let first = &result[0];
    // Check that result has valid properties
    assert!(!first.lhs.is_empty(), "lhs should not be empty");
    assert!(!first.rhs.is_empty(), "rhs should not be empty");
    assert!(first.complexity > 0, "complexity should be positive");
}

#[wasm_bindgen_test]
fn test_wasm_match_to_string() {
    // Test WasmMatch to_string method
    let result = search(2.0, None).expect("search should succeed");
    let first = &result[0];
    let s = first.to_string();
    assert!(s.contains("="), "to_string should contain '='");
    assert!(s.contains("error"), "to_string should contain 'error'");
}

#[wasm_bindgen_test]
fn test_wasm_match_to_json() {
    // Test WasmMatch serialization
    let result = search(2.0, None).expect("search should succeed");
    let first = &result[0];
    let json = first.to_json().expect("to_json should succeed");
    assert!(!json.is_null(), "to_json should return non-null value");
}

#[wasm_bindgen_test]
fn test_wasm_list_presets() {
    // Test list_presets function
    let presets = list_presets().expect("list_presets should succeed");
    assert!(!presets.is_undefined(), "presets should be defined");
}

#[wasm_bindgen_test]
fn test_wasm_search_exact_value() {
    // Test search for exact mathematical constants
    let result = search(std::f64::consts::PI, None).expect("search should succeed");
    assert!(!result.is_empty(), "search should return results for pi");

    // Check for exact matches (pi is commonly found)
    let has_small_error = result.iter().any(|m| m.error.abs() < 1e-10);
    assert!(has_small_error, "should find very close matches for pi");
}

#[wasm_bindgen_test]
fn test_target_text_is_validated() {
    let options = search_options(0, 1);
    for text in ["1garbage", "0x10", "NaN", "2"] {
        Reflect::set(
            &options,
            &JsValue::from_str("targetText"),
            &JsValue::from_str(text),
        )
        .unwrap();
        assert!(
            search(1.0, Some(options.clone())).is_err(),
            "accepted {text}"
        );
    }
}

#[wasm_bindgen_test]
fn test_preset_constants_are_named() {
    let options = search_options(0, 16);
    Reflect::set(
        &options,
        &JsValue::from_str("preset"),
        &JsValue::from_str("analytic-nt"),
    )
    .unwrap();
    let results = search(1.64493406685, Some(options)).unwrap();
    assert!(results
        .iter()
        .any(|m| m.lhs.contains("z2") || m.rhs.contains("z2")));
    assert!(results
        .iter()
        .all(|m| !m.lhs.contains("u0") && !m.rhs.contains("u0")));
}

#[wasm_bindgen_test]
fn test_decimal_precision_and_underflow_are_checked() {
    let options = search_options(0, 1);
    Reflect::set(
        &options,
        &JsValue::from_str("matchAllDigits"),
        &JsValue::TRUE,
    )
    .unwrap();
    for text in [
        "1.000000000000000000000000000001",
        "-1.000000000000000000000000000001",
        "1e-400",
    ] {
        Reflect::set(
            &options,
            &JsValue::from_str("targetText"),
            &JsValue::from_str(text),
        )
        .unwrap();
        assert!(
            search(text.parse().unwrap(), Some(options.clone())).is_err(),
            "{text}"
        );
    }
    Reflect::set(
        &options,
        &JsValue::from_str("targetText"),
        &JsValue::from_str("1.000000000000000000000000000000"),
    )
    .unwrap();
    assert!(search(1.0, Some(options)).is_ok());
}

//! WASM binding tests
//!
//! Tests for WebAssembly bindings. Run with:
//! ```bash
//! wasm-pack test --node
//! ```

#![cfg(all(feature = "wasm", target_arch = "wasm32"))]

use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

// Import the WASM functions
use ries_rs::{init, list_presets, version, wasm_search as search, SearchOptions, WasmMatch};

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
fn test_wasm_basic_search() {
    // Test that basic search works through WASM bindings
    let result = search(2.0, None).expect("search should succeed");
    assert!(!result.is_empty(), "search should return results for 2.0");
}

#[wasm_bindgen_test]
fn test_wasm_search_with_options() {
    // Test search with custom options
    let options = SearchOptions::new().level(1).max_matches(5);

    // Convert options to JsValue
    let options_js = options.to_json().expect("options should serialize");

    let result = search(3.14159, Some(options_js)).expect("search with options should succeed");
    assert!(!result.is_empty(), "search should return results for pi");
    assert!(result.len() <= 5, "should respect max_matches limit");
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

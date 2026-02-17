//! Integration tests for full search functionality

mod common;

use ries_rs::gen::{generate_all, GenConfig};
use ries_rs::search::{search, search_with_stats};

fn default_config() -> GenConfig {
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 20; // Reduced for faster tests
    config.max_rhs_complexity = 22;
    config
}

#[test]
fn test_search_finds_matches() {
    let matches = search(2.5, &default_config(), 5);
    assert!(!matches.is_empty());
}

#[test]
fn test_search_finds_2x_equals_5() {
    let matches = search(2.5, &default_config(), 20);

    // Should find 2x = 5
    let has_2x = matches
        .iter()
        .any(|m| m.lhs.expr.to_postfix() == "2x*" && m.rhs.expr.to_postfix() == "5");
    assert!(has_2x, "Should find 2x = 5");
}

#[test]
fn test_search_exact_matches() {
    let matches = search(2.5, &default_config(), 20);

    // Count exact matches
    let exact: Vec<_> = matches.iter().filter(|m| m.error.abs() < 1e-14).collect();

    assert!(!exact.is_empty(), "Should have at least one exact match");

    // Verify 2x = 5 is exact
    let two_x_exact = exact.iter().any(|m| m.lhs.expr.to_postfix() == "2x*");
    assert!(two_x_exact, "2x = 5 should be an exact match");
}

#[test]
fn test_search_complexity_ordering() {
    let matches = search(2.5, &default_config(), 20);

    // Matches should be sorted by complexity
    let complexities: Vec<u32> = matches.iter().map(|m| m.complexity).collect();
    let mut sorted = complexities.clone();
    sorted.sort();
    assert_eq!(complexities, sorted);
}

#[test]
fn test_expression_generation_contains_expected() {
    let generated = generate_all(&default_config(), 2.5);

    // Should contain 2x* (2*x)
    let has_2x = generated.lhs.iter().any(|e| e.expr.to_postfix() == "2x*");
    assert!(has_2x, "Should generate 2x*");

    // Should contain 5
    let has_5 = generated.rhs.iter().any(|e| e.expr.to_postfix() == "5");
    assert!(has_5, "Should generate 5");
}

#[test]
fn test_search_with_stats() {
    let (matches, stats) = search_with_stats(2.5, &default_config(), 20);

    assert!(!matches.is_empty());
    assert!(stats.lhs_count > 0);
    assert!(stats.rhs_count > 0);
    assert!(stats.search_time.as_nanos() > 0);
}

#[test]
fn test_pi_search() {
    let matches = search(std::f64::consts::PI, &default_config(), 20);

    // Should find x = pi exactly
    let has_pi = matches
        .iter()
        .any(|m| m.error.abs() < 1e-14 && m.rhs.expr.to_postfix() == "p");
    assert!(has_pi, "Should find x = pi");
}

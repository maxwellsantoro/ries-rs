//! Test fixtures for RIES-RS tests
//!
//! These utilities are kept for future test development.

use ries_rs::gen::GenConfig;
use ries_rs::search::SearchConfig;

/// Create a default GenConfig for testing
#[allow(dead_code)]
pub fn test_gen_config() -> GenConfig {
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 30;
    config.max_rhs_complexity = 30;
    config
}

/// Create a default SearchConfig for testing
#[allow(dead_code)]
pub fn test_search_config(target: f64) -> SearchConfig {
    SearchConfig {
        target,
        max_matches: 100,
        max_error: 1.0,
        stop_at_exact: false,
        stop_below: None,
        zero_value_threshold: 1e-4,
        newton_iterations: 15,
        user_constants: Vec::new(),
        user_functions: Vec::new(),
        refine_with_newton: true,
        rhs_allowed_symbols: None,
        rhs_excluded_symbols: None,
        show_newton: false,
        show_match_checks: false,
        show_pruned_arith: false,
        match_all_digits: false,
        derivative_margin: ries_rs::thresholds::DEGENERATE_DERIVATIVE,
    }
}

/// Common test targets with known results
#[allow(dead_code)]
pub mod targets {
    /// π ≈ 3.14159...
    pub const PI: f64 = std::f64::consts::PI;
    /// e ≈ 2.71828...
    pub const E: f64 = std::f64::consts::E;
    /// φ (golden ratio) ≈ 1.61803...
    pub const PHI: f64 = 1.618_033_988_749_895;
    /// √2 ≈ 1.41421...
    pub const SQRT2: f64 = std::f64::consts::SQRT_2;
    /// 2.5 (simple test case)
    pub const TWO_POINT_FIVE: f64 = 2.5;
}

/// Well-known expressions for testing
#[allow(dead_code)]
pub mod expressions {
    use ries_rs::symbol::Symbol;

    /// x^2 (postfix: xs)
    pub const X_SQUARED: &[Symbol] = &[Symbol::X, Symbol::Square];

    /// 2x (postfix: 2x*)
    pub const TWO_X: &[Symbol] = &[Symbol::Two, Symbol::X, Symbol::Mul];

    /// π (postfix: p)
    pub const PI: &[Symbol] = &[Symbol::Pi];

    /// e (postfix: e)
    pub const E: &[Symbol] = &[Symbol::E];
}

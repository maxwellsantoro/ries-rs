//! Common test utilities for ries-rs
//!
//! These utilities are kept for future test development.

pub mod fixtures;

/// Check if two floating point values are approximately equal
#[allow(dead_code)]
pub fn approx_eq(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() < tolerance
}

/// Check if a value is close to zero
#[allow(dead_code)]
pub fn is_near_zero(a: f64, tolerance: f64) -> bool {
    a.abs() < tolerance
}

/// Default epsilon for float comparisons
#[allow(dead_code)]
pub const DEFAULT_EPSILON: f64 = 1e-10;

/// Check if two floats are approximately equal with default epsilon
#[allow(dead_code)]
pub fn approx_eq_default(a: f64, b: f64) -> bool {
    approx_eq(a, b, DEFAULT_EPSILON)
}

/// Find a match by its LHS postfix representation
#[allow(dead_code)]
pub fn find_match_by_lhs_postfix(
    matches: &[ries_rs::search::Match],
    postfix: &str,
) -> Option<ries_rs::search::Match> {
    matches
        .iter()
        .find(|m| m.lhs.expr.to_postfix() == postfix)
        .cloned()
}

/// Find a match by its RHS postfix representation
#[allow(dead_code)]
pub fn find_match_by_rhs_postfix(
    matches: &[ries_rs::search::Match],
    postfix: &str,
) -> Option<ries_rs::search::Match> {
    matches
        .iter()
        .find(|m| m.rhs.expr.to_postfix() == postfix)
        .cloned()
}

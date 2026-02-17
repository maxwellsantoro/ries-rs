//! Common test utilities for RIES-RS integration tests

pub mod fixtures;

/// Check if two floating point values are approximately equal
pub fn approx_eq(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() < tolerance
}

/// Check if a value is close to zero
pub fn is_near_zero(a: f64, tolerance: f64) -> bool {
    a.abs() < tolerance
}

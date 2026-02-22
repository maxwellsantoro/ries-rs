//! Common test utilities for ries-rs

/// Check if two floats are approximately equal with default epsilon (1e-10)
pub fn approx_eq_default(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-10
}

//! Named threshold constants for numerical comparisons
//!
//! This module centralizes all magic numbers used for numerical comparisons
//! throughout the codebase. Using named constants improves code readability
//! and makes it easier to adjust thresholds globally.
//!
//! # Categories
//!
//! - **Match quality**: Thresholds for determining match accuracy
//! - **Newton-Raphson**: Convergence and stability thresholds
//! - **Pruning**: Thresholds for expression pruning decisions
//! - **Numerical safety**: Thresholds for avoiding numerical issues

// =============================================================================
// Match Quality Thresholds
// =============================================================================

/// Tolerance for considering a match "exact"
///
/// Matches with error below this threshold are considered mathematically exact
/// (within floating-point precision limits).
///
/// Value: 1e-14 (roughly 100x machine epsilon for f64)
pub const EXACT_MATCH_TOLERANCE: f64 = 1e-14;

/// Maximum relative error for coarse filtering
///
/// Used in the initial candidate filtering before Newton-Raphson refinement.
/// Candidates with estimated error above this are skipped.
///
/// Value: 1.0 (100% relative error)
pub const COARSE_ERROR_MAX: f64 = 1.0;

/// Minimum search radius as fraction of derivative
///
/// When searching for RHS matches, the minimum radius is this fraction
/// of the LHS derivative magnitude.
///
/// Value: 0.5
pub const MIN_SEARCH_RADIUS_FACTOR: f64 = 0.5;

// =============================================================================
// Newton-Raphson Thresholds
// =============================================================================

/// Convergence tolerance for Newton-Raphson iteration
///
/// Iteration stops when |delta| < tolerance * (1 + |x|).
///
/// Value: 1e-15 (approximately machine epsilon)
pub const NEWTON_TOLERANCE: f64 = 1e-15;

/// Threshold below which derivative is considered degenerate
///
/// If |derivative| < this threshold, the Newton-Raphson method is likely
/// to fail or produce unreliable results.
///
/// Value: 1e-100
pub const DEGENERATE_DERIVATIVE: f64 = 1e-100;

/// Maximum acceptable Newton-Raphson result for refinement success
///
/// After Newton-Raphson refinement, if |f(x) - rhs| > this threshold,
/// the result is considered not converged.
///
/// Value: 1e-10
pub const NEWTON_FINAL_TOLERANCE: f64 = 1e-10;

/// Threshold for detecting degenerate expressions during test evaluation
///
/// When testing if an expression is degenerate (derivative ~0 everywhere),
/// this threshold is used to check if derivative is still near zero.
///
/// Value: 1e-10
pub const DEGENERATE_TEST_THRESHOLD: f64 = 1e-10;

/// Maximum x value magnitude before Newton-Raphson is considered diverged
///
/// Value: 1e100
pub const NEWTON_DIVERGENCE_THRESHOLD: f64 = 1e100;

// =============================================================================
// Pruning Thresholds
// =============================================================================

/// Threshold for pruning zero-value LHS expressions
///
/// LHS expressions with |value| < this threshold are pruned to avoid
/// flooding matches with trivial identities.
///
/// Value: 1e-4
pub const ZERO_VALUE_PRUNE: f64 = 1e-4;

/// Threshold for pruning degenerate expressions (derivative near zero)
///
/// Value: 1e-10
pub const DEGENERATE_DERIVATIVE_PRUNE: f64 = 1e-10;

// =============================================================================
// Numerical Safety Thresholds
// =============================================================================

/// Minimum absolute value before division is considered division by zero
///
/// Used in evaluation to detect potential division by zero.
///
/// Value: f64::MIN_POSITIVE
pub const DIVISION_BY_ZERO_THRESHOLD: f64 = f64::MIN_POSITIVE;

/// Maximum absolute value before result is considered overflow
///
/// Used in evaluation to detect potential overflow in generated expressions.
///
/// Value: 1e12
pub const VALUE_OVERFLOW_THRESHOLD: f64 = 1e12;

/// Maximum absolute value before skipping expression entirely
///
/// Used in generation to skip expressions with extreme values.
///
/// Value: 1e10
pub const EXTREME_VALUE_THRESHOLD: f64 = 1e10;

// =============================================================================
// Pool Thresholds
// =============================================================================

/// Factor for tightening best error when exact match is found
///
/// Value: 0.999
pub const BEST_ERROR_TIGHTEN_FACTOR: f64 = 0.999;

/// Factor for tightening accept error for diversity
///
/// Value: 0.9999
pub const ACCEPT_ERROR_TIGHTEN_FACTOR: f64 = 0.9999;

/// Strict gate threshold fraction
///
/// When pool is near capacity, only accept candidates with error
/// below this fraction of accept_error.
///
/// Value: 0.5
pub const STRICT_GATE_FACTOR: f64 = 0.5;

/// Pool capacity fraction for triggering strict gate
///
/// When pool is above this fraction of capacity, strict gating is applied.
///
/// Value: 4/5 = 0.8
pub const STRICT_GATE_CAPACITY_FRACTION: f64 = 0.8;

// =============================================================================
// Quantization Thresholds
// =============================================================================

/// Scale factor for value quantization in deduplication
///
/// Values are multiplied by this factor before rounding to integer
/// for deduplication purposes.
///
/// Value: 1e8 (preserves ~8 significant digits)
pub const QUANTIZE_SCALE: f64 = 1e8;

// =============================================================================
// Helper Functions
// =============================================================================

/// Check if an error is within exact match tolerance
#[inline]
pub fn is_exact_match(error: f64) -> bool {
    error.abs() < EXACT_MATCH_TOLERANCE
}

/// Check if a derivative is degenerate (too small for Newton-Raphson)
#[inline]
pub fn is_degenerate_derivative(derivative: f64) -> bool {
    derivative.abs() < DEGENERATE_DERIVATIVE
}

/// Check if a value is effectively zero for pruning purposes
#[inline]
pub fn is_effectively_zero(value: f64) -> bool {
    value.abs() < ZERO_VALUE_PRUNE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_exact_match() {
        assert!(is_exact_match(0.0));
        assert!(is_exact_match(1e-15));
        assert!(is_exact_match(-1e-15));
        assert!(!is_exact_match(1e-13));
        assert!(!is_exact_match(0.001));
    }

    #[test]
    fn test_is_degenerate_derivative() {
        assert!(is_degenerate_derivative(0.0));
        assert!(is_degenerate_derivative(1e-101));
        assert!(is_degenerate_derivative(-1e-101));
        assert!(!is_degenerate_derivative(1e-99));
        assert!(!is_degenerate_derivative(0.001));
    }

    #[test]
    fn test_is_effectively_zero() {
        assert!(is_effectively_zero(0.0));
        assert!(is_effectively_zero(1e-5));
        assert!(is_effectively_zero(-1e-5));
        assert!(!is_effectively_zero(1e-3));
        assert!(!is_effectively_zero(0.1));
    }

    #[test]
    fn test_constants_are_sane() {
        // Exact match tolerance should be small but not at machine epsilon
        assert!(EXACT_MATCH_TOLERANCE > 1e-16);
        assert!(EXACT_MATCH_TOLERANCE < 1e-10);

        // Newton tolerance should be very tight
        assert!(NEWTON_TOLERANCE < EXACT_MATCH_TOLERANCE);

        // Pruning thresholds should be larger than exact tolerance
        assert!(ZERO_VALUE_PRUNE > EXACT_MATCH_TOLERANCE);
    }
}

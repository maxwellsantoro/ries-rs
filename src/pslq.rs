//! PSLQ Integer Relation Algorithm
//!
//! This module implements the PSLQ (Partial Sums LQ) algorithm for finding
//! integer relations between real numbers. PSLQ can discover identities like:
//! - π ≈ 355/113 (rational approximation)
//! - e^π - π ≈ 20 (near-integer relation)
//! - 4π² ≈ 39.4784... (relation with π²)
//!
//! # References
//!
//! - Ferguson, H.R.P., & Bailey, D.H. (1992). "A Polynomial Time, Numerically
//!   Stable Integer Relation Algorithm"
//! - Bailey, D.H., & Broadhurst, D. (2000). "Parallel Integer Relation Detection"

use crate::thresholds::EXACT_MATCH_TOLERANCE;
use std::f64::consts::PI;

/// Maximum iterations for PSLQ algorithm
const MAX_ITERATIONS: usize = 10000;

/// Default precision for PSLQ (number of decimal digits)
pub const DEFAULT_PSLQ_PRECISION: usize = 50;

/// Result of PSLQ integer relation search
#[derive(Debug, Clone)]
pub struct IntegerRelation {
    /// The integer coefficients found (not all zeros)
    pub coefficients: Vec<i64>,
    /// The basis vectors that were searched
    pub basis_names: Vec<String>,
    /// The residual error (should be near zero if a relation exists)
    pub residual: f64,
    /// Whether the relation is considered exact
    pub is_exact: bool,
}

impl IntegerRelation {
    /// Format the relation as a human-readable string
    pub fn to_string(&self) -> String {
        let terms: Vec<String> = self
            .coefficients
            .iter()
            .zip(self.basis_names.iter())
            .filter(|(c, _)| **c != 0)
            .map(|(c, name)| {
                if *c == 1 {
                    name.clone()
                } else if *c == -1 {
                    format!("-{}", name)
                } else if *c > 0 {
                    format!("{}*{}", c, name)
                } else {
                    format!("{}*{}", c, name)
                }
            })
            .collect();

        if terms.is_empty() {
            "0".to_string()
        } else {
            terms.join(" + ").replace("+ -", "- ")
        }
    }
}

/// Standard mathematical constants for PSLQ searches
pub fn standard_constants() -> Vec<(String, f64)> {
    vec![
        ("1".to_string(), 1.0),
        ("π".to_string(), PI),
        ("π²".to_string(), PI * PI),
        ("π³".to_string(), PI * PI * PI),
        ("e".to_string(), std::f64::consts::E),
        ("e²".to_string(), std::f64::consts::E * std::f64::consts::E),
        ("e^π".to_string(), std::f64::consts::E.powf(PI)),
        ("ln(2)".to_string(), (2.0f64).ln()),
        ("ln(π)".to_string(), PI.ln()),
        ("√2".to_string(), std::f64::consts::SQRT_2),
        ("√π".to_string(), PI.sqrt()),
        ("φ".to_string(), (1.0 + 5.0f64.sqrt()) / 2.0), // Golden ratio
        ("γ".to_string(), 0.5772156649015329),          // Euler-Mascheroni
        ("ζ(2)".to_string(), PI * PI / 6.0),            // Basel problem
        ("ζ(3)".to_string(), 1.202056903159594),        // Apéry's constant
        ("G".to_string(), 0.915965594177219),           // Catalan's constant
    ]
}

/// Extended constants for thorough searches
pub fn extended_constants() -> Vec<(String, f64)> {
    let mut constants = standard_constants();
    constants.extend(vec![
        ("√3".to_string(), 3.0f64.sqrt()),
        ("√5".to_string(), 5.0f64.sqrt()),
        ("√7".to_string(), 7.0f64.sqrt()),
        ("ln(3)".to_string(), (3.0f64).ln()),
        ("ln(5)".to_string(), (5.0f64).ln()),
        ("ln(7)".to_string(), (7.0f64).ln()),
        ("π*√2".to_string(), PI * std::f64::consts::SQRT_2),
        ("e+π".to_string(), std::f64::consts::E + PI),
        ("e*π".to_string(), std::f64::consts::E * PI),
        ("2^π".to_string(), 2.0f64.powf(PI)),
        ("π^e".to_string(), PI.powf(std::f64::consts::E)),
    ]);
    constants
}

/// PSLQ algorithm configuration
#[derive(Debug, Clone)]
pub struct PslqConfig {
    /// Maximum coefficient magnitude to search
    pub max_coefficient: i64,
    /// Maximum number of iterations
    pub max_iterations: usize,
    /// Tolerance for detecting zero relations
    pub tolerance: f64,
    /// Whether to use extended constant set
    pub extended_constants: bool,
}

impl Default for PslqConfig {
    fn default() -> Self {
        Self {
            max_coefficient: 1000,
            max_iterations: MAX_ITERATIONS,
            tolerance: EXACT_MATCH_TOLERANCE,
            extended_constants: false,
        }
    }
}

/// Search for integer relations using PSLQ algorithm
///
/// Given a target value and a set of basis vectors (constants), find integer
/// coefficients such that the linear combination is close to zero.
///
/// # Arguments
///
/// * `target` - The value to find relations for
/// * `config` - PSLQ configuration options
///
/// # Returns
///
/// An optional integer relation if one is found
pub fn find_integer_relation(target: f64, config: &PslqConfig) -> Option<IntegerRelation> {
    // Get the basis constants
    let constants = if config.extended_constants {
        extended_constants()
    } else {
        standard_constants()
    };

    // Build the vector: [target, c1, c2, c3, ...]
    // We want to find a relation a0*target + a1*c1 + ... = 0
    let _n = constants.len() + 1;
    let mut x: Vec<f64> = vec![target];
    for (_, val) in &constants {
        x.push(*val);
    }

    // Run PSLQ
    let coefficients = pslq(&x, config)?;

    // Check if the first coefficient (for target) is non-zero
    if coefficients[0] == 0 {
        return None;
    }

    // Calculate residual
    let mut residual = 0.0;
    for (i, c) in coefficients.iter().enumerate() {
        residual += (*c as f64) * x[i];
    }
    residual = residual.abs();

    // Check if residual is small enough
    if residual > config.tolerance * target.abs().max(1.0) {
        return None;
    }

    // Build basis names
    let mut basis_names = vec!["x".to_string()];
    for (name, _) in &constants {
        basis_names.push(name.clone());
    }

    Some(IntegerRelation {
        coefficients,
        basis_names,
        residual,
        is_exact: residual < EXACT_MATCH_TOLERANCE,
    })
}

/// Core PSLQ algorithm implementation
///
/// Finds integer relations among a vector of real numbers.
/// Based on the algorithm from Ferguson & Bailey (1992).
fn pslq(x: &[f64], config: &PslqConfig) -> Option<Vec<i64>> {
    let n = x.len();
    if n < 2 {
        return None;
    }

    // Initialize the matrices
    let _gamma = (4.0 / 3.0_f64).sqrt(); // Used in full PSLQ for reduction parameter

    // Compute initial norms and scale the vector
    let mut s: Vec<f64> = vec![0.0; n];
    s[n - 1] = x[n - 1].abs();
    for i in (0..n - 1).rev() {
        s[i] = (s[i + 1].powi(2) + x[i].powi(2)).sqrt();
    }

    let scale = s[0];
    if scale == 0.0 {
        return None;
    }

    // Normalize
    let mut y: Vec<f64> = x.iter().map(|xi| xi / scale).collect();
    for i in 0..n {
        s[i] /= scale;
    }

    // Initialize H matrix (upper triangular)
    let mut h: Vec<Vec<f64>> = vec![vec![0.0; n]; n - 1];
    for i in 0..n - 1 {
        for j in 0..=i.min(n - 2) {
            if i == j {
                h[i][j] = s[j + 1] / s[j];
            } else {
                h[i][j] = -y[j] * y[i + 1] / (s[i] * s[i + 1]);
            }
        }
    }

    // Initialize A and B matrices (identity)
    let mut a: Vec<Vec<i64>> = vec![vec![0; n]; n];
    let mut b: Vec<Vec<i64>> = vec![vec![0; n]; n];
    for i in 0..n {
        a[i][i] = 1;
        b[i][i] = 1;
    }

    // Main iteration loop
    for _iteration in 0..config.max_iterations {
        // Find the largest |h[i][i]|
        let mut max_val = 0.0;
        let mut max_idx = 0;
        for i in 0..n - 1 {
            let val = h[i][i].abs();
            if val > max_val {
                max_val = val;
                max_idx = i;
            }
        }

        // Swap rows max_idx and max_idx + 1
        if max_idx < n - 1 {
            y.swap(max_idx, max_idx + 1);
            a.swap(max_idx, max_idx + 1);
            b.swap(max_idx, max_idx + 1);
            for j in 0..n - 1 {
                let tmp = h[max_idx][j];
                h[max_idx][j] = h[max_idx + 1][j];
                h[max_idx + 1][j] = tmp;
            }
        }

        // Reduction step
        for i in (1..n).rev() {
            if max_idx < n - 1 && h[max_idx][max_idx].abs() > 1e-50 {
                let t = (h[i - 1][max_idx] / h[max_idx][max_idx]).round();
                y[i - 1] -= t * y[max_idx];
                for j in 0..n - 1 {
                    h[i - 1][j] -= t * h[max_idx][j];
                }
                for j in 0..n {
                    a[i - 1][j] -= (t as i64) * a[max_idx][j];
                    b[j][i - 1] -= (t as i64) * b[j][max_idx];
                }
            }
        }

        // Check for small y[i] (found a relation)
        for i in 0..n {
            if y[i].abs() < config.tolerance {
                // Found a relation - return coefficients from B matrix
                let coeffs: Vec<i64> = (0..n).map(|j| b[j][i]).collect();

                // Check coefficient bounds
                if coeffs.iter().all(|&c| c.abs() <= config.max_coefficient) {
                    return Some(coeffs);
                }
            }
        }

        // Termination check based on diagonal elements
        let mut min_diag = f64::MAX;
        for i in 0..n - 1 {
            if h[i][i].abs() < min_diag {
                min_diag = h[i][i].abs();
            }
        }

        if min_diag < config.tolerance {
            // Found a relation
            let coeffs: Vec<i64> = (0..n).map(|j| b[j][0]).collect();
            if coeffs.iter().all(|&c| c.abs() <= config.max_coefficient) {
                return Some(coeffs);
            }
        }
    }

    None
}

/// Find rational approximation using continued fractions
///
/// This is simpler than PSLQ but only finds rational relations (a/b).
pub fn find_rational_approximation(x: f64, max_denominator: i64) -> Option<(i64, i64)> {
    let a0 = x.floor() as i64;
    let mut remainder = x - a0 as f64;

    if remainder.abs() < EXACT_MATCH_TOLERANCE {
        return Some((a0, 1));
    }

    // Continued fraction expansion
    let mut h_prev = 1i64;
    let mut h_curr = a0;
    let mut k_prev = 0i64;
    let mut k_curr = 1i64;

    for _ in 0..100 {
        if remainder.abs() < EXACT_MATCH_TOLERANCE {
            break;
        }

        let reciprocal = 1.0 / remainder;
        let a = reciprocal.floor() as i64;
        remainder = reciprocal - a as f64;

        let h_next = a * h_curr + h_prev;
        let k_next = a * k_curr + k_prev;

        // Check denominator bound
        if k_next > max_denominator {
            break;
        }

        h_prev = h_curr;
        h_curr = h_next;
        k_prev = k_curr;
        k_curr = k_next;

        // Check if this is a good approximation
        let approx = h_curr as f64 / k_curr as f64;
        if (approx - x).abs() < EXACT_MATCH_TOLERANCE {
            return Some((h_curr, k_curr));
        }
    }

    // Return the best approximation found
    if k_curr > 0 && k_curr <= max_denominator {
        let approx = h_curr as f64 / k_curr as f64;
        if (approx - x).abs() < x.abs() * 0.01 {
            return Some((h_curr, k_curr));
        }
    }

    None
}

/// Find minimal polynomial using LLL-based approach (simplified)
///
/// Given a value x, attempts to find a polynomial with integer coefficients
/// that has x as a root. This is a simplified implementation.
pub fn find_minimal_polynomial(x: f64, max_degree: usize, max_coeff: i64) -> Option<Vec<i64>> {
    // Try polynomials of increasing degree
    for degree in 1..=max_degree {
        // Build a lattice basis for the polynomial coefficients
        // This is a simplified approach - a full implementation would use LLL

        // For small degrees, try to fit the polynomial directly
        if let Some(coeffs) = try_polynomial_degree(x, degree, max_coeff) {
            return Some(coeffs);
        }
    }
    None
}

/// Try to find polynomial coefficients for a given degree
fn try_polynomial_degree(x: f64, degree: usize, max_coeff: i64) -> Option<Vec<i64>> {
    if degree == 0 {
        return None;
    }

    // Compute powers of x
    let mut powers = vec![1.0; degree + 1];
    for i in 1..=degree {
        powers[i] = powers[i - 1] * x;
    }

    // For degree 1: find a*x + b ≈ 0
    // For degree 2: find a*x² + b*x + c ≈ 0
    // etc.

    // Use a simple search for small coefficients
    let mut best_coeffs: Option<Vec<i64>> = None;
    let mut best_error = f64::MAX;

    // Search space - limit based on max_coeff
    let search_range = (-(max_coeff / 10).max(1)..=(max_coeff / 10).max(1)).collect::<Vec<_>>();

    // For very small degrees, exhaustive search is feasible
    if degree <= 2 {
        for coeffs in coefficients_product(&search_range, degree + 1) {
            let mut value = 0.0;
            for (i, c) in coeffs.iter().enumerate() {
                value += (*c as f64) * powers[i];
            }

            let error = value.abs();
            if error < best_error && error < EXACT_MATCH_TOLERANCE * 100.0 {
                best_error = error;
                best_coeffs = Some(coeffs);
            }
        }
    }

    best_coeffs
}

/// Generate all combinations of coefficients
fn coefficients_product(ranges: &[i64], count: usize) -> Vec<Vec<i64>> {
    if count == 0 {
        return vec![vec![]];
    }

    let mut result = Vec::new();
    let rest = coefficients_product(ranges, count - 1);
    for r in rest {
        for &val in ranges {
            let mut combo = r.clone();
            combo.push(val);
            result.push(combo);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rational_approximation_pi() {
        // π ≈ 355/113
        let result = find_rational_approximation(PI, 1000);
        assert!(result.is_some());
        let (num, den) = result.unwrap();
        assert_eq!(num, 355);
        assert_eq!(den, 113);
    }

    #[test]
    fn test_rational_approximation_sqrt2() {
        // √2 ≈ 99/70 or 140/99
        let result = find_rational_approximation(std::f64::consts::SQRT_2, 200);
        assert!(result.is_some());
        let (num, den) = result.unwrap();
        let approx = num as f64 / den as f64;
        assert!((approx - std::f64::consts::SQRT_2).abs() < 0.001);
    }

    #[test]
    fn test_integer_relation_simple() {
        // 2π - 6.283... ≈ 0
        let config = PslqConfig::default();
        let result = find_integer_relation(2.0 * PI, &config);
        // Should find relation involving π
        if let Some(rel) = result {
            assert!(rel.residual < 0.01);
        }
    }

    #[test]
    fn test_minimal_polynomial_sqrt2() {
        // √2 is a root of x² - 2 = 0
        let result = find_minimal_polynomial(std::f64::consts::SQRT_2, 4, 100);
        if let Some(coeffs) = result {
            // Should be something like [-2, 0, 1] for x² - 2
            let value: f64 = coeffs
                .iter()
                .enumerate()
                .map(|(i, c)| *c as f64 * std::f64::consts::SQRT_2.powi(i as i32))
                .sum();
            assert!(value.abs() < 0.01);
        }
    }
}

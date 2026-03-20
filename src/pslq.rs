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

#![allow(clippy::needless_range_loop)]
#![allow(dead_code)]
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
    pub fn format(&self) -> String {
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

    let coefficients =
        find_two_term_relation(target, &constants, config).or_else(|| pslq(&x, config))?;

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

fn find_two_term_relation(
    target: f64,
    constants: &[(String, f64)],
    config: &PslqConfig,
) -> Option<Vec<i64>> {
    let residual_tolerance = config.tolerance * target.abs().max(1.0);
    let relation_len = constants.len() + 1;
    let mut best: Option<(Vec<i64>, i64, f64)> = None;

    for (idx, (_, value)) in constants.iter().enumerate() {
        let value = *value;
        if !value.is_finite() {
            continue;
        }

        let direct_residual = (target - value).abs();
        if direct_residual <= residual_tolerance {
            let mut coeffs = vec![0_i128; relation_len];
            coeffs[0] = 1;
            coeffs[idx + 1] = -1;
            if let Some(normalized) = normalize_relation(coeffs, config.max_coefficient) {
                return Some(normalized);
            }
        }

        if value == 0.0 {
            continue;
        }

        let Some((num, den)) = find_rational_approximation(target / value, config.max_coefficient)
        else {
            continue;
        };
        if den == 0 || num.abs() > config.max_coefficient || den.abs() > config.max_coefficient {
            continue;
        }

        let residual = ((den as f64) * target - (num as f64) * value).abs();
        if residual > residual_tolerance {
            continue;
        }

        let mut coeffs = vec![0_i128; relation_len];
        coeffs[0] = den as i128;
        coeffs[idx + 1] = -(num as i128);
        let Some(normalized) = normalize_relation(coeffs, config.max_coefficient) else {
            continue;
        };

        let height = normalized
            .iter()
            .map(|coeff| coeff.abs())
            .max()
            .unwrap_or(config.max_coefficient);
        match &best {
            None => best = Some((normalized, height, residual)),
            Some((_, best_height, best_residual)) => {
                if height < *best_height
                    || (height == *best_height && residual + residual_tolerance < *best_residual)
                {
                    best = Some((normalized, height, residual));
                }
            }
        }
    }

    best.map(|(coeffs, _, _)| coeffs)
}

/// Core PSLQ algorithm implementation
///
/// Finds integer relations among a vector of real numbers.
/// Based on the algorithm from Ferguson & Bailey (1992).
fn pslq(x: &[f64], config: &PslqConfig) -> Option<Vec<i64>> {
    let n = x.len();
    if n < 2 || x.iter().any(|value| !value.is_finite()) {
        return None;
    }

    // Ferguson/Bailey PSLQ uses a lower-trapezoidal H with n rows and n - 1 columns.
    // The previous implementation stored a transposed/truncated variant and skipped the
    // corner-removal rotation, which caused it to miss even direct basis relations.
    let gamma = (4.0 / 3.0_f64).sqrt();

    // Compute initial norms and scale the vector
    let mut s: Vec<f64> = vec![0.0; n];
    s[n - 1] = x[n - 1].abs();
    for i in (0..n - 1).rev() {
        s[i] = (s[i + 1].powi(2) + x[i].powi(2)).sqrt();
    }

    let scale = s[0];
    if scale <= f64::EPSILON || !scale.is_finite() {
        return None;
    }

    // Normalize
    let mut y: Vec<f64> = x.iter().map(|xi| xi / scale).collect();
    for value in &mut s {
        *value /= scale;
    }

    // Initialize H as an n × (n - 1) lower-trapezoidal matrix.
    let mut h: Vec<Vec<f64>> = vec![vec![0.0; n - 1]; n];
    for i in 0..n {
        for j in 0..n - 1 {
            if i == j {
                h[i][j] = s[j + 1] / s[j];
            } else if i > j {
                h[i][j] = -y[i] * y[j] / (s[j] * s[j + 1]);
            } else {
                h[i][j] = 0.0;
            }
        }
    }

    // Initialize A and B matrices (identity).
    let mut a: Vec<Vec<i128>> = vec![vec![0; n]; n];
    let mut b: Vec<Vec<i128>> = vec![vec![0; n]; n];
    for i in 0..n {
        a[i][i] = 1;
        b[i][i] = 1;
    }

    reduce_h(&mut y, &mut h, &mut a, &mut b, 1, n - 2);

    // Main iteration loop
    for _iteration in 0..config.max_iterations {
        if let Some(coeffs) = detect_relation(x, &y, &b, config.max_coefficient, config.tolerance) {
            return Some(coeffs);
        }

        // Select m to maximize gamma^i * |h[i][i]|.
        let mut max_metric = 0.0;
        let mut max_idx = 0;
        for i in 0..n - 1 {
            let metric = gamma.powi(i as i32) * h[i][i].abs();
            if metric > max_metric {
                max_metric = metric;
                max_idx = i;
            }
        }

        // Exchange y[m], y[m + 1], corresponding rows of A and H, and columns of B.
        y.swap(max_idx, max_idx + 1);
        a.swap(max_idx, max_idx + 1);
        h.swap(max_idx, max_idx + 1);
        for row in &mut b {
            row.swap(max_idx, max_idx + 1);
        }

        remove_corner(&mut h, max_idx);

        // Block reduction after the swap only needs to touch the affected suffix.
        reduce_h(
            &mut y,
            &mut h,
            &mut a,
            &mut b,
            max_idx + 1,
            (max_idx + 1).min(n - 2),
        );

        if let Some(coeffs) = detect_relation(x, &y, &b, config.max_coefficient, config.tolerance) {
            return Some(coeffs);
        }

        let max_diag = (0..n - 1).map(|i| h[i][i].abs()).fold(0.0_f64, f64::max);
        if max_diag <= f64::EPSILON {
            break;
        }

        // If the norm lower bound already exceeds the user's coefficient cap, no valid
        // relation can remain: any vector with |c_i| <= C has Euclidean norm <= C * sqrt(n).
        let norm_lower_bound = 1.0 / max_diag;
        let coefficient_norm_cap = (config.max_coefficient as f64) * (n as f64).sqrt();
        if norm_lower_bound > coefficient_norm_cap {
            break;
        }

        // IEEE-754 doubles only preserve 53 bits of mantissa. If A grows beyond that,
        // the floating-point y/H state is no longer trustworthy for exact detection.
        if max_abs_matrix_entry(&a) > (1_i128 << 52) {
            break;
        }
    }

    detect_relation(x, &y, &b, config.max_coefficient, config.tolerance)
}

fn reduce_h(
    y: &mut [f64],
    h: &mut [Vec<f64>],
    a: &mut [Vec<i128>],
    b: &mut [Vec<i128>],
    row_start: usize,
    max_active_col: usize,
) {
    if h.is_empty() || h[0].is_empty() || row_start >= h.len() {
        return;
    }

    let max_col_count = h[0].len();
    let active_col_count = (max_active_col + 1).min(max_col_count);

    for i in row_start.max(1)..h.len() {
        let upper = i.min(active_col_count);
        for j in (0..upper).rev() {
            let denom = h[j][j];
            if denom.abs() <= f64::EPSILON {
                continue;
            }

            let t = (h[i][j] / denom).round();
            if t == 0.0 {
                continue;
            }

            y[i] -= t * y[j];
            for k in 0..=j {
                h[i][k] -= t * h[j][k];
            }

            let t_int = t as i128;
            for k in 0..a[i].len() {
                a[i][k] -= t_int * a[j][k];
                b[k][j] += t_int * b[k][i];
            }
        }
    }
}

fn remove_corner(h: &mut [Vec<f64>], pivot_row: usize) {
    if h.is_empty() || h[0].len() < 2 || pivot_row + 1 >= h[0].len() {
        return;
    }

    let corner = h[pivot_row][pivot_row + 1];
    if corner.abs() <= f64::EPSILON {
        return;
    }

    let diagonal = h[pivot_row][pivot_row];
    let norm = (diagonal * diagonal + corner * corner).sqrt();
    if norm <= f64::EPSILON {
        return;
    }

    let c = diagonal / norm;
    let s = corner / norm;
    for row in pivot_row..h.len() {
        let left = h[row][pivot_row];
        let right = h[row][pivot_row + 1];
        h[row][pivot_row] = c * left + s * right;
        h[row][pivot_row + 1] = -s * left + c * right;
    }
}

fn detect_relation(
    x: &[f64],
    y: &[f64],
    b: &[Vec<i128>],
    max_coefficient: i64,
    tolerance: f64,
) -> Option<Vec<i64>> {
    let mut candidate_order: Vec<usize> = (0..y.len()).collect();
    candidate_order.sort_by(|&left, &right| y[left].abs().total_cmp(&y[right].abs()));

    let residual_tolerance = tolerance * x.iter().map(|value| value.abs()).sum::<f64>().max(1.0);
    let mut best: Option<(Vec<i64>, f64, f64)> = None;

    for idx in candidate_order {
        let coeffs: Vec<i128> = (0..y.len()).map(|row| b[row][idx]).collect();
        let Some(normalized) = normalize_relation(coeffs, max_coefficient) else {
            continue;
        };

        let residual = x
            .iter()
            .zip(normalized.iter())
            .map(|(value, coeff)| value * (*coeff as f64))
            .sum::<f64>()
            .abs();
        if residual > residual_tolerance {
            continue;
        }

        let y_magnitude = y[idx].abs();
        match &best {
            None => best = Some((normalized, residual, y_magnitude)),
            Some((_, best_residual, best_y)) => {
                let clearly_better = residual + residual_tolerance < *best_residual;
                let same_residual = (residual - *best_residual).abs() <= residual_tolerance;
                if clearly_better || (same_residual && y_magnitude < *best_y) {
                    best = Some((normalized, residual, y_magnitude));
                }
            }
        }
    }

    best.map(|(coeffs, _, _)| coeffs)
}

fn normalize_relation(coeffs: Vec<i128>, max_coefficient: i64) -> Option<Vec<i64>> {
    if coeffs.iter().all(|&coeff| coeff == 0) {
        return None;
    }

    let mut gcd = 0_i128;
    for &coeff in &coeffs {
        gcd = gcd_i128(gcd, coeff.abs());
    }

    let mut normalized = if gcd > 1 {
        coeffs
            .into_iter()
            .map(|coeff| coeff / gcd)
            .collect::<Vec<_>>()
    } else {
        coeffs
    };

    if let Some(&first_non_zero) = normalized.iter().find(|&&coeff| coeff != 0) {
        if first_non_zero < 0 {
            for coeff in &mut normalized {
                *coeff = -*coeff;
            }
        }
    }

    let cap = i128::from(max_coefficient);
    if normalized.iter().any(|&coeff| coeff.abs() > cap) {
        return None;
    }

    normalized
        .into_iter()
        .map(|coeff| i64::try_from(coeff).ok())
        .collect()
}

fn gcd_i128(mut left: i128, mut right: i128) -> i128 {
    if left == 0 {
        return right;
    }
    if right == 0 {
        return left;
    }

    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.abs()
}

fn max_abs_matrix_entry(matrix: &[Vec<i128>]) -> i128 {
    matrix
        .iter()
        .flat_map(|row| row.iter())
        .map(|value| value.abs())
        .max()
        .unwrap_or(0)
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
        let config = PslqConfig::default();
        let rel = find_integer_relation(2.0 * PI, &config).expect("2*pi should be found");
        assert_eq!(rel.format(), "x - 2*π");
        assert!(rel.residual < EXACT_MATCH_TOLERANCE);
    }

    #[test]
    fn test_pslq_duplicate_relation() {
        let coeffs = pslq(&[PI, PI], &PslqConfig::default()).expect("duplicate relation");
        assert_eq!(coeffs, vec![1, -1]);
    }

    #[test]
    fn test_pslq_scalar_multiple_relation() {
        let coeffs =
            pslq(&[2.0 * PI, PI], &PslqConfig::default()).expect("scalar multiple relation");
        assert_eq!(coeffs, vec![1, -2]);
    }

    #[test]
    fn test_integer_relation_direct_basis_hits() {
        let config = PslqConfig::default();

        let pi = find_integer_relation(PI, &config).expect("pi should be found");
        assert_eq!(pi.format(), "x - π");

        let phi = find_integer_relation((1.0 + 5.0_f64.sqrt()) / 2.0, &config)
            .expect("phi should be found");
        assert_eq!(phi.format(), "x - φ");

        let sqrt_pi = find_integer_relation(PI.sqrt(), &config).expect("sqrt(pi) should be found");
        assert_eq!(sqrt_pi.format(), "x - √π");

        let zeta2 = find_integer_relation(PI * PI / 6.0, &config).expect("zeta(2) should be found");
        assert_eq!(zeta2.format(), "x - ζ(2)");
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

    #[test]
    fn test_pslq_last_diagonal_no_panic() {
        let config = PslqConfig {
            max_iterations: 1,
            ..PslqConfig::default()
        };

        let _ = pslq(&[100.0, 1.0, 1.0], &config);
    }

    #[test]
    fn test_reduce_h_keeps_y_in_sync_with_a() {
        let x: [f64; 3] = [10.0, 1.0, 1.0];
        let n = x.len();

        let mut s = vec![0.0; n];
        s[n - 1] = x[n - 1].abs();
        for i in (0..n - 1).rev() {
            s[i] = (s[i + 1].powi(2) + x[i].powi(2)).sqrt();
        }
        let scale = s[0];

        let mut y: Vec<f64> = x.iter().map(|value| value / scale).collect();
        for value in &mut s {
            *value /= scale;
        }

        let mut h = vec![vec![0.0; n - 1]; n];
        for i in 0..n {
            for j in 0..n - 1 {
                if i == j {
                    h[i][j] = s[j + 1] / s[j];
                } else if i > j {
                    h[i][j] = -y[i] * y[j] / (s[j] * s[j + 1]);
                }
            }
        }

        let mut a = vec![vec![0_i128; n]; n];
        let mut b = vec![vec![0_i128; n]; n];
        for i in 0..n {
            a[i][i] = 1;
            b[i][i] = 1;
        }

        reduce_h(&mut y, &mut h, &mut a, &mut b, 1, n - 2);
        assert!(
            max_abs_matrix_entry(&a) > 1,
            "test vector should trigger a non-trivial reduction"
        );

        for i in 0..n {
            let expected = a[i]
                .iter()
                .zip(x.iter())
                .map(|(coeff, value)| (*coeff as f64) * *value / scale)
                .sum::<f64>();
            assert!(
                (y[i] - expected).abs() < 1e-12,
                "row {i} drifted: y={}, expected={expected}",
                y[i]
            );
        }
    }
}

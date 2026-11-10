//! High-precision verification for match refinement
//!
//! After f64 search finds candidate matches, this module verifies them
//! at higher precision to distinguish true formulas from numerical coincidences.

use crate::search::Match;
#[cfg(feature = "highprec")]
use crate::symbol::Symbol;
#[cfg(feature = "highprec")]
use crate::thresholds::EXACT_MATCH_TOLERANCE;

#[cfg(feature = "highprec")]
use crate::precision::{HighPrec, RiesFloat};

/// Result of high-precision verification
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Original match (f64 precision)
    pub original: Match,
    /// Estimated absolute root error. Infinity means a failed verification.
    /// `None` means no high-precision evaluation was available.
    pub highprec_error: Option<f64>,
    /// Whether the match is verified at high precision
    pub is_verified: bool,
    /// Relative change in error (highprec_error / original.error)
    /// Values >> 1 indicate the f64 match was an impostor
    pub error_ratio: Option<f64>,
}

/// Outcome of the high-precision evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    Verified,
    Failed,
    Unavailable,
}

impl VerificationResult {
    /// Distinguish a rejected equation from one we could not evaluate.
    pub fn status(&self) -> VerificationStatus {
        match self.highprec_error {
            None => VerificationStatus::Unavailable,
            Some(_) if self.is_verified => VerificationStatus::Verified,
            Some(_) => VerificationStatus::Failed,
        }
    }

    #[cfg(feature = "highprec")]
    fn unavailable(original: Match) -> Self {
        Self {
            original,
            highprec_error: None,
            is_verified: false,
            error_ratio: None,
        }
    }

    /// Create a verification result for f64 mode (no high precision)
    pub fn f64_result(m: Match) -> Self {
        Self {
            original: m,
            highprec_error: None,
            is_verified: true, // Assume verified in f64 mode
            error_ratio: None,
        }
    }
}

/// Verify matches at high precision
///
/// When the `highprec` feature is enabled, this re-evaluates matches
/// using arbitrary precision to verify they are true formulas.
#[cfg(feature = "highprec")]
pub fn verify_matches_highprec(
    matches: Vec<Match>,
    target: f64,
    precision_bits: u32,
    user_constants: &[crate::profile::UserConstant],
) -> Vec<VerificationResult> {
    verify_matches_highprec_with_trig_scale(
        matches,
        target,
        precision_bits,
        user_constants,
        crate::eval::DEFAULT_TRIG_ARGUMENT_SCALE,
    )
}

/// Verify matches at high precision using an explicit trig argument scale.
#[cfg(feature = "highprec")]
pub fn verify_matches_highprec_with_trig_scale(
    matches: Vec<Match>,
    target: f64,
    precision_bits: u32,
    user_constants: &[crate::profile::UserConstant],
    trig_argument_scale: f64,
) -> Vec<VerificationResult> {
    if !(rug::float::prec_min()..=rug::float::prec_max()).contains(&precision_bits) {
        return matches
            .into_iter()
            .map(VerificationResult::unavailable)
            .collect();
    }
    let target_hp = HighPrec::from_f64_with_prec(target, precision_bits);
    let hp_constants: Vec<_> = user_constants
        .iter()
        .map(|constant| HighPrec::from_f64_with_prec(constant.value, precision_bits))
        .collect();
    let eval_context = crate::eval::EvalContext::from_slices(user_constants, &[])
        .with_trig_argument_scale(trig_argument_scale);

    matches
        .into_iter()
        .map(|m| {
            // Refine from the f64 root without allowing Newton iteration to jump
            // to a different solution. Evaluate values at the full-precision root;
            // f64 derivatives suffice for convergence near the supplied root.
            let initial = HighPrec::from_f64_with_prec(m.x_value, precision_bits);
            let mut root = initial.clone();
            let max_displacement = 64.0 * f64::EPSILON * (1.0 + m.x_value.abs());
            let mut verified = false;
            let mut available = true;
            for _ in 0..8 {
                let lhs = evaluate_highprec(
                    &m.lhs.expr,
                    &root,
                    precision_bits,
                    &hp_constants,
                    trig_argument_scale,
                );
                let rhs = evaluate_highprec(
                    &m.rhs.expr,
                    &root,
                    precision_bits,
                    &hp_constants,
                    trig_argument_scale,
                );
                let (Some(lhs), Some(rhs)) = (lhs, rhs) else {
                    available = false;
                    break;
                };
                let lhs_abs = lhs.clone().abs();
                let rhs_abs = rhs.clone().abs();
                let scale = if lhs_abs > rhs_abs { lhs_abs } else { rhs_abs };
                let residual = lhs - rhs;
                if !scale.is_finite() || !residual.is_finite() {
                    break;
                }
                let zero = HighPrec::zero_with_prec(precision_bits);
                if residual == zero {
                    verified = true;
                    break;
                }
                let sample_x = root.to_f64();
                let slope = match (
                    crate::eval::evaluate_with_context(&m.lhs.expr, sample_x, &eval_context),
                    crate::eval::evaluate_with_context(&m.rhs.expr, sample_x, &eval_context),
                ) {
                    (Ok(lhs), Ok(rhs)) => lhs.derivative - rhs.derivative,
                    _ => {
                        available = false;
                        break;
                    }
                };
                if !slope.is_finite() || slope.abs() < crate::DEGENERATE_DERIVATIVE {
                    break;
                }
                let correction =
                    residual.clone() / HighPrec::from_f64_with_prec(slope, precision_bits);
                let tolerance = HighPrec::from_f64_with_prec(EXACT_MATCH_TOLERANCE, precision_bits);
                let x_scale = HighPrec::from_f64_with_prec(1.0 + sample_x.abs(), precision_bits);
                // Only accept a root already re-evaluated after refinement.
                if root != initial
                    && residual.abs() <= tolerance.clone() * scale
                    && correction.clone().abs() <= tolerance * x_scale
                {
                    verified = true;
                    break;
                }
                let next = root.clone() - correction;
                if (next.clone() - initial.clone()).abs()
                    > HighPrec::from_f64_with_prec(max_displacement, precision_bits)
                    || next == root
                {
                    break;
                }
                root = next;
            }
            if !available {
                VerificationResult::unavailable(m)
            } else if verified {
                let hp_error = (root - target_hp.clone()).abs().to_f64();
                let error_ratio = if m.error.abs() > 1e-20 {
                    Some(hp_error / m.error.abs())
                } else {
                    None
                };
                VerificationResult {
                    original: m,
                    highprec_error: Some(hp_error),
                    is_verified: true,
                    error_ratio,
                }
            } else {
                VerificationResult {
                    original: m,
                    highprec_error: Some(f64::INFINITY),
                    is_verified: false,
                    error_ratio: None,
                }
            }
        })
        .collect()
}

/// Fallback for non-highprec builds
#[cfg(not(feature = "highprec"))]
pub fn verify_matches_highprec(
    matches: Vec<Match>,
    _target: f64,
    _precision_bits: u32,
    _user_constants: &[crate::profile::UserConstant],
) -> Vec<VerificationResult> {
    verify_matches_highprec_with_trig_scale(
        matches,
        _target,
        _precision_bits,
        _user_constants,
        crate::eval::DEFAULT_TRIG_ARGUMENT_SCALE,
    )
}

/// Fallback for non-highprec builds with explicit trig scaling.
#[cfg(not(feature = "highprec"))]
pub fn verify_matches_highprec_with_trig_scale(
    matches: Vec<Match>,
    _target: f64,
    _precision_bits: u32,
    _user_constants: &[crate::profile::UserConstant],
    _trig_argument_scale: f64,
) -> Vec<VerificationResult> {
    matches
        .into_iter()
        .map(VerificationResult::f64_result)
        .collect()
}

/// Evaluate an expression at high precision
#[cfg(feature = "highprec")]
fn evaluate_highprec(
    expr: &crate::expr::Expression,
    x: &HighPrec,
    precision_bits: u32,
    user_constants: &[HighPrec],
    trig_argument_scale: f64,
) -> Option<HighPrec> {
    // Use precision-aware constructors for full accuracy beyond f64 limits
    let zero = HighPrec::zero_with_prec(precision_bits);
    let one = HighPrec::one_with_prec(precision_bits);
    let custom_trig_scale = (trig_argument_scale != crate::eval::DEFAULT_TRIG_ARGUMENT_SCALE)
        .then(|| HighPrec::from_f64_with_prec(trig_argument_scale, precision_bits));
    let x_hp = x.clone();

    let mut stack: Vec<HighPrec> = Vec::with_capacity(32);

    let mut index = 0;
    while index < expr.len() {
        if let Some((len, value)) = expr.decimal_literal_at(index) {
            stack.push(HighPrec::from_f64_with_prec(value, precision_bits));
            index += len;
            continue;
        }
        let sym = expr.symbols()[index];
        index += 1;
        match sym {
            // Numbers - use string-based construction for consistency
            Symbol::One => stack.push(one.clone()),
            Symbol::Two => stack.push(HighPrec::from_str_with_prec("2", precision_bits)),
            Symbol::Three => stack.push(HighPrec::from_str_with_prec("3", precision_bits)),
            Symbol::Four => stack.push(HighPrec::from_str_with_prec("4", precision_bits)),
            Symbol::Five => stack.push(HighPrec::from_str_with_prec("5", precision_bits)),
            Symbol::Six => stack.push(HighPrec::from_str_with_prec("6", precision_bits)),
            Symbol::Seven => stack.push(HighPrec::from_str_with_prec("7", precision_bits)),
            Symbol::Eight => stack.push(HighPrec::from_str_with_prec("8", precision_bits)),
            Symbol::Nine => stack.push(HighPrec::from_str_with_prec("9", precision_bits)),

            // Variables and constants - use precision-aware constructors
            Symbol::X => stack.push(x_hp.clone()),
            Symbol::Pi => stack.push(HighPrec::pi_with_prec(precision_bits)),
            Symbol::E => stack.push(HighPrec::e_with_prec(precision_bits)),
            Symbol::Phi => stack.push(HighPrec::phi_with_prec(precision_bits)),
            Symbol::Gamma => stack.push(HighPrec::gamma_with_prec(precision_bits)),
            Symbol::Apery => stack.push(HighPrec::apery_with_prec(precision_bits)),
            Symbol::Catalan => stack.push(HighPrec::catalan_with_prec(precision_bits)),
            Symbol::Plastic => stack.push(HighPrec::plastic_with_prec(precision_bits)),

            // Binary operators
            Symbol::Add => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(a + b);
            }
            Symbol::Sub => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(a - b);
            }
            Symbol::Mul => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                stack.push(a * b);
            }
            Symbol::Div => {
                let b = stack.pop()?;
                let a = stack.pop()?;
                if b.clone() == zero {
                    return None;
                }
                stack.push(a / b);
            }
            Symbol::Pow => {
                let exp = stack.pop()?;
                let base = stack.pop()?;
                stack.push(base.pow(exp));
            }

            // Unary operators
            Symbol::Neg => {
                let a = stack.pop()?;
                stack.push(-a);
            }
            Symbol::Recip => {
                let a = stack.pop()?;
                if a.clone() == zero {
                    return None;
                }
                stack.push(one.clone() / a);
            }
            Symbol::Sqrt => {
                let a = stack.pop()?;
                if a.clone() < zero {
                    return None;
                }
                stack.push(a.sqrt());
            }
            Symbol::Square => {
                let a = stack.pop()?;
                stack.push(a.clone() * a);
            }
            Symbol::Ln => {
                let a = stack.pop()?;
                if a.clone() <= zero {
                    return None;
                }
                stack.push(a.ln());
            }
            Symbol::Exp => {
                let a = stack.pop()?;
                stack.push(a.exp());
            }

            // Trig functions
            Symbol::SinPi => {
                let a = stack.pop()?;
                stack.push(if let Some(scale) = &custom_trig_scale {
                    (scale.clone() * a).sin()
                } else {
                    a.sin_pi()
                });
            }
            Symbol::CosPi => {
                let a = stack.pop()?;
                stack.push(if let Some(scale) = &custom_trig_scale {
                    (scale.clone() * a).cos()
                } else {
                    a.cos_pi()
                });
            }
            Symbol::TanPi => {
                let a = stack.pop()?;
                stack.push(if let Some(scale) = &custom_trig_scale {
                    (scale.clone() * a).tan()
                } else {
                    a.tan_pi()
                });
            }

            // User constants
            sym if matches!(
                sym,
                Symbol::UserConstant0
                    | Symbol::UserConstant1
                    | Symbol::UserConstant2
                    | Symbol::UserConstant3
                    | Symbol::UserConstant4
                    | Symbol::UserConstant5
                    | Symbol::UserConstant6
                    | Symbol::UserConstant7
                    | Symbol::UserConstant8
                    | Symbol::UserConstant9
                    | Symbol::UserConstant10
                    | Symbol::UserConstant11
                    | Symbol::UserConstant12
                    | Symbol::UserConstant13
                    | Symbol::UserConstant14
                    | Symbol::UserConstant15
            ) =>
            {
                let idx = sym.user_constant_index()? as usize;
                if idx < user_constants.len() {
                    stack.push(user_constants[idx].clone());
                } else {
                    return None;
                }
            }

            // Skip other symbols for now
            _ => return None,
        }
    }

    stack.pop()
}

/// Format verification results for display
pub fn format_verification_report(results: &[VerificationResult], max_display: usize) -> String {
    format_verification_report_with(results, max_display, |expr| expr.to_infix())
}

/// Format verification using the same expression rendering as the search report.
pub fn format_verification_report_with(
    results: &[VerificationResult],
    max_display: usize,
    render: impl Fn(&crate::expr::Expression) -> String,
) -> String {
    let mut output = String::new();

    // Separate verified and unverified
    let verified: Vec<_> = results
        .iter()
        .filter(|r| r.status() == VerificationStatus::Verified)
        .take(max_display)
        .collect();

    let unverified: Vec<_> = results
        .iter()
        .filter(|r| r.status() == VerificationStatus::Failed)
        .take(max_display)
        .collect();

    if !verified.is_empty() {
        output.push_str("\n  -- Verified at high precision --\n\n");
        for r in &verified {
            let hp_str = match r.highprec_error {
                Some(e) if e.is_finite() => format!(" [hp error: {:.2e}]", e),
                Some(_) => " [hp error: inf]".to_string(),
                None => String::new(),
            };
            let ratio_str = match r.error_ratio {
                Some(ratio) if ratio > 1.1 => {
                    format!(" [error ratio: {:.1}x - likely impostor]", ratio)
                }
                Some(ratio) if ratio > 1.01 => format!(" [error ratio: {:.2}x]", ratio),
                _ => String::new(),
            };
            output.push_str(&format!(
                "  {:<24} = {:<24}  {{{}}}{}{}\n",
                render(&r.original.lhs.expr),
                render(&r.original.rhs.expr),
                r.original.complexity,
                hp_str,
                ratio_str
            ));
        }
    }

    if !unverified.is_empty() {
        output.push_str("\n  -- Failed high-precision verification (impostors) --\n\n");
        for r in &unverified {
            output.push_str(&format!(
                "  {:<24} = {:<24}  {{{}}}\n",
                render(&r.original.lhs.expr),
                render(&r.original.rhs.expr),
                r.original.complexity
            ));
        }
    }

    let mut unavailable = results
        .iter()
        .filter(|r| r.status() == VerificationStatus::Unavailable)
        .take(max_display)
        .peekable();
    if unavailable.peek().is_some() {
        output.push_str("\n  -- High-precision evaluation unavailable --\n\n");
        for r in unavailable {
            output.push_str(&format!(
                "  {:<24} = {:<24}  {{{}}}\n",
                render(&r.original.lhs.expr),
                render(&r.original.rhs.expr),
                r.original.complexity,
            ));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_f64_result() {
        let m = crate::search::Match {
            lhs: crate::expr::EvaluatedExpr::new(
                crate::expr::Expression::parse("x").unwrap(),
                2.5,
                1.0,
                crate::symbol::NumType::Integer,
            ),
            rhs: crate::expr::EvaluatedExpr::new(
                crate::expr::Expression::parse("5").unwrap(),
                5.0,
                0.0,
                crate::symbol::NumType::Integer,
            ),
            x_value: 2.5,
            error: 0.0,
            complexity: 14,
            exact: true,
        };

        let result = VerificationResult::f64_result(m);
        assert!(result.is_verified);
        assert!(result.highprec_error.is_none());
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_format_verification_report() {
        let m = crate::search::Match {
            lhs: crate::expr::EvaluatedExpr::new(
                crate::expr::Expression::parse("x").unwrap(),
                3.14159,
                1.0,
                crate::symbol::NumType::Integer,
            ),
            rhs: crate::expr::EvaluatedExpr::new(
                crate::expr::Expression::parse("p").unwrap(),
                std::f64::consts::PI,
                0.0,
                crate::symbol::NumType::Integer,
            ),
            x_value: std::f64::consts::PI,
            error: 1e-10,
            complexity: 14,
            exact: false,
        };

        let verified = VerificationResult {
            original: m,
            highprec_error: Some(1e-12),
            is_verified: true,
            error_ratio: Some(0.01),
        };

        let report = format_verification_report(&[verified], 10);
        assert!(report.contains("Verified at high precision"));
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn refinement_is_independent_of_reciprocal_scaling() {
        fn candidate(lhs: &str, rhs: &str, root: f64) -> crate::search::Match {
            let make = |s| {
                crate::expr::EvaluatedExpr::new(
                    crate::expr::Expression::parse(s).unwrap(),
                    0.0,
                    0.0,
                    crate::symbol::NumType::Transcendental,
                )
            };
            crate::search::Match {
                lhs: make(lhs),
                rhs: make(rhs),
                x_value: root,
                error: 0.0,
                complexity: 1,
                exact: false,
            }
        }
        let root = 100.12337445797377_f64;
        // sin(pi*x) = sqrt(1/7), and the equivalent reciprocal equation.
        let root = root + ((1.0_f64 / 7.0).sqrt().asin() / std::f64::consts::PI + 100.0 - root);
        let results = verify_matches_highprec(
            vec![candidate("xS", "7rq", root), candidate("xSr", "7q", root)],
            root,
            128,
            &[],
        );
        assert!(results.iter().all(|r| r.is_verified), "{results:?}");
        assert!(
            (results[0].highprec_error.unwrap() - results[1].highprec_error.unwrap()).abs() < 1e-25
        );
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_evaluate_highprec_reads_user_constant_slots() {
        let expr = crate::expr::Expression::from_symbols(&[crate::symbol::Symbol::UserConstant0]);
        let constants = vec![HighPrec::from_f64_with_prec(1.234567890123456, 256)];

        let evaluated = evaluate_highprec(
            &expr,
            &HighPrec::from_f64_with_prec(0.0, 256),
            256,
            &constants,
            crate::eval::DEFAULT_TRIG_ARGUMENT_SCALE,
        )
        .expect("expected user constant slot to resolve");
        assert!((evaluated.to_f64() - 1.234567890123456).abs() < 1e-15);
    }

    #[cfg(feature = "highprec")]
    #[test]
    fn test_evaluate_highprec_fails_for_missing_user_constant_slot() {
        let expr = crate::expr::Expression::from_symbols(&[crate::symbol::Symbol::UserConstant1]);
        let constants = vec![HighPrec::from_f64_with_prec(1.0, 256)];

        let evaluated = evaluate_highprec(
            &expr,
            &HighPrec::from_f64_with_prec(0.0, 256),
            256,
            &constants,
            crate::eval::DEFAULT_TRIG_ARGUMENT_SCALE,
        );
        assert!(
            evaluated.is_none(),
            "missing user constant slot should fail verification evaluation"
        );
    }
}

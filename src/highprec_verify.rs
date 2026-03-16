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
    /// Error at high precision (None if f64 mode)
    pub highprec_error: Option<f64>,
    /// Whether the match is verified at high precision
    pub is_verified: bool,
    /// Relative change in error (highprec_error / original.error)
    /// Values >> 1 indicate the f64 match was an impostor
    pub error_ratio: Option<f64>,
}

impl VerificationResult {
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
    let target_hp = HighPrec::from_f64_with_prec(target, precision_bits);
    let tolerance_hp = HighPrec::from_f64_with_prec(EXACT_MATCH_TOLERANCE, precision_bits);

    matches
        .into_iter()
        .map(|m| {
            // Get user constant values at high precision
            let hp_constants: Vec<HighPrec> = user_constants
                .iter()
                .map(|uc| HighPrec::from_f64_with_prec(uc.value, precision_bits))
                .collect();

            // Evaluate LHS(x) at high precision
            let lhs_val = evaluate_highprec(
                &m.lhs.expr,
                m.x_value,
                precision_bits,
                &hp_constants,
                trig_argument_scale,
            );

            // Evaluate RHS at high precision
            let rhs_val = evaluate_highprec(
                &m.rhs.expr,
                m.x_value,
                precision_bits,
                &hp_constants,
                trig_argument_scale,
            );

            match (lhs_val, rhs_val) {
                (Some(lhs), Some(rhs)) => {
                    // Check if LHS ≈ RHS at high precision
                    let diff = (lhs.clone() - rhs.clone()).abs();

                    if diff.clone() < tolerance_hp.clone() {
                        // Equation holds at high precision - verified!
                        let hp_error = (lhs - target_hp.clone()).abs().to_f64();
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
                        // Equation doesn't hold at high precision - impostor
                        VerificationResult {
                            original: m,
                            highprec_error: Some(f64::INFINITY),
                            is_verified: false,
                            error_ratio: None,
                        }
                    }
                }
                _ => {
                    // Evaluation failed at high precision
                    VerificationResult {
                        original: m,
                        highprec_error: Some(f64::INFINITY),
                        is_verified: false,
                        error_ratio: None,
                    }
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
    x: f64,
    precision_bits: u32,
    user_constants: &[HighPrec],
    trig_argument_scale: f64,
) -> Option<HighPrec> {
    // Use precision-aware constructors for full accuracy beyond f64 limits
    let zero = HighPrec::zero_with_prec(precision_bits);
    let one = HighPrec::one_with_prec(precision_bits);
    let trig_scale = HighPrec::from_f64_with_prec(trig_argument_scale, precision_bits);
    let x_hp = HighPrec::from_f64_with_prec(x, precision_bits);

    let mut stack: Vec<HighPrec> = Vec::with_capacity(32);

    for sym in expr.symbols() {
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
                stack.push((trig_scale.clone() * a).sin());
            }
            Symbol::CosPi => {
                let a = stack.pop()?;
                stack.push((trig_scale.clone() * a).cos());
            }
            Symbol::TanPi => {
                let a = stack.pop()?;
                stack.push((trig_scale.clone() * a).tan());
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
    let mut output = String::new();

    // Separate verified and unverified
    let verified: Vec<_> = results
        .iter()
        .filter(|r| r.is_verified)
        .take(max_display)
        .collect();

    let unverified: Vec<_> = results
        .iter()
        .filter(|r| !r.is_verified)
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
                r.original.lhs.expr.to_infix(),
                r.original.rhs.expr.to_infix(),
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
                r.original.lhs.expr.to_infix(),
                r.original.rhs.expr.to_infix(),
                r.original.complexity
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
    fn test_evaluate_highprec_reads_user_constant_slots() {
        let expr = crate::expr::Expression::from_symbols(&[crate::symbol::Symbol::UserConstant0]);
        let constants = vec![HighPrec::from_f64_with_prec(1.234567890123456, 256)];

        let evaluated = evaluate_highprec(
            &expr,
            0.0,
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
            0.0,
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

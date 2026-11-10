use crate::eval::EvalContext;

#[cfg(test)]
use crate::thresholds::DEGENERATE_DERIVATIVE;
use crate::thresholds::{NEWTON_DIVERGENCE_THRESHOLD, NEWTON_FINAL_TOLERANCE, NEWTON_TOLERANCE};

/// Newton-Raphson method to find x where lhs(x) = rhs_value
/// Uses configurable max iterations - balances speed vs. convergence
///
/// Uses thread-local workspace for zero heap allocations in hot loop.
#[cfg(test)]
pub(super) fn newton_raphson(
    lhs: &crate::expr::Expression,
    rhs_value: f64,
    initial_x: f64,
    max_iterations: usize,
) -> Option<f64> {
    let context = EvalContext::new();
    newton_raphson_with_constants(
        lhs,
        rhs_value,
        initial_x,
        max_iterations,
        &context,
        false,
        DEGENERATE_DERIVATIVE,
    )
    .map(|hit| hit.x)
}

/// A candidate Newton returned. The step test can fire before the residual is small.
///
/// Exactness is not decided here. The caller checks the two sides at the typed
/// target; the residual at the landed float can be zero on a different root.
#[derive(Clone, Copy)]
pub(super) struct NewtonHit {
    pub x: f64,
    /// Accepted by the tight residual test or the loose local-root test.
    pub real: bool,
}

/// Residual small enough, relative to the RHS, to call the iterate a root.
fn newton_residual_acceptable(value: f64, rhs_value: f64) -> bool {
    let diff = (value - rhs_value).abs();
    if diff == 0.0 {
        return true;
    }
    let scale = rhs_value.abs().max(1.0);
    diff <= NEWTON_FINAL_TOLERANCE * scale
}

/// Relative cap for a root that is only a few ulps away in `x`.
///
/// `max(|rhs|, 1)` made every right-hand side below 1 an absolute `1e-5`
/// window. `sinpi(1/x) = exp(-8)` at `1.77e-11` has sides near `3.3e-4` and a
/// gap of about `1e-5`, which is a 3% relative miss and used to pass.
const LOOSE_RELATIVE_CAP: f64 = 1e-5;
/// Floor so two vanished sides do not make the cap zero. Far below any value
/// the search treats as a real magnitude.
const LOOSE_RESIDUAL_FLOOR: f64 = 1e-300;

/// True when `x` is within a few ulps of a smooth root and the residual is
/// small relative to both sides.
///
/// The plain `1e-10` test rejects a steep genuine root: sitting a fraction of
/// an ulp off a derivative of `1e15` leaves a relative residual around `1e-6`
/// (phi). The step test also stops a couple of ulps early (`0.123456789`'s
/// last match is 1.09 ulps off). A coarse grid is a different thing: at `1e15`
/// half an ulp is `0.0625`, so `e^sinpi(x) = 2` can land one ulp away with a
/// residual of `0.7`. Those stay above the relative cap.
fn ulp_local_root(value: f64, derivative: f64, rhs_value: f64, x: f64) -> bool {
    let f = value - rhs_value;
    if !derivative.is_finite() || derivative == 0.0 || !f.is_finite() {
        return false;
    }
    let scale = value.abs().max(rhs_value.abs()).max(LOOSE_RESIDUAL_FLOOR);
    if f.abs() > LOOSE_RELATIVE_CAP * scale {
        return false;
    }
    let gap = (x.next_up() - x).abs().max((x - x.next_down()).abs());
    gap > 0.0 && (f / derivative).abs() <= 8.0 * gap
}

fn hit_from_sample(x: f64, value: f64, derivative: f64, rhs_value: f64) -> NewtonHit {
    NewtonHit {
        x,
        real: sample_is_root(value, derivative, rhs_value, x),
    }
}

fn sample_is_root(value: f64, derivative: f64, rhs_value: f64, x: f64) -> bool {
    newton_residual_acceptable(value, rhs_value) || ulp_local_root(value, derivative, rhs_value, x)
}

/// Whether the refined float solves the equation.
///
/// Applied to every root the step test returns, including ones that moved off
/// the target. At `1e15` a step of `1/π` still passes `|delta| < 1e-15*(1+|x|)`
/// and lands one ulp away, where `e^sinpi` is about 1.3 rather than 2.
#[cfg(test)]
pub(super) fn refined_match_is_real(
    lhs: &crate::expr::Expression,
    rhs_value: f64,
    refined_x: f64,
    eval_context: &EvalContext<'_>,
) -> bool {
    crate::eval::evaluate_search(lhs, refined_x, eval_context)
        .is_ok_and(|result| sample_is_root(result.value, result.derivative, rhs_value, refined_x))
}

/// Newton-Raphson with user constants support
#[allow(clippy::too_many_arguments)]
pub(super) fn newton_raphson_with_constants(
    lhs: &crate::expr::Expression,
    rhs_value: f64,
    initial_x: f64,
    max_iterations: usize,
    eval_context: &EvalContext<'_>,
    show_newton: bool,
    derivative_margin: f64,
) -> Option<NewtonHit> {
    let mut x = initial_x;
    let tolerance = NEWTON_TOLERANCE;

    for iter in 0..max_iterations {
        let result = crate::eval::evaluate_search(lhs, x, eval_context).ok()?;
        let f = result.value - rhs_value;
        let df = result.derivative;

        if df.abs() < derivative_margin {
            if show_newton {
                eprintln!("  [newton] iter={} x={:.10} derivative too small", iter, x);
            }
            return None; // Derivative too small
        }

        let x_before = x;
        let delta = f / df;
        x -= delta;

        if show_newton {
            eprintln!("  [newton] iter={} x={:.10} dx={:.10e}", iter, x, delta);
        }

        if delta.abs() < tolerance * (1.0 + x.abs()) {
            // The historical step test is wider than one ulp for large |x|.
            // Decide from the sample already in hand when the float did not
            // move; otherwise evaluate the float the step actually landed on.
            // A step this small on an ordinary target is sub-ulp, so the
            // extra evaluation is the large-target case.
            let hit = if x == x_before {
                hit_from_sample(x, result.value, df, rhs_value)
            } else {
                match crate::eval::evaluate_search(lhs, x, eval_context) {
                    Ok(at) => hit_from_sample(x, at.value, at.derivative, rhs_value),
                    Err(_) => NewtonHit { x, real: false },
                }
            };
            return Some(hit);
        }

        // Check for divergence
        if x.abs() > NEWTON_DIVERGENCE_THRESHOLD || x.is_nan() {
            if show_newton {
                eprintln!("  [newton] iter={} diverged", iter);
            }
            return None;
        }
    }

    // Check final result
    let result = crate::eval::evaluate_search(lhs, x, eval_context).ok()?;
    let hit = hit_from_sample(x, result.value, result.derivative, rhs_value);
    if hit.real {
        Some(hit)
    } else {
        if show_newton {
            eprintln!(
                "  [newton] failed to converge after {} iterations",
                max_iterations
            );
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::ulp_local_root;

    /// The old cap was `1e-5 * max(|rhs|, 1)`. These gaps sit under that
    /// absolute `1e-5` and over `1e-5` relative to the two sides.
    #[test]
    fn loose_cap_is_relative_to_both_sides() {
        let x = 1.774_999_999_999_999_7e-11;
        let df = 1.0e22;

        let exp_m8 = 3.3546e-4_f64;
        let exp_m8_lhs = 3.2565e-4_f64;
        assert!(
            (exp_m8 - exp_m8_lhs).abs() < 1e-5,
            "the old absolute cap would have accepted exp(-8)"
        );
        assert!(
            !ulp_local_root(exp_m8_lhs, df, exp_m8, x),
            "sinpi(1/x) = exp(-8) is a 3% miss"
        );

        let ninth = 1.0 / 9.0;
        assert!(!ulp_local_root(ninth * (1.0 - 6.8e-5), df, ninth, x));
        let sixth = 1.0 / 6.0;
        assert!(!ulp_local_root(sixth * (1.0 - 4.8e-5), df, sixth, x));

        // 1/e is inside the relative cap (about 4.9e-6). Exactness is separate.
        let inv_e = 1.0 / std::f64::consts::E;
        assert!(ulp_local_root(inv_e * (1.0 - 4.9e-6), df, inv_e, x));

        // Steep phi root, relative residual about 8.6e-7, correction under an ulp.
        let phi = 1.618_033_988_700_895_2;
        let phi_rhs = 142_857.713;
        assert!(ulp_local_root(
            phi_rhs * (1.0 - 8.6e-7),
            1.458e15,
            phi_rhs,
            phi
        ));

        // 1e12 row the re-test wants kept as a non-exact match: relative 8.4e-6.
        let huge = 1.0e12;
        let rhs = 0.998_258_542_623_497;
        assert!(ulp_local_root(rhs * (1.0 + 8.4e-6), 3.136, rhs, huge));
    }
}

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
pub(super) struct NewtonHit {
    pub x: f64,
    /// `|f(x) - rhs|` is small relative to the RHS, or `x` is the closest float
    /// to a smooth root.
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

/// True when `x` is within a few ulps of a smooth root and the residual is
/// still small on the scale of the RHS.
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
    let scale = rhs_value.abs().max(1.0);
    if f.abs() > 1e-5 * scale {
        return false;
    }
    let gap = (x.next_up() - x).abs().max((x - x.next_down()).abs());
    gap > 0.0 && (f / derivative).abs() <= 8.0 * gap
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
            let real = if x == x_before {
                sample_is_root(result.value, df, rhs_value, x)
            } else {
                match crate::eval::evaluate_search(lhs, x, eval_context) {
                    Ok(at) => sample_is_root(at.value, at.derivative, rhs_value, x),
                    Err(_) => false,
                }
            };
            return Some(NewtonHit { x, real });
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
    if sample_is_root(result.value, result.derivative, rhs_value, x) {
        Some(NewtonHit { x, real: true })
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

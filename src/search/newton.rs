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

/// Whether a refined root that landed on the target float really solves the equation.
///
/// The step test `|delta| < 1e-15 * (1 + |x|)` is wider than one ulp for large
/// `|x|`, so Newton can return the original target for `e^sinpi(x) = 2` at
/// `2^52` with a residual of 1. A root that moved off the target is unchanged.
/// A root that stayed on the target is kept only when `|f(x) - rhs|` is small
/// relative to the RHS.
pub(super) fn refined_match_is_real(
    lhs: &crate::expr::Expression,
    rhs_value: f64,
    refined_x: f64,
    target: f64,
    eval_context: &EvalContext<'_>,
) -> bool {
    if refined_x != target {
        return true;
    }
    crate::eval::evaluate_search(lhs, refined_x, eval_context)
        .is_ok_and(|result| newton_residual_acceptable(result.value, rhs_value))
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
) -> Option<f64> {
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

        let delta = f / df;
        x -= delta;

        if show_newton {
            eprintln!("  [newton] iter={} x={:.10} dx={:.10e}", iter, x, delta);
        }

        if delta.abs() < tolerance * (1.0 + x.abs()) {
            return Some(x);
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
    if newton_residual_acceptable(result.value, rhs_value) {
        Some(x)
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

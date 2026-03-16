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
        let result = crate::eval::evaluate_fast_with_context(lhs, x, eval_context).ok()?;
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
    let result = crate::eval::evaluate_fast_with_context(lhs, x, eval_context).ok()?;
    if (result.value - rhs_value).abs() < NEWTON_FINAL_TOLERANCE {
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

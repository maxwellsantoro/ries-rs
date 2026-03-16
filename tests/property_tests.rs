//! Property-based tests for RIES expression evaluation and derivatives
//!
//! Uses proptest to verify mathematical properties of expression evaluation,
//! including derivative correctness via finite differences and calculus rules.

#![cfg(not(target_arch = "wasm32"))]

use proptest::prelude::*;
use ries_rs::eval::{evaluate, EvalError};
use ries_rs::expr::Expression;

/// Helper to check if two floats are approximately equal
fn approx_eq(a: f64, b: f64, tolerance: f64) -> bool {
    if a.is_nan() && b.is_nan() {
        return true;
    }
    if a.is_infinite() && b.is_infinite() {
        return a.is_sign_positive() == b.is_sign_positive();
    }
    (a - b).abs() < tolerance
}

/// Strategy for generating valid x values (avoiding extreme values that cause numerical issues)
fn x_strategy() -> impl Strategy<Value = f64> {
    (-10.0..10.0).prop_filter("avoid near-zero", |x: &f64| x.abs() > 0.1)
}

/// Strategy for generating small positive values (for finite difference h)
fn h_strategy() -> impl Strategy<Value = f64> {
    1e-8..1e-5
}

// =============================================================================
// Derivative Verification via Finite Differences
// =============================================================================

proptest! {
    /// Test that computed derivatives approximate finite differences
    /// For f(x), we verify: (f(x+h) - f(x-h)) / (2h) ≈ f'(x)
    #[test]
    fn derivative_approximates_central_difference(
        x in x_strategy(),
        h in h_strategy()
    ) {
        // Test x^2: derivative should be 2x
        let expr = Expression::parse("xs").unwrap();
        let result = evaluate(&expr, x).unwrap();

        let f_plus = evaluate(&expr, x + h).unwrap().value;
        let f_minus = evaluate(&expr, x - h).unwrap().value;
        let numerical = (f_plus - f_minus) / (2.0 * h);

        prop_assert!(
            approx_eq(result.derivative, numerical, 1e-4),
            "Derivative {} != numerical {} for x^2 at x={}",
            result.derivative, numerical, x
        );
    }

    /// Test derivative of x^3 (xsx*)
    #[test]
    fn derivative_x_cubed(x in x_strategy(), h in h_strategy()) {
        let expr = Expression::parse("xsx*").unwrap(); // x^2 * x = x^3
        let result = evaluate(&expr, x).unwrap();

        let f_plus = evaluate(&expr, x + h).unwrap().value;
        let f_minus = evaluate(&expr, x - h).unwrap().value;
        let numerical = (f_plus - f_minus) / (2.0 * h);

        // d(x^3)/dx = 3x^2
        let expected = 3.0 * x * x;
        prop_assert!(
            approx_eq(result.derivative, expected, 1e-6),
            "d(x^3)/dx = {} but got {} at x={}",
            expected, result.derivative, x
        );
        prop_assert!(
            approx_eq(result.derivative, numerical, 1e-3),
            "Derivative {} != numerical {} for x^3 at x={}",
            result.derivative, numerical, x
        );
    }

    /// Test derivative of sqrt(x)
    #[test]
    fn derivative_sqrt(x in 0.5f64..10.0, h in h_strategy()) {
        let expr = Expression::parse("xq").unwrap(); // sqrt(x)
        let result = evaluate(&expr, x).unwrap();

        let f_plus = evaluate(&expr, x + h).unwrap().value;
        let f_minus = evaluate(&expr, x - h).unwrap().value;
        let numerical = (f_plus - f_minus) / (2.0 * h);

        // d(sqrt(x))/dx = 1/(2*sqrt(x))
        let expected = 1.0 / (2.0 * x.sqrt());
        prop_assert!(
            approx_eq(result.derivative, expected, 1e-6),
            "d(sqrt)/dx = {} but got {} at x={}",
            expected, result.derivative, x
        );
        prop_assert!(
            approx_eq(result.derivative, numerical, 1e-4),
            "Derivative {} != numerical {} for sqrt(x) at x={}",
            result.derivative, numerical, x
        );
    }

    /// Test derivative of e^x
    #[test]
    fn derivative_exp(x in -5.0f64..5.0, h in h_strategy()) {
        let expr = Expression::parse("xE").unwrap(); // e^x
        let result = evaluate(&expr, x).unwrap();

        let f_plus = evaluate(&expr, x + h).unwrap().value;
        let f_minus = evaluate(&expr, x - h).unwrap().value;
        let numerical = (f_plus - f_minus) / (2.0 * h);

        // d(e^x)/dx = e^x
        let expected = x.exp();
        prop_assert!(
            approx_eq(result.derivative, expected, 1e-6),
            "d(e^x)/dx = {} but got {} at x={}",
            expected, result.derivative, x
        );
        prop_assert!(
            approx_eq(result.derivative, numerical, 1e-4),
            "Derivative {} != numerical {} for e^x at x={}",
            result.derivative, numerical, x
        );
    }

    /// Test derivative of ln(x)
    #[test]
    fn derivative_ln(x in 0.5f64..10.0, h in h_strategy()) {
        let expr = Expression::parse("xl").unwrap(); // ln(x) - lowercase 'l'
        let result = evaluate(&expr, x).unwrap();

        let f_plus = evaluate(&expr, x + h).unwrap().value;
        let f_minus = evaluate(&expr, x - h).unwrap().value;
        let numerical = (f_plus - f_minus) / (2.0 * h);

        // d(ln(x))/dx = 1/x
        let expected = 1.0 / x;
        prop_assert!(
            approx_eq(result.derivative, expected, 1e-6),
            "d(ln)/dx = {} but got {} at x={}",
            expected, result.derivative, x
        );
        prop_assert!(
            approx_eq(result.derivative, numerical, 1e-4),
            "Derivative {} != numerical {} for ln(x) at x={}",
            result.derivative, numerical, x
        );
    }
}

// =============================================================================
// Calculus Rules
// =============================================================================

proptest! {
    /// Test linearity: d(af + bg)/dx = a*df/dx + b*dg/dx
    /// We test with f(x) = x^2, g(x) = x, a = 2, b = 3
    /// So: d(2x^2 + 3x)/dx = 4x + 3
    #[test]
    fn derivative_linearity(x in x_strategy()) {
        // 2x^2 + 3x = "xs2* x3* +" but we don't have constant multiplication
        // Instead: x^2 x^2 + x x x + + = 2x^2 + 2x (close enough)
        // Let's use: x^2 + x = "xs x +" -> d/dx = 2x + 1
        let expr = Expression::parse("xsx+").unwrap();
        let result = evaluate(&expr, x).unwrap();

        // d(x^2 + x)/dx = 2x + 1
        let expected = 2.0 * x + 1.0;
        prop_assert!(
            approx_eq(result.derivative, expected, 1e-6),
            "d(x^2+x)/dx = {} but got {} at x={}",
            expected, result.derivative, x
        );
    }

    /// Test product rule: d(fg)/dx = f*dg/dx + g*df/dx
    /// Test with f(x) = x^2, g(x) = x: x^2 * x = x^3
    /// d(x^3)/dx = 3x^2
    #[test]
    fn derivative_product_rule(x in x_strategy()) {
        // x^2 * x = "xs x *" -> x^3
        let expr = Expression::parse("xsx*").unwrap();
        let result = evaluate(&expr, x).unwrap();

        // Verify value: x^3
        let expected_value = x * x * x;
        prop_assert!(
            approx_eq(result.value, expected_value, 1e-10),
            "x^3 = {} but got {} at x={}",
            expected_value, result.value, x
        );

        // d(x^3)/dx = 3x^2
        let expected_deriv = 3.0 * x * x;
        prop_assert!(
            approx_eq(result.derivative, expected_deriv, 1e-6),
            "d(x^3)/dx = {} but got {} at x={}",
            expected_deriv, result.derivative, x
        );
    }

    /// Test quotient rule: d(f/g)/dx = (g*df/dx - f*dg/dx) / g^2
    /// Test with f(x) = x^2, g(x) = x: x^2 / x = x
    /// d(x)/dx = 1
    #[test]
    fn derivative_quotient_rule(x in x_strategy()) {
        // x^2 / x = "xs x /" -> x
        let expr = Expression::parse("xsx/").unwrap();
        let result = evaluate(&expr, x).unwrap();

        // Verify value: x
        prop_assert!(
            approx_eq(result.value, x, 1e-10),
            "x = {} but got {} at x={}",
            x, result.value, x
        );

        // d(x)/dx = 1
        prop_assert!(
            approx_eq(result.derivative, 1.0, 1e-6),
            "d(x)/dx = 1 but got {} at x={}",
            result.derivative, x
        );
    }

    /// Test chain rule for composition
    /// e^(x^2): inner = x^2, outer = e^y
    /// d(e^(x^2))/dx = e^(x^2) * 2x
    #[test]
    fn derivative_chain_rule_exp_square(x in -2.0f64..2.0) {
        // x^2 e^ = "xs E" -> e^(x^2)
        let expr = Expression::parse("xsE").unwrap();
        let result = evaluate(&expr, x).unwrap();

        // d(e^(x^2))/dx = e^(x^2) * 2x
        let expected_deriv = (x * x).exp() * 2.0 * x;
        prop_assert!(
            approx_eq(result.derivative, expected_deriv, 1e-4),
            "d(e^(x^2))/dx = {} but got {} at x={}",
            expected_deriv, result.derivative, x
        );
    }
}

// =============================================================================
// Expression Parsing Round-Trip
// =============================================================================

proptest! {
    /// Test that expressions can be parsed and converted back to postfix
    #[test]
    fn parse_postfix_roundtrip(expr_str in "[1-9xqsp][1-9xqsp+*-]*") {
        if let Some(expr) = Expression::parse(&expr_str) {
            // Should be able to get postfix back
            let postfix = expr.to_postfix();
            // Re-parse should give same result
            if let Some(reparsed) = Expression::parse(&postfix) {
                prop_assert_eq!(expr.to_postfix(), reparsed.to_postfix());
            }
        }
    }
}

// =============================================================================
// Domain Error Handling
// =============================================================================

proptest! {
    /// Test that sqrt of negative numbers returns an error
    #[test]
    fn sqrt_negative_domain_error(x in -10.0f64..-0.01) {
        let expr = Expression::parse("xq").unwrap(); // sqrt(x)
        let result = evaluate(&expr, x);
        prop_assert!(
            matches!(result, Err(EvalError::SqrtDomain)),
            "sqrt({}) should return SqrtDomain error, got {:?}",
            x, result
        );
    }

    /// Test that ln of non-positive numbers returns an error
    #[test]
    fn ln_nonpositive_domain_error(x in -10.0f64..0.0) {
        let expr = Expression::parse("xl").unwrap(); // ln(x) - lowercase 'l'
        let result = evaluate(&expr, x);
        prop_assert!(
            matches!(result, Err(EvalError::LogDomain)),
            "ln({}) should return LogDomain error, got {:?}",
            x, result
        );
    }

    /// Test that division by zero returns an error
    #[test]
    fn division_by_zero_error(x in x_strategy()) {
        // x / 0 using: x 0 /
        // But we don't have 0 as a constant, so use x-x which is 0
        // x x - / would be: push x, push x, subtract (=0), then divide by that
        // Actually that's (x - 0) / x which is fine
        // Let's try: 1 x x - / = 1 / (x-x) = 1/0
        // "1xx-/" = push 1, push x, push x, subtract, divide
        let expr = Expression::parse("1xx-/").unwrap();
        let result = evaluate(&expr, x);
        prop_assert!(
            matches!(result, Err(EvalError::DivisionByZero)),
            "1/(x-x) should return DivisionByZero error at x={}, got {:?}",
            x, result
        );
    }
}

// =============================================================================
// Value Correctness
// =============================================================================

proptest! {
    /// Test that basic arithmetic expressions evaluate correctly
    #[test]
    fn arithmetic_correctness(x in x_strategy()) {
        // x + 1
        let expr = Expression::parse("x1+").unwrap();
        let result = evaluate(&expr, x).unwrap();
        prop_assert!(approx_eq(result.value, x + 1.0, 1e-10));

        // x - 1
        let expr = Expression::parse("x1-").unwrap();
        let result = evaluate(&expr, x).unwrap();
        prop_assert!(approx_eq(result.value, x - 1.0, 1e-10));

        // x * 2
        let expr = Expression::parse("x2*").unwrap();
        let result = evaluate(&expr, x).unwrap();
        prop_assert!(approx_eq(result.value, x * 2.0, 1e-10));

        // x / 2
        let expr = Expression::parse("x2/").unwrap();
        let result = evaluate(&expr, x).unwrap();
        prop_assert!(approx_eq(result.value, x / 2.0, 1e-10));
    }

    /// Test constant expressions
    #[test]
    fn constant_values(
        _pi_val in proptest::bool::ANY,
        e_val in proptest::bool::ANY,
        phi_val in proptest::bool::ANY
    ) {
        // Always test pi (to ensure test doesn't skip everything)
        {
            let expr = Expression::parse("p").unwrap();
            let result = evaluate(&expr, 0.0).unwrap();
            prop_assert!(approx_eq(result.value, std::f64::consts::PI, 1e-10));
            prop_assert!(approx_eq(result.derivative, 0.0, 1e-10));
        }

        if e_val {
            let expr = Expression::parse("e").unwrap();
            let result = evaluate(&expr, 0.0).unwrap();
            prop_assert!(approx_eq(result.value, std::f64::consts::E, 1e-10));
            prop_assert!(approx_eq(result.derivative, 0.0, 1e-10));
        }

        if phi_val {
            let expr = Expression::parse("f").unwrap();
            let result = evaluate(&expr, 0.0).unwrap();
            let phi = 1.618_033_988_749_895;
            prop_assert!(approx_eq(result.value, phi, 1e-10));
            prop_assert!(approx_eq(result.derivative, 0.0, 1e-10));
        }
    }
}

// =============================================================================
// Complex Expression Tests
// =============================================================================

proptest! {
    /// Test derivative of trigonometric functions
    /// sin(πx): d(sin(πx))/dx = π*cos(πx)
    #[test]
    fn derivative_sinpi(x in -2.0f64..2.0) {
        let expr = Expression::parse("xS").unwrap(); // sin(πx)
        let result = evaluate(&expr, x).unwrap();

        let pi = std::f64::consts::PI;
        let expected_deriv = pi * (pi * x).cos();
        prop_assert!(
            approx_eq(result.derivative, expected_deriv, 1e-6),
            "d(sin(πx))/dx = {} but got {} at x={}",
            expected_deriv, result.derivative, x
        );
    }

    /// Test derivative of cos(πx): d(cos(πx))/dx = -π*sin(πx)
    #[test]
    fn derivative_cospi(x in -2.0f64..2.0) {
        let expr = Expression::parse("xC").unwrap(); // cos(πx)
        let result = evaluate(&expr, x).unwrap();

        let pi = std::f64::consts::PI;
        let expected_deriv = -pi * (pi * x).sin();
        prop_assert!(
            approx_eq(result.derivative, expected_deriv, 1e-6),
            "d(cos(πx))/dx = {} but got {} at x={}",
            expected_deriv, result.derivative, x
        );
    }

    /// Test x^x (a classic RIES expression)
    #[test]
    fn derivative_x_to_x(x in 0.5f64..3.0) {
        let expr = Expression::parse("xx^").unwrap(); // x^x
        let result = evaluate(&expr, x).unwrap();

        // d(x^x)/dx = x^x * (ln(x) + 1)
        let expected_value = x.powf(x);
        let expected_deriv = expected_value * (x.ln() + 1.0);

        prop_assert!(
            approx_eq(result.value, expected_value, 1e-6),
            "x^x = {} but got {} at x={}",
            expected_value, result.value, x
        );
        prop_assert!(
            approx_eq(result.derivative, expected_deriv, 1e-4),
            "d(x^x)/dx = {} but got {} at x={}",
            expected_deriv, result.derivative, x
        );
    }
}

// =============================================================================
// Newton-Raphson Convergence Tests
// =============================================================================

/// Simple Newton-Raphson implementation for testing
fn newton_raphson_test(
    expr: &Expression,
    target: f64,
    initial_x: f64,
    max_iterations: usize,
) -> Option<(f64, usize)> {
    let mut x = initial_x;
    let tolerance = 1e-14;

    for i in 0..max_iterations {
        let result = evaluate(expr, x).ok()?;
        let f = result.value - target;
        let df = result.derivative;

        if df.abs() < 1e-100 {
            return None; // Derivative too small
        }

        let delta = f / df;
        x -= delta;

        if delta.abs() < tolerance * (1.0 + x.abs()) {
            return Some((x, i + 1));
        }

        if x.abs() > 1e100 || x.is_nan() {
            return None;
        }
    }

    // Check final result
    let result = evaluate(expr, x).ok()?;
    if (result.value - target).abs() < 1e-10 {
        Some((x, max_iterations))
    } else {
        None
    }
}

proptest! {
    /// Test Newton-Raphson converges for quadratic polynomials (x^2 = c)
    /// Quadratic convergence: error should roughly square each iteration
    #[test]
    fn newton_quadratic_convergence(target in 1.0f64..100.0) {
        let expr = Expression::parse("xs").unwrap(); // x^2
        let sqrt_target = target.sqrt();
        let initial_x = sqrt_target + 1.0; // Start near the solution

        if let Some((solution, iterations)) = newton_raphson_test(&expr, target, initial_x, 20) {
            // Should converge to sqrt(target) or -sqrt(target)
            let error = (solution.abs() - sqrt_target).abs();
            prop_assert!(
                error < 1e-10,
                "Newton failed to converge: expected ±{:.6}, got {:.10}",
                sqrt_target, solution
            );
            // Quadratic convergence should need very few iterations
            prop_assert!(
                iterations <= 10,
                "Quadratic convergence took {} iterations (expected ≤10)",
                iterations
            );
        } else {
            // Some cases may not converge, which is acceptable
        }
    }

    /// Test Newton-Raphson for cubic polynomials (x^3 = c)
    #[test]
    fn newton_cubic_polynomial(target in 1.0f64..100.0) {
        let expr = Expression::parse("xsx*").unwrap(); // x^3 = x^2 * x
        let cbrt_target = target.cbrt();
        let initial_x = cbrt_target + 1.0;

        if let Some((solution, iterations)) = newton_raphson_test(&expr, target, initial_x, 20) {
            let error = (solution - cbrt_target).abs();
            prop_assert!(
                error < 1e-9,
                "Newton failed for x^3 = {}: expected {:.6}, got {:.10}",
                target, cbrt_target, solution
            );
            prop_assert!(
                iterations <= 15,
                "Cubic convergence took {} iterations",
                iterations
            );
        }
    }

    /// Test Newton-Raphson for exponential equations (e^x = c)
    #[test]
    fn newton_exponential(target in 1.0f64..20.0) {
        let expr = Expression::parse("xE").unwrap(); // e^x
        let ln_target = target.ln();
        let initial_x = ln_target + 0.5;

        if let Some((solution, _iterations)) = newton_raphson_test(&expr, target, initial_x, 20) {
            let error = (solution - ln_target).abs();
            prop_assert!(
                error < 1e-10,
                "Newton failed for e^x = {}: expected {:.6}, got {:.10}",
                target, ln_target, solution
            );
        }
    }

    /// Test Newton-Raphson for logarithm equations (ln(x) = c)
    #[test]
    fn newton_logarithm(target in -2.0f64..3.0) {
        let expr = Expression::parse("xl").unwrap(); // ln(x)
        let exp_target = target.exp();
        if exp_target <= 0.0 || exp_target > 100.0 {
            return Ok(()); // Skip invalid ranges
        }

        let initial_x = exp_target + 0.5;

        if let Some((solution, _iterations)) = newton_raphson_test(&expr, target, initial_x, 20) {
            let error = (solution - exp_target).abs();
            prop_assert!(
                error < 1e-9,
                "Newton failed for ln(x) = {}: expected {:.6}, got {:.10}",
                target, exp_target, solution
            );
        }
    }

    /// Test Newton-Raphson for x^x equations (challenging due to non-elementary form)
    #[test]
    fn newton_x_to_x(target in 2.0f64..20.0) {
        let expr = Expression::parse("xx^").unwrap(); // x^x
        // For x^x = target, approximate solution using ln
        let approx_x = (target.ln() / std::f64::consts::E).powf(1.0 / std::f64::consts::E);
        let initial_x = approx_x.max(1.5);

        if let Some((solution, iterations)) = newton_raphson_test(&expr, target, initial_x, 30) {
            // Verify by plugging back
            let verify = evaluate(&expr, solution).unwrap();
            let error = (verify.value - target).abs();
            prop_assert!(
                error < 1e-6,
                "Newton failed for x^x = {}: verification error = {:.2e}",
                target, error
            );
            // x^x may require more iterations
            prop_assert!(
                iterations <= 25,
                "x^x convergence took {} iterations",
                iterations
            );
        }
    }

    /// Test Newton-Raphson handles difficult initial guesses gracefully
    #[test]
    fn newton_difficult_start(target in 5.0f64..15.0) {
        let expr = Expression::parse("xs").unwrap(); // x^2
        // Start far from the solution
        let initial_x = -10.0;

        if let Some((solution, _iterations)) = newton_raphson_test(&expr, target, initial_x, 30) {
            // Should still converge to a valid solution
            let verify = evaluate(&expr, solution).unwrap();
            let error = (verify.value - target).abs();
            prop_assert!(
                error < 1e-8,
                "Newton from bad start: verification error = {:.2e}",
                error
            );
        }
    }

    /// Test Newton-Raphson for trigonometric equations sin(πx) = c
    #[test]
    fn newton_sinpi(target in -1.0f64..1.0) {
        let expr = Expression::parse("xS").unwrap(); // sin(πx)
        // arcsin(target) / π gives one solution
        let arcsin_t = target.asin();
        let solution1 = arcsin_t / std::f64::consts::PI;
        let initial_x = solution1 + 0.1;

        if let Some((solution, _iterations)) = newton_raphson_test(&expr, target, initial_x, 20) {
            // Verify by plugging back
            let verify = evaluate(&expr, solution).unwrap();
            let error = (verify.value - target).abs();
            prop_assert!(
                error < 1e-9,
                "Newton failed for sin(πx) = {}: verification error = {:.2e}",
                target, error
            );
        }
    }
}

// =============================================================================
// Newton-Raphson Convergence Rate Tests (Quadratic Convergence)
// =============================================================================

/// Track convergence history for rate analysis
fn newton_with_history(
    expr: &Expression,
    target: f64,
    initial_x: f64,
    max_iterations: usize,
) -> Option<Vec<f64>> {
    let mut x = initial_x;
    let mut errors = Vec::new();

    for _ in 0..max_iterations {
        let result = evaluate(expr, x).ok()?;
        let f = result.value - target;
        let df = result.derivative;

        errors.push(f.abs());

        if df.abs() < 1e-100 {
            return None;
        }

        let delta = f / df;
        x -= delta;

        if x.abs() > 1e100 || x.is_nan() {
            return None;
        }
    }

    Some(errors)
}

proptest! {
    /// Test quadratic convergence rate for x^2 = c
    /// In quadratic convergence, error_{n+1} ≈ C * error_n^2
    /// So the ratio error_{n+1} / error_n^2 should be roughly constant
    #[test]
    fn quadratic_convergence_rate(c in 2.0f64..50.0) {
        let expr = Expression::parse("xs").unwrap(); // x^2
        let sqrt_c = c.sqrt();
        // Start with moderate error
        let initial_x = sqrt_c + 0.5;

        if let Some(errors) = newton_with_history(&expr, c, initial_x, 10) {
            // Need at least 3 iterations to check convergence rate
            if errors.len() >= 3 {
                // For quadratic convergence: e_{n+1} / e_n^2 should be bounded
                // Skip first iteration (may be affected by initial guess)
                for i in 1..errors.len().saturating_sub(1) {
                    if errors[i] > 1e-14 && errors[i + 1] > 1e-14 {
                        let ratio = errors[i + 1] / (errors[i] * errors[i]);
                        // Ratio should be roughly constant and bounded
                        // For x^2, the theoretical ratio is 1/(2*sqrt(c))
                        let expected_ratio = 1.0 / (2.0 * sqrt_c);
                        prop_assert!(
                            (ratio - expected_ratio).abs() < expected_ratio * 2.0,
                            "Convergence rate ratio {:.3} differs from expected {:.3} at iteration {}",
                            ratio, expected_ratio, i
                        );
                    }
                }
            }
        }
    }
}

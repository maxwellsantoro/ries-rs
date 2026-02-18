//! Tests for expression evaluation and automatic differentiation

#![allow(clippy::field_reassign_with_default)]

mod common;
use common::approx_eq_default;

use ries_rs::eval::{evaluate, evaluate_with_constants, EvalError};
use ries_rs::expr::Expression;
use ries_rs::profile::UserConstant;

#[test]
fn test_basic_evaluation() {
    let expr = Expression::parse("32+").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    assert!(approx_eq_default(result.value, 5.0));
    assert!(approx_eq_default(result.derivative, 0.0));
}

#[test]
fn test_variable_evaluation() {
    let expr = Expression::parse("x").unwrap();
    let result = evaluate(&expr, 3.5).unwrap();
    assert!(approx_eq_default(result.value, 3.5));
    assert!(approx_eq_default(result.derivative, 1.0));
}

#[test]
fn test_x_squared() {
    let expr = Expression::parse("xs").unwrap();
    let result = evaluate(&expr, 3.0).unwrap();
    assert!(approx_eq_default(result.value, 9.0));
    assert!(approx_eq_default(result.derivative, 6.0)); // 2x
}

#[test]
fn test_sqrt_pi() {
    let expr = Expression::parse("pq").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    assert!(approx_eq_default(result.value, std::f64::consts::PI.sqrt()));
}

#[test]
fn test_exponential() {
    let expr = Expression::parse("xE").unwrap();
    let result = evaluate(&expr, 1.0).unwrap();
    assert!(approx_eq_default(result.value, std::f64::consts::E));
    assert!(approx_eq_default(result.derivative, std::f64::consts::E));
}

#[test]
fn test_complex_expression() {
    // x^2 + 2*x + 1 = (x+1)^2
    let expr = Expression::parse("xs2x*+1+").unwrap();
    let result = evaluate(&expr, 3.0).unwrap();
    assert!(approx_eq_default(result.value, 16.0)); // (3+1)^2
    assert!(approx_eq_default(result.derivative, 8.0)); // 2x + 2 = 8
}

#[test]
fn test_division_by_zero() {
    // 1 / x when x = 0
    let expr = Expression::parse("1x/").unwrap();
    let result = evaluate(&expr, 0.0);
    assert!(matches!(result, Err(EvalError::DivisionByZero)));
}

#[test]
fn test_sqrt_negative() {
    // sqrt(-1) = sqrt(negate(1))
    let expr = Expression::parse("1nq").unwrap();
    let result = evaluate(&expr, 0.0);
    assert!(matches!(result, Err(EvalError::SqrtDomain)));
}

#[test]
fn test_log_domain() {
    // ln(-1) = ln(negate(1))
    let expr = Expression::parse("1nl").unwrap();
    let result = evaluate(&expr, 0.0);
    assert!(matches!(result, Err(EvalError::LogDomain)));
}

#[test]
fn test_user_constant() {
    // Create a simple expression to test user constant evaluation
    // Since from_symbols is only available in cfg(test), we test with
    // an empty user constants list and a regular expression
    let user_constants: Vec<UserConstant> = vec![];

    // Test that evaluate_with_constants works with empty constants
    let expr = Expression::parse("32+").unwrap();
    let result = evaluate_with_constants(&expr, 0.0, &user_constants).unwrap();
    assert!(approx_eq_default(result.value, 5.0));
}

#[test]
fn test_lambert_w() {
    // W(1) ≈ 0.5671432904
    let expr = Expression::parse("1W").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    assert!((result.value - 0.5671432904).abs() < 1e-9);

    // W(e) = 1
    let expr = Expression::parse("eW").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    assert!(approx_eq_default(result.value, 1.0));
}

// ============================================================================
// Root domain tests - regression tests for P1
// ============================================================================

#[test]
fn test_root_odd_of_negative() {
    // Cube root of -8 = -2
    // Expression: 38nv = push 3, push -8 (8n), apply root
    // Root computes a-th root of b, so we want root(3, -8) = cube root of -8
    let expr = Expression::parse("38nv").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    assert!(approx_eq_default(result.value, -2.0));
}

#[test]
fn test_root_even_of_negative() {
    // Square root of -8 should fail
    // Expression: 28nv = push 2, push -8, apply root
    let expr = Expression::parse("28nv").unwrap();
    let result = evaluate(&expr, 0.0);
    assert!(matches!(result, Err(EvalError::SqrtDomain)));
}

#[test]
fn test_root_non_integer_index_of_negative() {
    // Non-integer indices of negative radicands have no real value

    // Test with index = e (transcendental, definitely non-integer)
    // Expression: e8nv = push e, push -8, apply root
    let expr = Expression::parse("e8nv").unwrap();
    let result = evaluate(&expr, 0.0);
    assert!(matches!(result, Err(EvalError::SqrtDomain)));

    // Test with index = pi (transcendental, definitely non-integer)
    // Expression: p8nv = push pi, push -8, apply root
    let expr = Expression::parse("p8nv").unwrap();
    let result = evaluate(&expr, 0.0);
    assert!(matches!(result, Err(EvalError::SqrtDomain)));

    // Test with index = phi (transcendental, definitely non-integer)
    // Expression: f8nv = push phi, push -8, apply root
    let expr = Expression::parse("f8nv").unwrap();
    let result = evaluate(&expr, 0.0);
    assert!(matches!(result, Err(EvalError::SqrtDomain)));
}

#[test]
fn test_root_fifth_of_negative() {
    // Fifth root of -1 = -1
    // Expression: 51nv = push 5, push -1 (1n), apply root
    let expr = Expression::parse("51nv").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    assert!(approx_eq_default(result.value, -1.0));
}

#[test]
fn test_root_positive_radicand_non_integer() {
    // Non-integer root of positive radicand should work fine
    // 8^(1/pi) should compute
    let expr = Expression::parse("p8v").unwrap();
    let result = evaluate(&expr, 0.0).unwrap();
    // 8^(1/pi) - just check it's a valid positive number
    assert!(result.value > 0.0 && result.value.is_finite());
}

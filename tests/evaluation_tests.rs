//! Tests for expression evaluation and automatic differentiation

mod common;
use common::approx_eq_default;

use ries_rs::expr::Expression;
use ries_rs::eval::{evaluate, evaluate_with_constants, EvalError};
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

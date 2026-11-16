//! Tests for expression parsing, conversion, and complexity

#![cfg(not(target_arch = "wasm32"))]
#![allow(clippy::field_reassign_with_default)]

mod common;

use ries_rs::expr::Expression;

#[allow(unused_imports)]
use ries_rs::symbol::Symbol;

#[test]
fn test_parse_basic() {
    let expr = Expression::parse("32+").unwrap();
    assert_eq!(expr.len(), 3);
    assert_eq!(expr.to_postfix(), "32+");
    assert!(!expr.contains_x());
}

#[test]
fn test_parse_with_variable() {
    let expr = Expression::parse("xs").unwrap();
    assert_eq!(expr.len(), 2);
    assert!(expr.contains_x());
}

#[test]
fn test_infix_conversion_basic() {
    assert_eq!(Expression::parse("32+").unwrap().to_infix(), "3+2");
    assert_eq!(Expression::parse("32*").unwrap().to_infix(), "3*2");
    assert_eq!(Expression::parse("xs").unwrap().to_infix(), "x^2");
    assert_eq!(Expression::parse("xq").unwrap().to_infix(), "sqrt(x)");
}

#[test]
fn test_infix_conversion_precedence() {
    assert_eq!(Expression::parse("32+5*").unwrap().to_infix(), "(3+2)*5");
}

#[test]
fn test_infix_conversion_constants() {
    assert_eq!(Expression::parse("pq").unwrap().to_infix(), "sqrt(pi)");
    // "ex*" is e * x in postfix (e, x, multiply)
    assert_eq!(Expression::parse("ex*").unwrap().to_infix(), "e*x");
}

#[test]
fn test_complexity_calculation() {
    let expr = Expression::parse("xs").unwrap(); // x^2
                                                 // x = 15, s (square) = 9
    assert_eq!(expr.complexity(), 15 + 9);
}

#[test]
fn test_expression_validity() {
    // Valid: 3 2 + (pushes 3, pushes 2, adds them -> 1 value)
    assert!(Expression::parse("32+").unwrap().is_valid());

    // Valid: x 2 ^ (x squared)
    assert!(Expression::parse("xs").unwrap().is_valid());

    // Manually-built malformed expressions can still be validated explicitly.
    let mut underflow = Expression::new();
    underflow.push(Symbol::Three);
    underflow.push(Symbol::Add);
    assert!(!underflow.is_valid());

    let mut incomplete = Expression::new();
    incomplete.push(Symbol::Three);
    incomplete.push(Symbol::Two);
    assert!(!incomplete.is_valid());
}

#[test]
fn test_parse_rejects_malformed_postfix() {
    // Invalid: 3 + (not enough operands)
    assert!(Expression::parse("3+").is_none());

    // Invalid: 3 2 (two values left on stack)
    assert!(Expression::parse("32").is_none());
}

#[test]
fn test_output_formats() {
    use ries_rs::expr::OutputFormat;

    let expr = Expression::parse("pq").unwrap(); // sqrt(pi)

    assert_eq!(expr.to_infix_with_format(OutputFormat::Default), "sqrt(pi)");
    assert!(expr
        .to_infix_with_format(OutputFormat::Pretty)
        .contains("π"));
    assert!(expr
        .to_infix_with_format(OutputFormat::Mathematica)
        .contains("Pi"));
}

#[test]
fn test_user_function_infix_does_not_panic() {
    let expr = Expression::parse("xH").unwrap(); // H maps to UserFunction0
    assert_eq!(expr.to_infix(), "f0(x)");
}

#[test]
fn test_mathematica_format_has_balanced_brackets() {
    use ries_rs::expr::OutputFormat;

    let expr = Expression::parse("pq").unwrap(); // sqrt(pi)
    let formatted = expr.to_infix_with_format(OutputFormat::Mathematica);

    let open = formatted.chars().filter(|c| *c == '[').count();
    let close = formatted.chars().filter(|c| *c == ']').count();
    assert_eq!(
        open, close,
        "Unbalanced Mathematica brackets: {}",
        formatted
    );
}

/// Regression: to_infix must never silently produce '?' for any valid parsed expression.
/// (Invalid expressions use a #[should_panic] unit test inside src/expr.rs where
///  from_symbols is accessible.)
#[test]
fn test_to_infix_no_question_marks_for_valid_expressions() {
    let cases = [
        ("x", "x"),
        ("32+", "3+2"),
        ("xs", "x^2"),
        ("p", "π"),
        ("2x*", "2*x"),
        ("3pq*", "3*sqrt(π)"),
    ];
    for (postfix, _expected_infix) in cases {
        let expr = Expression::parse(postfix).unwrap();
        let infix = expr.to_infix();
        assert!(
            !infix.contains('?'),
            "to_infix produced '?' for valid expression '{postfix}': got '{infix}'"
        );
    }
}

#[test]
fn cas_exports_preserve_real_roots_and_fractional_positive_indices() {
    for (postfix, sympy, mathematica) in [
        (
            "38nv",
            "real_root(-8, 3)",
            "Piecewise[{{Surd[-8, 3], -8<0}}, (-8)^(1/3)]",
        ),
        (
            "3n8nv",
            "real_root(-8, -3)",
            "Piecewise[{{Surd[-8, -3], -8<0}}, (-8)^(1/(-3))]",
        ),
        (
            "12+8nv",
            "real_root(-8, 1+2)",
            "Piecewise[{{Surd[-8, 1+2], -8<0}}, (-8)^(1/(1+2))]",
        ),
        (
            "12/4v",
            "real_root(4, 1/2)",
            "Piecewise[{{Surd[4, 1/2], 4<0}}, 4^(1/(1/2))]",
        ),
        (
            "3xv2*",
            "real_root(x, 3)*2",
            "Piecewise[{{Surd[x, 3], x<0}}, x^(1/3)]*2",
        ),
    ] {
        let expr = Expression::parse(postfix).unwrap();
        assert_eq!(expr.to_infix_sympy(), sympy, "{postfix}");
        assert_eq!(expr.to_infix_mathematica(), mathematica, "{postfix}");
    }
    // These are the real values the exported expressions must preserve.
    for (postfix, expected) in [("38nv", -2.0), ("3n8nv", -0.5), ("12/4v", 16.0)] {
        let result = ries_rs::eval::evaluate(&Expression::parse(postfix).unwrap(), 0.0).unwrap();
        assert!((result.value - expected).abs() < 1e-14);
    }
}

#[test]
fn sympy_exports_the_public_lambert_w_function() {
    for (postfix, expected) in [("1W", "LambertW(1)"), ("x1+W", "LambertW(x+1)")] {
        assert_eq!(
            Expression::parse(postfix).unwrap().to_infix_sympy(),
            expected
        );
    }
}

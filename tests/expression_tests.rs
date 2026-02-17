//! Tests for expression parsing, conversion, and complexity

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
                                                 // x = 6, s (square) = 5
    assert_eq!(expr.complexity(), 6 + 5);
}

#[test]
fn test_expression_validity() {
    // Valid: 3 2 + (pushes 3, pushes 2, adds them -> 1 value)
    assert!(Expression::parse("32+").unwrap().is_valid());

    // Valid: x 2 ^ (x squared)
    assert!(Expression::parse("xs").unwrap().is_valid());

    // Invalid: 3 + (not enough operands)
    assert!(!Expression::parse("3+").unwrap().is_valid());

    // Invalid: 3 2 (two values left on stack)
    assert!(!Expression::parse("32").unwrap().is_valid());
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

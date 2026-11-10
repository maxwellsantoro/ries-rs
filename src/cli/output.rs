//! Output formatting helpers for RIES
//!
//! This module provides functions for formatting expressions, matches,
//! and other output for display to the user.

pub use ries_rs::ReportDisplayFormat as DisplayFormat;
use ries_rs::{expr, search, symbol, EvalContext, SymbolTable};

/// Format a numeric value for display.
///
/// Uses scientific notation for very large or very small values.
pub fn format_value(v: f64) -> String {
    if v.abs() >= 1e6 || (v.abs() < 1e-4 && v != 0.0) {
        format!("{:.10e}", v)
    } else {
        format!("{:.10}", v)
    }
}

/// Parse output format from a string argument.
///
/// Supports numeric codes (0, 1, 3) and named formats (pretty, mathematica, sympy).
pub fn parse_display_format(s: &str) -> DisplayFormat {
    match s.to_lowercase().as_str() {
        "0" => DisplayFormat::PostfixCompact,
        "1" => DisplayFormat::Condensed, // alias for PostfixCompact
        "3" => DisplayFormat::PostfixVerbose,
        "pretty" | "unicode" => DisplayFormat::Infix(expr::OutputFormat::Pretty),
        "mathematica" | "math" | "mma" => DisplayFormat::Infix(expr::OutputFormat::Mathematica),
        "sympy" | "python" => DisplayFormat::Infix(expr::OutputFormat::SymPy),
        _ => DisplayFormat::Infix(expr::OutputFormat::Default),
    }
}

/// Print a match in relative format (showing error from target).
pub fn print_match_relative(
    m: &search::Match,
    _solve: bool,
    format: DisplayFormat,
    explicit_multiply: bool,
    solved_rhs: Option<&expr::Expression>,
    table: Option<&SymbolTable>,
    trig_scale: f64,
) {
    let lhs_expr = if solved_rhs.is_some() {
        let mut x_expr = expr::Expression::new();
        x_expr.push(symbol::Symbol::X);
        x_expr
    } else {
        m.lhs.expr.clone()
    };
    let rhs_expr = solved_rhs.unwrap_or(&m.rhs.expr);

    let lhs_str = format.format_expression(&lhs_expr, explicit_multiply, table, trig_scale);
    let rhs_str = format.format_expression(rhs_expr, explicit_multiply, table, trig_scale);

    let error_str = if m.exact {
        "('exact' match)".to_string()
    } else {
        let sign = if m.error >= 0.0 { "+" } else { "-" };
        format!("for x = T {} {:.6e}", sign, m.error.abs())
    };

    println!(
        "{:>24} = {:<24} {} {{{}}}",
        lhs_str, rhs_str, error_str, m.complexity
    );
}

/// Print a match in absolute format (showing x value).
pub fn print_match_absolute(
    m: &search::Match,
    _solve: bool,
    format: DisplayFormat,
    explicit_multiply: bool,
    solved_rhs: Option<&expr::Expression>,
    table: Option<&SymbolTable>,
    trig_scale: f64,
) {
    let lhs_expr = if solved_rhs.is_some() {
        let mut x_expr = expr::Expression::new();
        x_expr.push(symbol::Symbol::X);
        x_expr
    } else {
        m.lhs.expr.clone()
    };
    let rhs_expr = solved_rhs.unwrap_or(&m.rhs.expr);

    let lhs_str = format.format_expression(&lhs_expr, explicit_multiply, table, trig_scale);
    let rhs_str = format.format_expression(rhs_expr, explicit_multiply, table, trig_scale);

    println!(
        "{:>24} = {:<24} for x = {:.15} {{{}}}",
        lhs_str, rhs_str, m.x_value, m.complexity
    );
}

/// Print the header for verbose output.
pub fn print_header(target: f64, level: i32) {
    println!();
    println!("  Target: {}", target);
    println!("  Level: {}", level);
    println!();
}

/// Print the footer for verbose output.
pub fn print_footer(stats: &search::SearchStats, elapsed: std::time::Duration) {
    println!();
    println!("  === Summary ===");
    let total_tested = stats.lhs_tested.saturating_add(stats.candidates_tested);
    println!("  Total expressions tested: {}", total_tested);
    println!("  LHS expressions: {}", stats.lhs_count);
    println!("  RHS expressions: {}", stats.rhs_count);
    println!("  Search time: {:.3}s", elapsed.as_secs_f64());
}

/// Build an expression from a slice of symbols.
fn expression_from_symbols(symbols: &[symbol::Symbol]) -> expr::Expression {
    let mut expression = expr::Expression::new();
    for &sym in symbols {
        expression.push(sym);
    }
    expression
}

/// Decompose an expression into its subexpressions for step-by-step display.
fn decompose_subexpressions(expression: &expr::Expression) -> Vec<expr::Expression> {
    let mut stack: Vec<expr::Expression> = Vec::new();
    let mut steps = Vec::new();

    for &sym in expression.symbols() {
        match sym.seft() {
            symbol::Seft::A => {
                let mut atom = expr::Expression::new();
                atom.push(sym);
                stack.push(atom.clone());
                steps.push(atom);
            }
            symbol::Seft::B => {
                let Some(mut a) = stack.pop() else {
                    break;
                };
                a.push(sym);
                stack.push(a.clone());
                steps.push(a);
            }
            symbol::Seft::C => {
                let Some(b) = stack.pop() else {
                    break;
                };
                let Some(a) = stack.pop() else {
                    break;
                };
                let mut combined = expression_from_symbols(a.symbols());
                for &rhs_sym in b.symbols() {
                    combined.push(rhs_sym);
                }
                combined.push(sym);
                stack.push(combined.clone());
                steps.push(combined);
            }
        }
    }

    steps
}

/// Print step-by-step evaluation details for an expression.
#[allow(clippy::too_many_arguments)]
fn print_expression_steps(
    label: &str,
    expression: &expr::Expression,
    x: f64,
    format: DisplayFormat,
    explicit_multiply: bool,
    eval_context: &EvalContext<'_>,
    table: Option<&SymbolTable>,
) {
    println!("    {} steps:", label);
    for (idx, step_expr) in decompose_subexpressions(expression).iter().enumerate() {
        let rendered = format.format_expression(
            step_expr,
            explicit_multiply,
            table,
            eval_context.trig_argument_scale,
        );
        match ries_rs::eval::evaluate_with_context(step_expr, x, eval_context) {
            Ok(result) => println!(
                "      {:>2}. {:<28} value={:+.12e} deriv={:+.12e}",
                idx + 1,
                rendered,
                result.value,
                result.derivative
            ),
            Err(err) => println!(
                "      {:>2}. {:<28} evaluation error: {}",
                idx + 1,
                rendered,
                err
            ),
        }
    }
}

/// Print detailed work information for a set of matches (--show-work output).
pub fn print_show_work_details(
    shown_matches: &[&search::Match],
    format: DisplayFormat,
    explicit_multiply: bool,
    eval_context: &EvalContext<'_>,
    table: Option<&SymbolTable>,
) {
    if shown_matches.is_empty() {
        return;
    }

    println!();
    println!("  --show-work details:");
    for (idx, m) in shown_matches.iter().enumerate() {
        println!("  Match {} at x = {:.15}", idx + 1, m.x_value);
        print_expression_steps(
            "LHS",
            &m.lhs.expr,
            m.x_value,
            format,
            explicit_multiply,
            eval_context,
            table,
        );
        print_expression_steps(
            "RHS",
            &m.rhs.expr,
            m.x_value,
            format,
            explicit_multiply,
            eval_context,
            table,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::approx_constant)]
    fn test_format_value() {
        assert_eq!(format_value(2.71828), "2.7182800000");
        assert_eq!(format_value(1e10), "1.0000000000e10");
    }

    #[test]
    fn test_parse_display_format() {
        assert!(matches!(
            parse_display_format("0"),
            DisplayFormat::PostfixCompact
        ));
        assert!(matches!(
            parse_display_format("pretty"),
            DisplayFormat::Infix(expr::OutputFormat::Pretty)
        ));
        assert!(matches!(
            parse_display_format("mathematica"),
            DisplayFormat::Infix(expr::OutputFormat::Mathematica)
        ));
    }
}

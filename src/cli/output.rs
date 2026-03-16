//! Output formatting helpers for RIES
//!
//! This module provides functions for formatting expressions, matches,
//! and other output for display to the user.

use ries_rs::{expr, search, symbol, EvalContext, SymbolTable, EXACT_MATCH_TOLERANCE};

/// Display format for expression output.
#[derive(Debug, Clone, Copy)]
pub enum DisplayFormat {
    /// Infix notation with specified format
    Infix(expr::OutputFormat),
    /// Compact postfix notation
    PostfixCompact,
    /// Verbose postfix notation with named tokens
    PostfixVerbose,
    /// Condensed format (alias for PostfixCompact)
    Condensed,
}

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

/// Convert a symbol to its verbose postfix token name.
fn postfix_verbose_token(sym: symbol::Symbol) -> String {
    use symbol::Symbol;
    match sym {
        Symbol::Neg => "neg".to_string(),
        Symbol::Recip => "recip".to_string(),
        Symbol::Sqrt => "sqrt".to_string(),
        Symbol::Square => "dup*".to_string(),
        Symbol::Pow => "**".to_string(),
        Symbol::Root => "root".to_string(),
        Symbol::Log => "logn".to_string(),
        Symbol::Exp => "exp".to_string(),
        _ => sym.display_name(),
    }
}

/// Apply explicit '*' multiplication operators to infix output.
///
/// Converts implicit multiplication (e.g., "2 x") to explicit form (e.g., "2*x").
pub fn apply_explicit_multiply(infix: &str) -> String {
    let chars: Vec<char> = infix.chars().collect();
    let mut out = String::with_capacity(infix.len() + 8);
    for i in 0..chars.len() {
        let ch = chars[i];
        if ch != ' ' {
            out.push(ch);
            continue;
        }

        let prev = i.checked_sub(1).and_then(|idx| chars.get(idx).copied());
        let next = chars.get(i + 1).copied();
        let implicit_mul = prev.is_some_and(|c| c.is_ascii_digit() || c == ')')
            && next.is_some_and(|c| c.is_ascii_alphabetic() || c == '(');
        if implicit_mul {
            out.push('*');
        } else {
            out.push(' ');
        }
    }
    out
}

/// Format an expression for display using the specified format.
///
/// If a SymbolTable is provided, uses it for symbol names (table-driven formatting).
/// Otherwise, falls back to default symbol names.
pub fn format_expression_for_display(
    expression: &expr::Expression,
    format: DisplayFormat,
    explicit_multiply: bool,
    table: Option<&SymbolTable>,
) -> String {
    match format {
        DisplayFormat::Infix(inner) => {
            // Use table-driven formatting if a table is provided
            // Note: Table-driven formatting currently uses default infix style.
            // Future enhancement: could support both table symbols AND specific formatting.
            let infix = if let Some(t) = table {
                expression.to_infix_with_table(t)
            } else {
                expression.to_infix_with_format(inner)
            };
            if explicit_multiply {
                apply_explicit_multiply(&infix)
            } else {
                infix
            }
        }
        DisplayFormat::PostfixCompact | DisplayFormat::Condensed => expression.to_postfix(),
        DisplayFormat::PostfixVerbose => expression
            .symbols()
            .iter()
            .map(|sym| postfix_verbose_token(*sym))
            .collect::<Vec<_>>()
            .join(" "),
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
) {
    let lhs_expr = if solved_rhs.is_some() {
        let mut x_expr = expr::Expression::new();
        x_expr.push(symbol::Symbol::X);
        x_expr
    } else {
        m.lhs.expr.clone()
    };
    let rhs_expr = solved_rhs.unwrap_or(&m.rhs.expr);

    let lhs_str = format_expression_for_display(&lhs_expr, format, explicit_multiply, table);
    let rhs_str = format_expression_for_display(rhs_expr, format, explicit_multiply, table);

    let error_str = if m.error.abs() < EXACT_MATCH_TOLERANCE {
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
) {
    let lhs_expr = if solved_rhs.is_some() {
        let mut x_expr = expr::Expression::new();
        x_expr.push(symbol::Symbol::X);
        x_expr
    } else {
        m.lhs.expr.clone()
    };
    let rhs_expr = solved_rhs.unwrap_or(&m.rhs.expr);

    let lhs_str = format_expression_for_display(&lhs_expr, format, explicit_multiply, table);
    let rhs_str = format_expression_for_display(rhs_expr, format, explicit_multiply, table);

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
        let rendered = format_expression_for_display(step_expr, format, explicit_multiply, table);
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

/// Compute tolerance for --match-all-digits based on significant digits of the target.
///
/// When --match-all-digits is enabled, the match tolerance is set so that matches
/// must agree with the target value to all significant digits provided.
///
/// For example:
/// - Target "2.5" (1 sig fig after decimal) -> tolerance ~0.05 (half of last digit)
/// - Target "2.50" (2 sig figs after decimal) -> tolerance ~0.005
/// - Target "2.500" (3 sig figs after decimal) -> tolerance ~0.0005
pub fn compute_significant_digits_tolerance(target: f64) -> f64 {
    if target == 0.0 {
        return 1e-15;
    }

    // Convert to string to count significant digits
    let target_str = format!("{:.15}", target);

    // Remove trailing zeros after decimal to get actual precision
    let trimmed = target_str.trim_end_matches('0');

    // Find decimal point position
    let decimal_pos = trimmed.find('.');

    // Count digits after decimal (not counting trailing zeros we removed)
    let digits_after_decimal = if let Some(pos) = decimal_pos {
        // Digits after decimal in the trimmed string
        trimmed.len() - pos - 1
    } else {
        0
    };

    // Tolerance is 0.5 * 10^(-digits_after_decimal)
    // This means the match must agree to the last digit shown
    let tolerance = 0.5 * 10_f64.powi(-(digits_after_decimal as i32));

    // Ensure a minimum tolerance for numerical stability
    tolerance.max(1e-15)
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

    #[test]
    fn test_compute_significant_digits_tolerance() {
        // 2.5 has 1 digit after decimal -> tolerance ~0.05
        let tol = compute_significant_digits_tolerance(2.5);
        assert!(tol > 0.04 && tol < 0.06);

        // Note: 2.50 f64 literal is the same as 2.5, so we can't test 2 digits
        // The function works based on the actual f64 value, not the source literal
        // For values with more precision, the tolerance is smaller
        let tol = compute_significant_digits_tolerance(2.51);
        assert!(tol > 0.004 && tol < 0.006);
    }
}

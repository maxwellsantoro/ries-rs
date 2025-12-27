//! Expression generation
//!
//! Generates valid postfix expressions by enumerating "forms" (stack effect patterns).

use crate::expr::{EvaluatedExpr, Expression, MAX_EXPR_LEN};
use crate::eval::evaluate;
use crate::symbol::{NumType, Seft, Symbol};

/// Configuration for expression generation
#[derive(Clone)]
pub struct GenConfig {
    /// Maximum complexity score
    pub max_complexity: u16,
    /// Maximum expression length
    pub max_length: usize,
    /// Symbols to use for constants (Seft::A)
    pub constants: Vec<Symbol>,
    /// Symbols to use for unary ops (Seft::B)
    pub unary_ops: Vec<Symbol>,
    /// Symbols to use for binary ops (Seft::C)
    pub binary_ops: Vec<Symbol>,
    /// Minimum numeric type required
    pub min_num_type: NumType,
    /// Whether to generate LHS expressions (containing x)
    pub generate_lhs: bool,
    /// Whether to generate RHS expressions (no x)
    pub generate_rhs: bool,
}

impl Default for GenConfig {
    fn default() -> Self {
        Self {
            max_complexity: 128,
            max_length: MAX_EXPR_LEN,
            constants: Symbol::constants().to_vec(),
            unary_ops: Symbol::unary_ops().to_vec(),
            binary_ops: Symbol::binary_ops().to_vec(),
            min_num_type: NumType::Transcendental,
            generate_lhs: true,
            generate_rhs: true,
        }
    }
}

/// Result of expression generation
pub struct GeneratedExprs {
    /// LHS expressions (contain x)
    pub lhs: Vec<EvaluatedExpr>,
    /// RHS expressions (constants only)
    pub rhs: Vec<EvaluatedExpr>,
}

/// Generate all valid expressions up to the configured limits
pub fn generate_all(config: &GenConfig, target: f64) -> GeneratedExprs {
    let mut lhs = Vec::new();
    let mut rhs = Vec::new();

    // Generate expressions for each possible "form" (sequence of stack effects)
    generate_recursive(
        config,
        target,
        &mut Expression::new(),
        0, // current stack depth
        &mut lhs,
        &mut rhs,
    );

    GeneratedExprs { lhs, rhs }
}

/// Recursively generate expressions
fn generate_recursive(
    config: &GenConfig,
    target: f64,
    current: &mut Expression,
    stack_depth: usize,
    lhs_out: &mut Vec<EvaluatedExpr>,
    rhs_out: &mut Vec<EvaluatedExpr>,
) {
    // Check if we have a complete expression
    if stack_depth == 1 && !current.is_empty() {
        // Try to evaluate it
        if let Ok(result) = evaluate(current, target) {
            // Check numeric type constraint
            if result.num_type >= config.min_num_type {
                let expr = current.clone();
                let eval_expr = EvaluatedExpr::new(
                    expr,
                    result.value,
                    result.derivative,
                    result.num_type,
                );

                if current.contains_x() {
                    if config.generate_lhs && result.derivative.abs() > 1e-100 {
                        lhs_out.push(eval_expr);
                    }
                } else if config.generate_rhs {
                    rhs_out.push(eval_expr);
                }
            }
        }
    }

    // Check limits before recursing
    if current.len() >= config.max_length {
        return;
    }
    if current.complexity() >= config.max_complexity {
        return;
    }

    // Calculate minimum additional complexity needed to complete expression
    let min_remaining = min_complexity_to_complete(stack_depth, config);
    if current.complexity() + min_remaining > config.max_complexity {
        return;
    }

    // Try adding each possible symbol

    // Constants (Seft::A) - always increase stack by 1
    for &sym in &config.constants {
        if current.complexity() + sym.weight() > config.max_complexity {
            continue;
        }

        // Skip x if we only want RHS
        if sym == Symbol::X && !config.generate_lhs {
            continue;
        }

        current.push(sym);
        generate_recursive(config, target, current, stack_depth + 1, lhs_out, rhs_out);
        current.pop();
    }

    // Also add x for LHS generation
    if config.generate_lhs && !config.constants.contains(&Symbol::X) {
        let sym = Symbol::X;
        if current.complexity() + sym.weight() <= config.max_complexity {
            current.push(sym);
            generate_recursive(config, target, current, stack_depth + 1, lhs_out, rhs_out);
            current.pop();
        }
    }

    // Unary operators (Seft::B) - need at least 1 on stack
    if stack_depth >= 1 {
        for &sym in &config.unary_ops {
            if current.complexity() + sym.weight() > config.max_complexity {
                continue;
            }

            // Apply pruning rules
            if should_prune_unary(current, sym) {
                continue;
            }

            current.push(sym);
            generate_recursive(config, target, current, stack_depth, lhs_out, rhs_out);
            current.pop();
        }
    }

    // Binary operators (Seft::C) - need at least 2 on stack
    if stack_depth >= 2 {
        for &sym in &config.binary_ops {
            if current.complexity() + sym.weight() > config.max_complexity {
                continue;
            }

            // Apply pruning rules
            if should_prune_binary(current, sym) {
                continue;
            }

            current.push(sym);
            generate_recursive(config, target, current, stack_depth - 1, lhs_out, rhs_out);
            current.pop();
        }
    }
}

/// Calculate minimum complexity needed to reduce stack to depth 1
fn min_complexity_to_complete(stack_depth: usize, config: &GenConfig) -> u16 {
    if stack_depth <= 1 {
        return 0;
    }

    // Need (stack_depth - 1) binary operators to reduce to 1
    let min_binary_weight = config.binary_ops.iter()
        .map(|s| s.weight())
        .min()
        .unwrap_or(4);

    ((stack_depth - 1) as u16) * min_binary_weight
}

/// Pruning rules for unary operators to avoid redundant expressions
fn should_prune_unary(expr: &Expression, sym: Symbol) -> bool {
    let symbols = expr.symbols();
    if symbols.is_empty() {
        return false;
    }

    let last = symbols[symbols.len() - 1];

    use Symbol::*;

    match (last, sym) {
        // Double negation: --a = a
        (Neg, Neg) => true,
        // Double reciprocal: 1/(1/a) = a
        (Recip, Recip) => true,
        // sqrt(a^2) = |a| (we don't handle absolute value)
        (Square, Sqrt) => true,
        // (sqrt(a))^2 = a
        (Sqrt, Square) => true,
        // ln(e^a) = a
        (Exp, Ln) => true,
        // e^(ln(a)) = a
        (Ln, Exp) => true,
        // Trig identities that reduce
        (SinPi, SinPi) | (CosPi, CosPi) => true,

        _ => false,
    }
}

/// Pruning rules for binary operators
fn should_prune_binary(expr: &Expression, sym: Symbol) -> bool {
    let symbols = expr.symbols();
    if symbols.len() < 2 {
        return false;
    }

    let last = symbols[symbols.len() - 1];
    let prev = symbols[symbols.len() - 2];

    use Symbol::*;

    match sym {
        // a - a = 0 (if both operands are identical)
        Sub if is_same_subexpr(symbols, 2) => true,
        // a / a = 1
        Div if is_same_subexpr(symbols, 2) => true,

        // Prefer a*2 over a+a
        Add if is_same_subexpr(symbols, 2) => true,

        // 1^b = 1, a^0 = 1 (degenerate)
        Pow if last == One || prev == One => {
            // Check if base is 1 (more complex check needed)
            false
        }

        // x * 1 = x, 1 * x = x
        Mul if last == One || prev == One => true,

        // x + 0 or 0 + x (we don't have 0, but similar with expressions)

        // Ordering: prefer 2+3 over 3+2 for commutative ops
        Add | Mul if prev > last && is_constant(last) && is_constant(prev) => true,

        _ => false,
    }
}

/// Check if the last n stack items are identical subexpressions
fn is_same_subexpr(_symbols: &[Symbol], _n: usize) -> bool {
    // Simplified check - would need more complex analysis
    // For now, just check if last two symbols are the same constant
    false // Conservative: don't prune
}

/// Check if a symbol is a constant (no x)
fn is_constant(sym: Symbol) -> bool {
    matches!(sym.seft(), Seft::A) && sym != Symbol::X
}

/// Generate expressions in parallel using Rayon
#[cfg(feature = "parallel")]
pub fn generate_all_parallel(config: &GenConfig, target: f64) -> GeneratedExprs {
    use rayon::prelude::*;

    // Split work by first symbol
    let first_symbols: Vec<Symbol> = config.constants.iter()
        .copied()
        .chain(if config.generate_lhs { Some(Symbol::X) } else { None })
        .collect();

    let results: Vec<(Vec<EvaluatedExpr>, Vec<EvaluatedExpr>)> = first_symbols
        .par_iter()
        .map(|&first_sym| {
            let mut lhs = Vec::new();
            let mut rhs = Vec::new();
            let mut expr = Expression::new();
            expr.push(first_sym);

            generate_recursive(
                config, target, &mut expr, 1, &mut lhs, &mut rhs
            );

            (lhs, rhs)
        })
        .collect();

    // Merge results
    let mut all_lhs = Vec::new();
    let mut all_rhs = Vec::new();
    for (lhs, rhs) in results {
        all_lhs.extend(lhs);
        all_rhs.extend(rhs);
    }

    GeneratedExprs { lhs: all_lhs, rhs: all_rhs }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_simple() {
        let mut config = GenConfig::default();
        config.max_complexity = 50;
        config.max_length = 5;
        config.generate_lhs = false; // Only RHS for simpler test

        let result = generate_all(&config, 1.0);

        // Should have some RHS expressions
        assert!(!result.rhs.is_empty());

        // All should be valid (evaluate without error)
        for expr in &result.rhs {
            assert!(!expr.expr.contains_x());
        }
    }

    #[test]
    fn test_generate_lhs() {
        let mut config = GenConfig::default();
        config.max_complexity = 40;
        config.max_length = 4;
        config.generate_rhs = false;

        let result = generate_all(&config, 2.0);

        // Should have LHS expressions containing x
        assert!(!result.lhs.is_empty());
        for expr in &result.lhs {
            assert!(expr.expr.contains_x());
        }
    }

    #[test]
    fn test_complexity_limit() {
        let mut config = GenConfig::default();
        config.max_complexity = 30;

        let result = generate_all(&config, 1.0);

        for expr in result.rhs.iter().chain(result.lhs.iter()) {
            assert!(expr.expr.complexity() <= config.max_complexity);
        }
    }
}

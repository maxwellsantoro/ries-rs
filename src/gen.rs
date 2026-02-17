//! Expression generation
//!
//! Generates valid postfix expressions by enumerating "forms" (stack effect patterns).

use crate::eval::evaluate_fast_with_constants_and_functions;
use crate::expr::{EvaluatedExpr, Expression, MAX_EXPR_LEN};
use crate::profile::UserConstant;
use crate::symbol::{NumType, Seft, Symbol};
use crate::udf::UserFunction;
use std::collections::HashMap;

/// Configuration for expression generation
#[derive(Clone)]
pub struct GenConfig {
    /// Maximum complexity for LHS expressions (containing x)
    pub max_lhs_complexity: u32,
    /// Maximum complexity for RHS expressions (constants only)
    pub max_rhs_complexity: u32,
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
    /// User-defined constants (for evaluation during generation)
    pub user_constants: Vec<UserConstant>,
    /// User-defined functions (for evaluation during generation)
    pub user_functions: Vec<UserFunction>,
}

impl Default for GenConfig {
    fn default() -> Self {
        Self {
            max_lhs_complexity: 128,
            max_rhs_complexity: 128,
            max_length: MAX_EXPR_LEN,
            constants: Symbol::constants().to_vec(),
            unary_ops: Symbol::unary_ops().to_vec(),
            binary_ops: Symbol::binary_ops().to_vec(),
            min_num_type: NumType::Transcendental,
            generate_lhs: true,
            generate_rhs: true,
            user_constants: Vec::new(),
            user_functions: Vec::new(),
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

/// Quantize a value to reduce floating-point noise
/// Uses ~8 significant digits for deduplication
#[inline]
fn quantize_value(v: f64) -> i64 {
    if !v.is_finite() || v.abs() > 1e10 {
        // For very large values, use a different quantization to avoid overflow
        if v > 1e10 {
            return i64::MAX - 1;
        } else if v < -1e10 {
            return i64::MIN + 1;
        }
        return i64::MAX;
    }
    // Scale to preserve ~8 significant digits (avoid overflow)
    (v * 1e8).round() as i64
}

/// Key for LHS deduplication: (quantized value, quantized derivative)
type LhsKey = (i64, i64);

/// Generate all valid expressions up to the configured limits
pub fn generate_all(config: &GenConfig, target: f64) -> GeneratedExprs {
    let mut lhs_raw = Vec::new();
    let mut rhs_raw = Vec::new();

    // Generate expressions for each possible "form" (sequence of stack effects)
    generate_recursive(
        config,
        target,
        &mut Expression::new(),
        0, // current stack depth
        &mut lhs_raw,
        &mut rhs_raw,
    );

    // Deduplicate RHS by value, keeping simplest expression for each value
    let mut rhs_map: HashMap<i64, EvaluatedExpr> = HashMap::new();
    for expr in rhs_raw {
        let key = quantize_value(expr.value);
        rhs_map
            .entry(key)
            .and_modify(|existing| {
                if expr.expr.complexity() < existing.expr.complexity() {
                    *existing = expr.clone();
                }
            })
            .or_insert(expr);
    }

    // Deduplicate LHS by (value, derivative), keeping simplest expression
    let mut lhs_map: HashMap<LhsKey, EvaluatedExpr> = HashMap::new();
    for expr in lhs_raw {
        let key = (quantize_value(expr.value), quantize_value(expr.derivative));
        lhs_map
            .entry(key)
            .and_modify(|existing| {
                if expr.expr.complexity() < existing.expr.complexity() {
                    *existing = expr.clone();
                }
            })
            .or_insert(expr);
    }

    GeneratedExprs {
        lhs: lhs_map.into_values().collect(),
        rhs: rhs_map.into_values().collect(),
    }
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
        // Try to evaluate it with user constants and functions support
        if let Ok(result) = evaluate_fast_with_constants_and_functions(
            current,
            target,
            &config.user_constants,
            &config.user_functions,
        ) {
            // Skip expressions with extreme values (overflow-prone, unlikely useful)
            if !result.value.is_finite() || result.value.abs() > 1e12 {
                // Skip infinite or very large values
            } else if result.num_type >= config.min_num_type {
                let expr = current.clone();
                let eval_expr =
                    EvaluatedExpr::new(expr, result.value, result.derivative, result.num_type);

                if current.contains_x() {
                    if config.generate_lhs && current.complexity() <= config.max_lhs_complexity {
                        // Keep all LHS expressions; derivative≈0 cases handled in search
                        lhs_out.push(eval_expr);
                    }
                } else if config.generate_rhs && current.complexity() <= config.max_rhs_complexity {
                    rhs_out.push(eval_expr);
                }
            }
        }
    }

    // Check limits before recursing
    if current.len() >= config.max_length {
        return;
    }

    // Use appropriate complexity limit based on whether expression contains x
    let max_complexity = if current.contains_x() {
        config.max_lhs_complexity
    } else {
        // For RHS-only paths, use RHS limit
        // For paths that might still add x, use the max of both
        std::cmp::max(config.max_lhs_complexity, config.max_rhs_complexity)
    };

    if current.complexity() >= max_complexity {
        return;
    }

    // Calculate minimum additional complexity needed to complete expression
    let min_remaining = min_complexity_to_complete(stack_depth, config);
    if current.complexity() + min_remaining > max_complexity {
        return;
    }

    // Try adding each possible symbol

    // Constants (Seft::A) - always increase stack by 1
    for &sym in &config.constants {
        if current.complexity() + sym.weight() > max_complexity {
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
        if current.complexity() + sym.weight() <= max_complexity {
            current.push(sym);
            generate_recursive(config, target, current, stack_depth + 1, lhs_out, rhs_out);
            current.pop();
        }
    }

    // Unary operators (Seft::B) - need at least 1 on stack
    if stack_depth >= 1 {
        for &sym in &config.unary_ops {
            if current.complexity() + sym.weight() > max_complexity {
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
            if current.complexity() + sym.weight() > max_complexity {
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
fn min_complexity_to_complete(stack_depth: usize, config: &GenConfig) -> u32 {
    if stack_depth <= 1 {
        return 0;
    }

    // Need (stack_depth - 1) binary operators to reduce to 1
    let min_binary_weight = config
        .binary_ops
        .iter()
        .map(|s| s.weight())
        .min()
        .unwrap_or(4);

    ((stack_depth - 1) as u32) * min_binary_weight
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

        // Additional pruning rules for cleaner output:
        // 1/sqrt(a) and 1/a^2 are rare, prefer a^-0.5 or a^-2 notation
        (Sqrt, Recip) => true,
        (Square, Recip) => true,
        // 1/ln(a) is rarely useful
        (Ln, Recip) => true,
        // Double square: (a^2)^2 = a^4, use power directly
        (Square, Square) => true,
        // Double sqrt: sqrt(sqrt(a)) = a^0.25, use power directly
        (Sqrt, Sqrt) => true,
        // Negation after subtraction is redundant with addition
        // e.g., -(a-b) = b-a which we could express directly
        (Sub, Neg) => true,

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
        // x - x = 0 (trivial - always 0)
        Sub if last == X && prev == X => true,

        // a / a = 1 (degenerate if a contains x)
        Div if is_same_subexpr(symbols, 2) => true,
        // x / x = 1 (trivial identity)
        Div if last == X && prev == X => true,
        // Division by 1: a/1 = a (useless)
        Div if last == One => true,

        // Prefer a*2 over a+a
        Add if is_same_subexpr(symbols, 2) => true,
        // x + (-x) = 0 - check for negated x
        Add if last == Neg
            && symbols.len() >= 3
            && symbols[symbols.len() - 2] == X
            && prev == X =>
        {
            true
        }

        // 1^b = 1 (degenerate - always equals 1 regardless of b)
        // This catches 1^x, 1^(anything)
        Pow if prev == One => true,
        // a^1 = a (useless)
        Pow if last == One => true,

        // x * 1 = x, 1 * x = x
        Mul if last == One || prev == One => true,

        // a"/1 = a^(1/1) = a (1st root is identity)
        // But more importantly: 1"/x = 1^(1/x) = 1 (degenerate)
        Root if prev == One => true,
        // x"/1 means 1^(1/x) = 1 (degenerate)
        Root if last == One => true,
        // 2nd root is just sqrt, prefer using sqrt
        Root if last == Two => true,

        // log_x(x) = 1 (trivial identity)
        Log if last == X && prev == X => true,
        // log_1(anything) is undefined/infinite, log_a(1) = 0
        Log if prev == One || last == One => true,
        // log_e(a) = ln(a) - prefer ln notation
        Log if prev == E => true,

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
    let first_symbols: Vec<Symbol> = config
        .constants
        .iter()
        .copied()
        .chain(if config.generate_lhs {
            Some(Symbol::X)
        } else {
            None
        })
        .collect();

    let results: Vec<(Vec<EvaluatedExpr>, Vec<EvaluatedExpr>)> = first_symbols
        .par_iter()
        .map(|&first_sym| {
            let mut lhs = Vec::new();
            let mut rhs = Vec::new();
            let mut expr = Expression::new();
            expr.push(first_sym);

            generate_recursive(config, target, &mut expr, 1, &mut lhs, &mut rhs);

            (lhs, rhs)
        })
        .collect();

    // Merge results
    let mut lhs_raw = Vec::new();
    let mut rhs_raw = Vec::new();
    for (lhs, rhs) in results {
        lhs_raw.extend(lhs);
        rhs_raw.extend(rhs);
    }

    // Deduplicate RHS by value, keeping simplest expression for each value
    let mut rhs_map: HashMap<i64, EvaluatedExpr> = HashMap::new();
    for expr in rhs_raw {
        let key = quantize_value(expr.value);
        rhs_map
            .entry(key)
            .and_modify(|existing| {
                if expr.expr.complexity() < existing.expr.complexity() {
                    *existing = expr.clone();
                }
            })
            .or_insert(expr);
    }

    // Deduplicate LHS by (value, derivative), keeping simplest expression
    let mut lhs_map: HashMap<LhsKey, EvaluatedExpr> = HashMap::new();
    for expr in lhs_raw {
        let key = (quantize_value(expr.value), quantize_value(expr.derivative));
        lhs_map
            .entry(key)
            .and_modify(|existing| {
                if expr.expr.complexity() < existing.expr.complexity() {
                    *existing = expr.clone();
                }
            })
            .or_insert(expr);
    }

    GeneratedExprs {
        lhs: lhs_map.into_values().collect(),
        rhs: rhs_map.into_values().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create a fast test config with limited complexity and operators
    fn fast_test_config() -> GenConfig {
        GenConfig {
            max_lhs_complexity: 20,
            max_rhs_complexity: 20,
            max_length: 8,
            constants: vec![
                Symbol::One, Symbol::Two, Symbol::Three, Symbol::Four,
                Symbol::Five, Symbol::Pi, Symbol::E,
            ],
            unary_ops: vec![Symbol::Neg, Symbol::Recip, Symbol::Square, Symbol::Sqrt],
            binary_ops: vec![Symbol::Add, Symbol::Sub, Symbol::Mul, Symbol::Div],
            min_num_type: NumType::Transcendental,
            generate_lhs: true,
            generate_rhs: true,
            user_constants: Vec::new(),
            user_functions: Vec::new(),
        }
    }

    #[test]
    fn test_generate_simple() {
        let mut config = fast_test_config();
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
        let mut config = fast_test_config();
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
        let config = fast_test_config();

        let result = generate_all(&config, 1.0);

        for expr in &result.rhs {
            assert!(expr.expr.complexity() <= config.max_rhs_complexity);
        }
        for expr in &result.lhs {
            assert!(expr.expr.complexity() <= config.max_lhs_complexity);
        }
    }
}

// =============================================================================
// EXPENSIVE DEBUG TESTS
// These tests use high complexity limits and all operators.
// Run with `cargo test -- --ignored` to include them.
// =============================================================================

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
#[allow(unused_imports)]
fn test_x_to_x_generated() {
    use crate::expr::Expression;

    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let result = generate_all(&config, 2.5);

    // Check if xx^ (x^x) is generated
    let has_xx_pow = result.lhs.iter().any(|e| e.expr.to_postfix() == "xx^");

    println!("LHS contains xx^ (x^x): {}", has_xx_pow);

    // Find expressions with value near 9.88 (x^x at 2.5)
    let near_xx: Vec<_> = result
        .lhs
        .iter()
        .filter(|e| (e.value - 9.88).abs() < 0.5)
        .take(5)
        .collect();

    println!("\nLHS expressions with value ≈ 9.88:");
    for e in &near_xx {
        println!(
            "  {} = {} (value={:.4}, deriv={:.4})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value,
            e.derivative
        );
    }

    assert!(has_xx_pow, "xx^ should be generated");
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_pi_squared_in_rhs() {
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let result = generate_all(&config, 2.5);

    // Check for pi^2 (postfix: ps)
    let has_pi_sq = result.rhs.iter().any(|e| e.expr.to_postfix() == "ps");
    println!("RHS contains ps (pi^2): {}", has_pi_sq);

    // Find RHS near 9.87 (pi^2)
    let near_pi2: Vec<_> = result
        .rhs
        .iter()
        .filter(|e| (e.value - 9.87).abs() < 0.5)
        .take(5)
        .collect();

    println!("\nRHS expressions with value ≈ 9.87 (pi^2):");
    for e in &near_pi2 {
        println!(
            "  {} = {} (value={:.6})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value
        );
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_pi_squared_value() {
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 60;
    config.max_rhs_complexity = 60;

    let result = generate_all(&config, 2.5);

    // Find RHS with value exactly near pi^2 = 9.8696
    let pi_sq = std::f64::consts::PI * std::f64::consts::PI;
    println!("pi^2 = {}", pi_sq);

    let near_pi2: Vec<_> = result
        .rhs
        .iter()
        .filter(|e| (e.value - pi_sq).abs() < 0.01)
        .collect();

    println!("\nRHS expressions with value within 0.01 of pi^2:");
    for e in &near_pi2 {
        println!(
            "  {} = {} (value={:.10})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value
        );
    }

    // Also check what's at value 9.882 (x^x at 2.5)
    let xx_val = 2.5_f64.powf(2.5);
    println!("\nx^x at 2.5 = {}", xx_val);

    let near_xx: Vec<_> = result
        .rhs
        .iter()
        .filter(|e| (e.value - xx_val).abs() < 0.02)
        .collect();

    println!("\nRHS expressions with value within 0.02 of x^x:");
    for e in &near_xx {
        println!(
            "  {} = {} (value={:.10})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value
        );
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_find_ps_specifically() {
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 60;
    config.max_rhs_complexity = 60;

    let result = generate_all(&config, 2.5);

    // Find ps specifically
    let ps_expr = result.rhs.iter().find(|e| e.expr.to_postfix() == "ps");

    if let Some(e) = ps_expr {
        println!(
            "Found ps: {} = {} (value={:.10})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value
        );
    } else {
        println!("ps not found in deduplicated RHS!");

        // Check what expression has the same quantized value
        let pi_sq = std::f64::consts::PI * std::f64::consts::PI;
        let key = (pi_sq * 1e8).round() as i64;
        println!("Key for pi^2 = {}", key);

        // Find all expressions with same key
        let same_key: Vec<_> = result
            .rhs
            .iter()
            .filter(|e| (e.value * 1e8).round() as i64 == key)
            .collect();

        println!("\nExpressions with same key:");
        for e in &same_key {
            println!(
                "  {} = {} (value={:.10}, complexity={})",
                e.expr.to_postfix(),
                e.expr.to_infix(),
                e.value,
                e.expr.complexity()
            );
        }
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_xx_in_final_lhs() {
    let mut config = GenConfig::default();
    config.max_lhs_complexity = 50;
    config.max_rhs_complexity = 50;

    let result = generate_all(&config, 2.5);

    // Check if xx^ is in final deduplicated LHS
    let xx_expr = result.lhs.iter().find(|e| e.expr.to_postfix() == "xx^");

    if let Some(e) = xx_expr {
        println!(
            "xx^ in final LHS: {} (value={:.4}, deriv={:.4}, complexity={})",
            e.expr.to_infix(),
            e.value,
            e.derivative,
            e.expr.complexity()
        );
    } else {
        println!("xx^ NOT in final LHS - was deduplicated");

        // Find what has the same key
        let xx_val = 2.5_f64.powf(2.5);
        let xx_deriv = xx_val * (1.0 + 2.5_f64.ln());
        println!("Expected: value={:.4}, deriv={:.4}", xx_val, xx_deriv);

        let key_val = (xx_val * 1e8).round() as i64;
        let key_deriv = (xx_deriv * 1e8).round() as i64;
        println!("Key: ({}, {})", key_val, key_deriv);

        // Find expressions with same key
        let same: Vec<_> = result
            .lhs
            .iter()
            .filter(|e| {
                let kv = (e.value * 1e8).round() as i64;
                let kd = (e.derivative * 1e8).round() as i64;
                kv == key_val && kd == key_deriv
            })
            .collect();

        println!("\nExpressions with same key:");
        for e in &same {
            println!(
                "  {} (value={:.4}, deriv={:.4}, complexity={})",
                e.expr.to_postfix(),
                e.value,
                e.derivative,
                e.expr.complexity()
            );
        }
    }
}

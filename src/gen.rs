//! Expression generation
//!
//! Generates valid postfix expressions by enumerating "forms" (stack effect patterns).
//!
//! # Streaming Architecture
//!
//! For high complexity levels, the traditional approach of generating ALL expressions
//! into memory before matching can cause memory exhaustion. This module provides both:
//!
//! - **Batch generation**: `generate_all()` returns all expressions (backward compatible)
//! - **Streaming generation**: `generate_streaming()` processes expressions via callbacks
//!
//! Streaming reduces memory from O(expressions) to O(depth) by processing expressions
//! as they're generated rather than accumulating them.

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
    /// Optional RHS-only constants override
    pub rhs_constants: Option<Vec<Symbol>>,
    /// Optional RHS-only unary ops override
    pub rhs_unary_ops: Option<Vec<Symbol>>,
    /// Optional RHS-only binary ops override
    pub rhs_binary_ops: Option<Vec<Symbol>>,
    /// Maximum per-expression symbol counts (for -O style limits)
    pub symbol_max_counts: HashMap<Symbol, u32>,
    /// Optional RHS-only symbol count limits (for --O-RHS)
    pub rhs_symbol_max_counts: Option<HashMap<Symbol, u32>>,
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
            rhs_constants: None,
            rhs_unary_ops: None,
            rhs_binary_ops: None,
            symbol_max_counts: HashMap::new(),
            rhs_symbol_max_counts: None,
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

/// Callbacks for streaming expression generation
///
/// Using callbacks instead of accumulation allows processing expressions
/// as they're generated, reducing memory from O(expressions) to O(depth).
pub struct StreamingCallbacks<'a> {
    /// Called for each RHS (constant-only) expression generated
    /// Return false to stop generation early
    pub on_rhs: &'a mut dyn FnMut(&EvaluatedExpr) -> bool,
    /// Called for each LHS (contains x) expression generated
    /// Return false to stop generation early
    pub on_lhs: &'a mut dyn FnMut(&EvaluatedExpr) -> bool,
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

    if config.generate_lhs && config.generate_rhs && has_rhs_symbol_overrides(config) {
        // LHS pass with base symbol set.
        let mut lhs_config = config.clone();
        lhs_config.generate_lhs = true;
        lhs_config.generate_rhs = false;
        generate_recursive(
            &lhs_config,
            target,
            &mut Expression::new(),
            0,
            &mut lhs_raw,
            &mut rhs_raw,
        );

        // RHS pass with RHS-specific symbol overrides.
        let rhs_config = rhs_only_config(config);
        generate_recursive(
            &rhs_config,
            target,
            &mut Expression::new(),
            0,
            &mut lhs_raw,
            &mut rhs_raw,
        );
    } else {
        // Generate expressions for each possible "form" (sequence of stack effects)
        generate_recursive(
            config,
            target,
            &mut Expression::new(),
            0, // current stack depth
            &mut lhs_raw,
            &mut rhs_raw,
        );
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

/// Generate expressions with streaming callbacks for memory-efficient processing
///
/// This function is the foundation of the streaming architecture. Instead of
/// accumulating all expressions in memory, it calls the provided callbacks
/// for each generated expression, allowing immediate processing.
///
/// # Memory Efficiency
///
/// - Traditional: O(expressions) memory - all expressions stored before processing
/// - Streaming: O(depth) memory - only the recursion stack is stored
///
/// # Early Exit
///
/// The callbacks can return `false` to signal early termination. This is useful
/// when good matches have been found and additional expressions aren't needed.
///
/// # Deduplication
///
/// The caller is responsible for deduplication if needed. This allows flexibility
/// in deduplication strategies (e.g., per-batch, per-tier, etc.).
///
/// # Example
///
/// ```ignore
/// let mut rhs_count = 0;
/// let mut lhs_count = 0;
/// let callbacks = StreamingCallbacks {
///     on_rhs: &mut |expr| {
///         rhs_count += 1;
///         process_rhs(expr);
///         true // continue generation
///     },
///     on_lhs: &mut |expr| {
///         lhs_count += 1;
///         process_lhs(expr);
///         true // continue generation
///     },
/// };
/// generate_streaming(&config, target, callbacks);
/// ```
pub fn generate_streaming(config: &GenConfig, target: f64, callbacks: &mut StreamingCallbacks) {
    if config.generate_lhs && config.generate_rhs && has_rhs_symbol_overrides(config) {
        let mut lhs_config = config.clone();
        lhs_config.generate_lhs = true;
        lhs_config.generate_rhs = false;
        if !generate_recursive_streaming(
            &lhs_config,
            target,
            &mut Expression::new(),
            0,
            callbacks,
        ) {
            return;
        }

        let rhs_config = rhs_only_config(config);
        generate_recursive_streaming(&rhs_config, target, &mut Expression::new(), 0, callbacks);
    } else {
        generate_recursive_streaming(
            config,
            target,
            &mut Expression::new(),
            0, // current stack depth
            callbacks,
        );
    }
}

#[inline]
fn has_rhs_symbol_overrides(config: &GenConfig) -> bool {
    config.rhs_constants.is_some()
        || config.rhs_unary_ops.is_some()
        || config.rhs_binary_ops.is_some()
        || config.rhs_symbol_max_counts.is_some()
}

fn rhs_only_config(config: &GenConfig) -> GenConfig {
    let mut rhs_config = config.clone();
    rhs_config.generate_lhs = false;
    rhs_config.generate_rhs = true;
    if let Some(constants) = &config.rhs_constants {
        rhs_config.constants = constants.clone();
    }
    if let Some(unary_ops) = &config.rhs_unary_ops {
        rhs_config.unary_ops = unary_ops.clone();
    }
    if let Some(binary_ops) = &config.rhs_binary_ops {
        rhs_config.binary_ops = binary_ops.clone();
    }
    if let Some(rhs_symbol_max_counts) = &config.rhs_symbol_max_counts {
        rhs_config.symbol_max_counts = rhs_symbol_max_counts.clone();
    }
    rhs_config
}

#[inline]
fn exceeds_symbol_limit(config: &GenConfig, current: &Expression, sym: Symbol) -> bool {
    config
        .symbol_max_counts
        .get(&sym)
        .is_some_and(|&max| current.count_symbol(sym) >= max)
}

/// Recursively generate expressions with streaming callbacks
///
/// This is the core streaming generation function. It mirrors `generate_recursive`
/// but calls callbacks instead of accumulating expressions.
fn generate_recursive_streaming(
    config: &GenConfig,
    target: f64,
    current: &mut Expression,
    stack_depth: usize,
    callbacks: &mut StreamingCallbacks,
) -> bool {
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
            if result.value.is_finite() && result.value.abs() <= 1e12 && result.num_type >= config.min_num_type {
                let expr = current.clone();
                let eval_expr =
                    EvaluatedExpr::new(expr, result.value, result.derivative, result.num_type);

                if current.contains_x() {
                    if config.generate_lhs && current.complexity() <= config.max_lhs_complexity {
                        // Call the LHS callback; return false if it signals stop
                        if !(callbacks.on_lhs)(&eval_expr) {
                            return false;
                        }
                    }
                } else if config.generate_rhs && current.complexity() <= config.max_rhs_complexity {
                    // Call the RHS callback; return false if it signals stop
                    if !(callbacks.on_rhs)(&eval_expr) {
                        return false;
                    }
                }
            }
        }
    }

    // Check limits before recursing
    if current.len() >= config.max_length {
        return true;
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
        return true;
    }

    // Calculate minimum additional complexity needed to complete expression
    let min_remaining = min_complexity_to_complete(stack_depth, config);
    if current.complexity() + min_remaining > max_complexity {
        return true;
    }

    // Try adding each possible symbol

    // Constants (Seft::A) - always increase stack by 1
    for &sym in &config.constants {
        if current.complexity() + sym.weight() > max_complexity {
            continue;
        }
        if exceeds_symbol_limit(config, current, sym) {
            continue;
        }

        // Skip x if we only want RHS
        if sym == Symbol::X && !config.generate_lhs {
            continue;
        }

        current.push(sym);
        if !generate_recursive_streaming(config, target, current, stack_depth + 1, callbacks) {
            current.pop();
            return false;
        }
        current.pop();
    }

    // Also add x for LHS generation
    if config.generate_lhs && !config.constants.contains(&Symbol::X) {
        let sym = Symbol::X;
        if current.complexity() + sym.weight() <= max_complexity
            && !exceeds_symbol_limit(config, current, sym)
        {
            current.push(sym);
            if !generate_recursive_streaming(config, target, current, stack_depth + 1, callbacks) {
                current.pop();
                return false;
            }
            current.pop();
        }
    }

    // Unary operators (Seft::B) - need at least 1 on stack
    if stack_depth >= 1 {
        for &sym in &config.unary_ops {
            if current.complexity() + sym.weight() > max_complexity {
                continue;
            }
            if exceeds_symbol_limit(config, current, sym) {
                continue;
            }

            // Apply pruning rules
            if should_prune_unary(current, sym) {
                continue;
            }

            current.push(sym);
            if !generate_recursive_streaming(config, target, current, stack_depth, callbacks) {
                current.pop();
                return false;
            }
            current.pop();
        }
    }

    // Binary operators (Seft::C) - need at least 2 on stack
    if stack_depth >= 2 {
        for &sym in &config.binary_ops {
            if current.complexity() + sym.weight() > max_complexity {
                continue;
            }
            if exceeds_symbol_limit(config, current, sym) {
                continue;
            }

            // Apply pruning rules
            if should_prune_binary(current, sym) {
                continue;
            }

            current.push(sym);
            if !generate_recursive_streaming(config, target, current, stack_depth - 1, callbacks) {
                current.pop();
                return false;
            }
            current.pop();
        }
    }

    true
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
        if exceeds_symbol_limit(config, current, sym) {
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
        if current.complexity() + sym.weight() <= max_complexity
            && !exceeds_symbol_limit(config, current, sym)
        {
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
            if exceeds_symbol_limit(config, current, sym) {
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
            if exceeds_symbol_limit(config, current, sym) {
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

        // ===== ENHANCED PRUNING RULES =====
        // Trig reduction: asin(sin(pi*x)/pi) = x, similar for acos
        // These are rarely useful and add many redundant expressions
        (SinPi, SinPi) => true,
        (CosPi, CosPi) => true,
        // asin after sinpi is identity (mod periodicity)
        // acos after cospi is identity (mod periodicity)
        // These patterns are captured by double application above

        // Exp grows too fast - double exp is almost never useful
        (Exp, Exp) => true,

        // LambertW after exp: W(e^a) = a, so W(e^x) = x
        (Exp, LambertW) => true,

        // LambertW on small values often doesn't converge usefully
        // W of reciprocal is rarely needed
        (Recip, LambertW) => true,

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
///
/// This uses a stack-based approach to identify subexpression boundaries.
/// For postfix notation, we track the stack depth to find where each
/// subexpression starts.
fn is_same_subexpr(symbols: &[Symbol], n: usize) -> bool {
    if symbols.len() < n * 2 || n < 2 {
        return false;
    }

    // Find the boundaries of the last n subexpressions on the stack
    // We need to trace backwards through the postfix to find where each
    // complete subexpression starts

    let mut stack_depths: Vec<usize> = Vec::with_capacity(symbols.len() + 1);
    stack_depths.push(0); // Initial depth

    for &sym in symbols {
        let prev_depth = *stack_depths.last().unwrap();
        let new_depth = match sym.seft() {
            Seft::A => prev_depth + 1,
            Seft::B => prev_depth, // pop 1, push 1
            Seft::C => prev_depth - 1, // pop 2, push 1
        };
        stack_depths.push(new_depth);
    }

    let final_depth = *stack_depths.last().unwrap();
    if final_depth < n {
        return false;
    }

    // Find where each of the last n subexpressions starts
    let mut subexpr_starts: Vec<usize> = Vec::with_capacity(n);
    let mut target_depth = final_depth;

    for i in (0..symbols.len()).rev() {
        if stack_depths[i] == target_depth && stack_depths[i + 1] > target_depth {
            subexpr_starts.push(i);
            target_depth -= 1;
            if subexpr_starts.len() == n {
                break;
            }
        }
    }

    if subexpr_starts.len() != n {
        return false;
    }

    // Check if all n subexpressions are identical
    // For simplicity with n=2, compare the two subexpressions
    if n == 2 && subexpr_starts.len() == 2 {
        let start1 = subexpr_starts[1]; // Earlier subexpression
        let start2 = subexpr_starts[0]; // Later subexpression
        let end1 = start2; // End of first is start of second
        let end2 = symbols.len(); // End of second is end of expression

        // Compare the symbol slices
        if end1 - start1 == end2 - start2 {
            return symbols[start1..end1] == symbols[start2..end2];
        }
    }

    false
}

/// Check if a symbol is a constant (no x)
fn is_constant(sym: Symbol) -> bool {
    matches!(sym.seft(), Seft::A) && sym != Symbol::X
}

/// Range-based pruning: prune expressions that will produce extreme values
///
/// This checks if applying a unary operator to the current expression
/// would produce values outside useful ranges, allowing early pruning
/// before evaluation.
#[allow(dead_code)]
fn should_prune_by_range(_expr: &Expression, sym: Symbol, value: f64) -> bool {
    use Symbol::*;

    match sym {
        // Sqrt of negative is invalid (returns NaN)
        Sqrt if value < 0.0 => true,

        // Ln of non-positive is invalid
        Ln if value <= 0.0 => true,

        // Exp of large values overflows
        Exp if value > 700.0 => true, // e^700 ≈ 1e304, near f64 max
        Exp if value < -700.0 => true, // e^-700 ≈ 0

        // Reciprocal of near-zero produces huge values
        Recip if value.abs() < 1e-100 => true,

        // Square of large values overflows
        Square if value.abs() > 1e150 => true,

        // LambertW is only defined for x >= -1/e
        LambertW if value < -0.36787944117144233 => true,

        // SinPi/CosPi of extreme values loses precision
        SinPi | CosPi if value.abs() > 1e15 => true,

        _ => false,
    }
}

/// Pattern-based pruning: prune expressions that match known redundant patterns
///
/// This catches higher-level patterns that span multiple operations,
/// beyond simple operator pairs.
#[allow(dead_code)]
fn should_prune_pattern(expr: &Expression, _sym: Symbol) -> bool {
    let symbols = expr.symbols();
    if symbols.is_empty() {
        return false;
    }

    // Check for patterns that indicate we're building something redundant
    // with a simpler expression

    // Count operator types - too many of one type is usually redundant
    let mut unary_count = 0;
    let mut last_unary = None;
    let mut consecutive_unary = 0;

    for &s in symbols {
        match s.seft() {
            Seft::B => {
                unary_count += 1;
                if last_unary == Some(s) {
                    consecutive_unary += 1;
                    // 3+ consecutive same unary ops is almost always redundant
                    if consecutive_unary >= 2 {
                        return true;
                    }
                } else {
                    consecutive_unary = 0;
                }
                last_unary = Some(s);
            }
            _ => {
                consecutive_unary = 0;
                last_unary = None;
            }
        }
    }

    // Too many unary operators overall makes expressions hard to interpret
    // and usually means there's a simpler equivalent
    if unary_count > 4 {
        return true;
    }

    false
}

/// Generate expressions in parallel using Rayon
#[cfg(feature = "parallel")]
pub fn generate_all_parallel(config: &GenConfig, target: f64) -> GeneratedExprs {
    use rayon::prelude::*;

    // Parallel path currently assumes shared LHS/RHS symbol sets.
    if has_rhs_symbol_overrides(config) {
        return generate_all(config, target);
    }

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
        .filter(|&sym| {
            config
                .symbol_max_counts
                .get(&sym)
                .is_none_or(|&max| max > 0)
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
            rhs_constants: None,
            rhs_unary_ops: None,
            rhs_binary_ops: None,
            symbol_max_counts: HashMap::new(),
            rhs_symbol_max_counts: None,
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

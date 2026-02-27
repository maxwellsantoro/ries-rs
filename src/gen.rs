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
use crate::symbol_table::SymbolTable;
use std::sync::Arc;

// =============================================================================
// NAMED CONSTANTS FOR QUANTIZATION AND VALUE LIMITS
// =============================================================================

/// Scale factor for quantizing floating-point values to integers.
///
/// This preserves approximately 8 significant digits, which is sufficient
/// for deduplication while avoiding overflow when converting to i64.
/// Values are quantized as: `(v * QUANTIZE_SCALE).round() as i64`
const QUANTIZE_SCALE: f64 = 1e8;

/// Maximum absolute value for quantization before using sentinel values.
///
/// Values larger than this threshold are represented by sentinel values
/// (i64::MAX - 1 for positive, i64::MIN + 1 for negative) to avoid
/// overflow during the quantization calculation.
const MAX_QUANTIZED_VALUE: f64 = 1e10;

/// Maximum absolute value for generated expressions.
///
/// Expressions with values larger than this are considered overflow-prone
/// and unlikely to be useful, so they are filtered out during generation.
const MAX_GENERATED_VALUE: f64 = 1e12;
use crate::expr::{EvaluatedExpr, Expression, MAX_EXPR_LEN};
use crate::profile::UserConstant;
use crate::symbol::{NumType, Seft, Symbol};
use crate::udf::UserFunction;
use std::collections::HashMap;

/// Configuration for expression generation
///
/// Controls which symbols are available, complexity limits,
/// and various generation options for creating candidate expressions
/// that may solve a given equation.
///
/// # Architecture
///
/// Expressions are generated in two categories:
/// - **LHS (Left-Hand Side)**: Expressions containing `x`, representing functions like `f(x)`
/// - **RHS (Right-Hand Side)**: Constant expressions not containing `x`, like `π²` or `sqrt(2)`
///
/// The generator creates all valid expressions up to the configured complexity limits,
/// then the solver finds pairs where `LHS(target) ≈ RHS`.
///
/// # Example
///
/// ```rust
/// use ries_rs::gen::GenConfig;
/// use ries_rs::symbol::Symbol;
/// use std::collections::HashMap;
///
/// let config = GenConfig {
///     max_lhs_complexity: 50,
///     max_rhs_complexity: 30,
///     max_length: 12,
///     constants: vec![Symbol::One, Symbol::Two, Symbol::Pi, Symbol::E],
///     unary_ops: vec![Symbol::Neg, Symbol::Sqrt, Symbol::Square],
///     binary_ops: vec![Symbol::Add, Symbol::Sub, Symbol::Mul, Symbol::Div],
///     ..GenConfig::default()
/// };
/// ```
#[derive(Clone)]
pub struct GenConfig {
    /// Maximum complexity score for left-hand-side expressions.
    ///
    /// LHS expressions contain `x` and represent the function side of equations.
    /// Higher values allow more complex expressions (e.g., `sin(x) + x²`), but
    /// exponentially increase search time and memory usage.
    ///
    /// Default: 128 (allows fairly complex expressions)
    pub max_lhs_complexity: u32,

    /// Maximum complexity score for right-hand-side expressions.
    ///
    /// RHS expressions are constants not containing `x`. Since they don't need
    /// to be solved for, they can typically use lower complexity limits than LHS.
    ///
    /// Default: 128
    pub max_rhs_complexity: u32,

    /// Maximum number of symbols in a single expression.
    ///
    /// This is a hard limit on expression length regardless of complexity score.
    /// Prevents pathological cases with many low-complexity symbols.
    ///
    /// Default: `MAX_EXPR_LEN` (255)
    pub max_length: usize,

    /// Symbols available for constants and variables (Seft::A type).
    ///
    /// These push a value onto the expression stack. Typically includes:
    /// - `One`, `Two`, `Three`, etc. (numeric constants)
    /// - `Pi`, `E` (mathematical constants)
    /// - `X` (the variable to solve for)
    ///
    /// Default: All built-in constants from `Symbol::constants()`
    pub constants: Vec<Symbol>,

    /// Symbols available for unary operations (Seft::B type).
    ///
    /// These transform a single value: `f(a)`. Includes operations like:
    /// - `Neg` (negation: `-a`)
    /// - `Sqrt`, `Square` (powers and roots)
    /// - `SinPi`, `CosPi` (trigonometric functions)
    /// - `Ln`, `Exp` (logarithmic and exponential)
    /// - `Recip` (reciprocal: `1/a`)
    ///
    /// Default: All built-in unary operators from `Symbol::unary_ops()`
    pub unary_ops: Vec<Symbol>,

    /// Symbols available for binary operations (Seft::C type).
    ///
    /// These combine two values: `f(a, b)`. Includes operations like:
    /// - `Add`, `Sub`, `Mul`, `Div` (arithmetic)
    /// - `Pow`, `Root`, `Log` (power functions and logarithms)
    ///
    /// Default: All built-in binary operators from `Symbol::binary_ops()`
    pub binary_ops: Vec<Symbol>,

    /// Optional override for RHS-only constant symbols.
    ///
    /// When set, RHS expressions use these symbols instead of `constants`.
    /// Useful for generating LHS with more symbols but keeping RHS simple.
    ///
    /// Default: `None` (use `constants` for both LHS and RHS)
    pub rhs_constants: Option<Vec<Symbol>>,

    /// Optional override for RHS-only unary operators.
    ///
    /// When set, RHS expressions use these operators instead of `unary_ops`.
    /// Example: allow Lambert W in LHS only, exclude from RHS constants.
    ///
    /// Default: `None` (use `unary_ops` for both LHS and RHS)
    pub rhs_unary_ops: Option<Vec<Symbol>>,

    /// Optional override for RHS-only binary operators.
    ///
    /// When set, RHS expressions use these operators instead of `binary_ops`.
    ///
    /// Default: `None` (use `binary_ops` for both LHS and RHS)
    pub rhs_binary_ops: Option<Vec<Symbol>>,

    /// Maximum usage count per symbol within a single expression.
    ///
    /// Maps each symbol to the maximum number of times it can appear.
    /// Useful for limiting redundancy (e.g., max 2 uses of `Pi`).
    /// Corresponds to the `-O` command-line option.
    ///
    /// Default: Empty (no limits)
    pub symbol_max_counts: HashMap<Symbol, u32>,

    /// Optional RHS-only symbol count limits.
    ///
    /// When set, applies different symbol count limits to RHS expressions.
    /// Corresponds to the `--O-RHS` command-line option.
    ///
    /// Default: `None` (use `symbol_max_counts` for both)
    pub rhs_symbol_max_counts: Option<HashMap<Symbol, u32>>,

    /// Minimum numeric type required for generated expressions.
    ///
    /// Filters expressions by the "sophistication" of numbers they produce:
    /// - `Integer`: Only integer results
    /// - `Rational`: Rational numbers (fractions)
    /// - `Algebraic`: Algebraic numbers (roots of polynomials)
    /// - `Transcendental`: Any real number (including π, e, trig)
    ///
    /// Lower values restrict output to simpler mathematical constructs.
    ///
    /// Default: `NumType::Transcendental` (accept all)
    pub min_num_type: NumType,

    /// Whether to generate LHS expressions containing `x`.
    ///
    /// Set to `false` if you only need constant RHS expressions.
    /// Can significantly reduce generation time when LHS is not needed.
    ///
    /// Default: `true`
    pub generate_lhs: bool,

    /// Whether to generate RHS constant expressions.
    ///
    /// Set to `false` if you only need LHS expressions.
    /// Useful for specific analysis tasks.
    ///
    /// Default: `true`
    pub generate_rhs: bool,

    /// User-defined constants for custom searches.
    ///
    /// These constants are available during expression evaluation,
    /// allowing searches involving domain-specific values.
    /// Defined via `-N` command-line option.
    ///
    /// Default: Empty
    pub user_constants: Vec<UserConstant>,

    /// User-defined functions for custom searches.
    ///
    /// Custom functions that can appear in generated expressions,
    /// extending the available operations beyond built-in symbols.
    /// Defined via `-F` command-line option.
    ///
    /// Default: Empty
    pub user_functions: Vec<UserFunction>,

    /// Enable diagnostic output for arithmetic pruning.
    ///
    /// When `true`, prints information about expressions that were
    /// discarded due to arithmetic errors (overflow, domain errors, etc.).
    /// Useful for debugging generation behavior.
    ///
    /// Default: `false`
    pub show_pruned_arith: bool,

    /// Symbol table with weights and display names.
    ///
    /// Provides complexity weights for each symbol and custom display
    /// names. Weights control how "expensive" each symbol is toward
    /// the complexity limit.
    ///
    /// Default: Empty table (uses built-in default weights)
    pub symbol_table: Arc<SymbolTable>,
}

/// Options for additional expression constraints
///
/// These constraints allow filtering expressions based on their numeric properties
/// or structural limits (like trig cycles or exponent types).
#[derive(Debug, Clone, Copy)]
pub struct ExpressionConstraintOptions {
    /// If true, power exponents must be rational (no transcendental exponents like x^pi)
    pub rational_exponents: bool,
    /// If true, trigonometric function arguments must be rational
    pub rational_trig_args: bool,
    /// Maximum number of trigonometric operations allowed in an expression
    pub max_trig_cycles: Option<u32>,
    /// Inherited numeric types for user-defined constants 0-15
    pub user_constant_types: [NumType; 16],
    /// Inherited numeric types for user-defined functions 0-15
    pub user_function_types: [NumType; 16],
}

impl Default for ExpressionConstraintOptions {
    fn default() -> Self {
        Self {
            rational_exponents: false,
            rational_trig_args: false,
            max_trig_cycles: None,
            user_constant_types: [NumType::Transcendental; 16],
            user_function_types: [NumType::Transcendental; 16],
        }
    }
}

/// Check if an expression respects the configured structural and numeric constraints.
///
/// This performs a symbolic walkthrough of the expression to verify that it
/// matches the requested properties (e.g., no transcendental exponents).
pub fn expression_respects_constraints(
    expression: &Expression,
    opts: ExpressionConstraintOptions,
) -> bool {
    #[derive(Clone, Copy)]
    struct ConstraintValue {
        has_x: bool,
        num_type: NumType,
    }

    let mut stack: Vec<ConstraintValue> = Vec::with_capacity(expression.len());
    let mut trig_ops: u32 = 0;

    for &sym in expression.symbols() {
        match sym.seft() {
            Seft::A => {
                let num_type = if let Some(idx) = sym.user_constant_index() {
                    opts.user_constant_types[idx as usize]
                } else {
                    sym.inherent_type()
                };
                stack.push(ConstraintValue {
                    has_x: sym == Symbol::X,
                    num_type,
                });
            }
            Seft::B => {
                let Some(arg) = stack.pop() else {
                    return false;
                };

                if matches!(sym, Symbol::SinPi | Symbol::CosPi | Symbol::TanPi) {
                    trig_ops = trig_ops.saturating_add(1);
                    if opts.rational_trig_args && (arg.has_x || arg.num_type < NumType::Rational) {
                        return false;
                    }
                }

                let num_type = match sym {
                    Symbol::Neg | Symbol::Square => arg.num_type,
                    Symbol::Recip => {
                        if arg.num_type >= NumType::Rational {
                            NumType::Rational
                        } else {
                            arg.num_type
                        }
                    }
                    Symbol::Sqrt => {
                        if arg.num_type >= NumType::Rational {
                            NumType::Algebraic
                        } else {
                            arg.num_type
                        }
                    }
                    Symbol::UserFunction0
                    | Symbol::UserFunction1
                    | Symbol::UserFunction2
                    | Symbol::UserFunction3
                    | Symbol::UserFunction4
                    | Symbol::UserFunction5
                    | Symbol::UserFunction6
                    | Symbol::UserFunction7
                    | Symbol::UserFunction8
                    | Symbol::UserFunction9
                    | Symbol::UserFunction10
                    | Symbol::UserFunction11
                    | Symbol::UserFunction12
                    | Symbol::UserFunction13
                    | Symbol::UserFunction14
                    | Symbol::UserFunction15 => {
                        let idx = sym.user_function_index().unwrap_or(0) as usize;
                        opts.user_function_types[idx]
                    }
                    _ => NumType::Transcendental,
                };

                stack.push(ConstraintValue {
                    has_x: arg.has_x,
                    num_type,
                });
            }
            Seft::C => {
                let Some(rhs) = stack.pop() else {
                    return false;
                };
                let Some(lhs) = stack.pop() else {
                    return false;
                };

                if opts.rational_exponents
                    && sym == Symbol::Pow
                    && (rhs.has_x || rhs.num_type < NumType::Rational)
                {
                    return false;
                }

                let num_type = match sym {
                    Symbol::Add | Symbol::Sub | Symbol::Mul => lhs.num_type.combine(rhs.num_type),
                    Symbol::Div => {
                        let combined = lhs.num_type.combine(rhs.num_type);
                        if combined == NumType::Integer {
                            NumType::Rational
                        } else {
                            combined
                        }
                    }
                    Symbol::Pow => {
                        if rhs.has_x {
                            NumType::Transcendental
                        } else if rhs.num_type == NumType::Integer {
                            lhs.num_type
                        } else if lhs.num_type >= NumType::Rational
                            && rhs.num_type >= NumType::Rational
                        {
                            NumType::Algebraic
                        } else {
                            NumType::Transcendental
                        }
                    }
                    Symbol::Root => NumType::Algebraic,
                    Symbol::Log | Symbol::Atan2 => NumType::Transcendental,
                    _ => NumType::Transcendental,
                };

                stack.push(ConstraintValue {
                    has_x: lhs.has_x || rhs.has_x,
                    num_type,
                });
            }
        }
    }

    if stack.len() != 1 {
        return false;
    }

    opts.max_trig_cycles
        .is_none_or(|max_cycles| trig_ops <= max_cycles)
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
            show_pruned_arith: false,
            symbol_table: Arc::new(SymbolTable::new()),
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
/// Key for LHS deduplication: (quantized value, quantized derivative)
pub type LhsKey = (i64, i64);

/// Uses ~8 significant digits for deduplication
#[inline]
pub fn quantize_value(v: f64) -> i64 {
    if !v.is_finite() || v.abs() > MAX_QUANTIZED_VALUE {
        // For very large values, use a different quantization to avoid overflow
        if v > MAX_QUANTIZED_VALUE {
            return i64::MAX - 1;
        } else if v < -MAX_QUANTIZED_VALUE {
            return i64::MIN + 1;
        }
        return i64::MAX;
    }
    // Scale to preserve ~8 significant digits (avoid overflow)
    (v * QUANTIZE_SCALE).round() as i64
}

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

/// Generate expressions with an early-abort limit on total count.
///
/// Returns `Some(expressions)` if generation completed within the limit,
/// or `None` if the limit was exceeded (caller should use streaming mode instead).
///
/// This is a safety mechanism to prevent OOM from unexpectedly large generation
/// at high complexity levels. The limit check happens during generation, not after.
///
/// # Arguments
///
/// * `config` - Generation configuration (complexity limits, symbols)
/// * `target` - Target value for evaluation
/// * `max_expressions` - Maximum total expressions (LHS + RHS) before aborting
///
/// # Returns
///
/// * `Some(GeneratedExprs)` - if generation completed within limit
/// * `None` - if the limit was exceeded during generation
pub fn generate_all_with_limit(
    config: &GenConfig,
    target: f64,
    max_expressions: usize,
) -> Option<GeneratedExprs> {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let count = Arc::new(AtomicUsize::new(0));
    let limit = max_expressions;

    // Collect expressions if within limit
    let mut lhs_raw = Vec::new();
    let mut rhs_raw = Vec::new();

    // Callback that counts expressions and stops when limit is hit
    let mut callbacks = StreamingCallbacks {
        on_lhs: &mut |expr| {
            let current = count.fetch_add(1, Ordering::Relaxed) + 1;
            if current > limit {
                return false; // Abort generation
            }
            lhs_raw.push(expr.clone());
            true
        },
        on_rhs: &mut |expr| {
            let current = count.fetch_add(1, Ordering::Relaxed) + 1;
            if current > limit {
                return false; // Abort generation
            }
            rhs_raw.push(expr.clone());
            true
        },
    };

    generate_streaming(config, target, &mut callbacks);

    // Check if we exceeded the limit
    let final_count = count.load(Ordering::Relaxed);
    if final_count > limit {
        return None;
    }

    // Deduplicate (same logic as generate_all)
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

    Some(GeneratedExprs {
        lhs: lhs_map.into_values().collect(),
        rhs: rhs_map.into_values().collect(),
    })
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
        if !generate_recursive_streaming(&lhs_config, target, &mut Expression::new(), 0, callbacks)
        {
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

/// Check if an evaluated expression meets generation criteria
///
/// This shared helper function is used by both batch and streaming generation
/// to validate expressions before including them in results.
#[inline]
fn should_include_expression(
    result: &crate::eval::EvalResult,
    config: &GenConfig,
    complexity: u32,
    contains_x: bool,
) -> bool {
    result.value.is_finite()
        && result.value.abs() <= MAX_GENERATED_VALUE
        && result.num_type >= config.min_num_type
        && if contains_x {
            config.generate_lhs && complexity <= config.max_lhs_complexity
        } else {
            config.generate_rhs && complexity <= config.max_rhs_complexity
        }
}

/// Calculate the appropriate complexity limit based on whether expression contains x
///
/// For expressions containing x, uses LHS limit.
/// For RHS-only paths, uses RHS limit.
/// For paths that might still add x, uses the max of both limits.
#[inline]
fn get_max_complexity(config: &GenConfig, contains_x: bool) -> u32 {
    if contains_x {
        config.max_lhs_complexity
    } else {
        // For RHS-only paths, use RHS limit
        // For paths that might still add x, use the max of both
        std::cmp::max(config.max_lhs_complexity, config.max_rhs_complexity)
    }
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
        match evaluate_fast_with_constants_and_functions(
            current,
            target,
            &config.user_constants,
            &config.user_functions,
        ) {
            Ok(result) => {
                // Use shared validation helper
                if should_include_expression(
                    &result,
                    config,
                    current.complexity(),
                    current.contains_x(),
                ) {
                    let expr = current.clone();
                    let eval_expr =
                        EvaluatedExpr::new(expr, result.value, result.derivative, result.num_type);

                    // Call the appropriate callback; return false if it signals stop
                    let should_continue = if current.contains_x() {
                        (callbacks.on_lhs)(&eval_expr)
                    } else {
                        (callbacks.on_rhs)(&eval_expr)
                    };
                    if !should_continue {
                        return false;
                    }
                }
            }
            Err(e) => {
                // Expression was pruned due to arithmetic error
                if config.show_pruned_arith {
                    eprintln!(
                        "  [pruned arith] expression=\"{}\" reason={:?}",
                        current.to_postfix(),
                        e
                    );
                }
            }
        }
    }

    // Check limits before recursing
    if current.len() >= config.max_length {
        return true;
    }

    // Use shared helper for complexity limit calculation
    let max_complexity = get_max_complexity(config, current.contains_x());

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
        let sym_weight = config.symbol_table.weight(sym);
        if current.complexity() + sym_weight > max_complexity {
            continue;
        }
        if exceeds_symbol_limit(config, current, sym) {
            continue;
        }

        // Skip x if we only want RHS
        if sym == Symbol::X && !config.generate_lhs {
            continue;
        }

        current.push_with_table(sym, &config.symbol_table);
        if !generate_recursive_streaming(config, target, current, stack_depth + 1, callbacks) {
            current.pop_with_table(&config.symbol_table);
            return false;
        }
        current.pop_with_table(&config.symbol_table);
    }

    // Also add x for LHS generation
    if config.generate_lhs && !config.constants.contains(&Symbol::X) {
        let sym = Symbol::X;
        let sym_weight = config.symbol_table.weight(sym);
        if current.complexity() + sym_weight <= max_complexity
            && !exceeds_symbol_limit(config, current, sym)
        {
            current.push_with_table(sym, &config.symbol_table);
            if !generate_recursive_streaming(config, target, current, stack_depth + 1, callbacks) {
                current.pop_with_table(&config.symbol_table);
                return false;
            }
            current.pop_with_table(&config.symbol_table);
        }
    }

    // Unary operators (Seft::B) - need at least 1 on stack
    if stack_depth >= 1 {
        for &sym in &config.unary_ops {
            let sym_weight = config.symbol_table.weight(sym);
            if current.complexity() + sym_weight > max_complexity {
                continue;
            }
            if exceeds_symbol_limit(config, current, sym) {
                continue;
            }

            // Apply pruning rules
            if should_prune_unary(current, sym) {
                continue;
            }

            current.push_with_table(sym, &config.symbol_table);
            if !generate_recursive_streaming(config, target, current, stack_depth, callbacks) {
                current.pop_with_table(&config.symbol_table);
                return false;
            }
            current.pop_with_table(&config.symbol_table);
        }
    }

    // Binary operators (Seft::C) - need at least 2 on stack
    if stack_depth >= 2 {
        for &sym in &config.binary_ops {
            let sym_weight = config.symbol_table.weight(sym);
            if current.complexity() + sym_weight > max_complexity {
                continue;
            }
            if exceeds_symbol_limit(config, current, sym) {
                continue;
            }

            // Apply pruning rules
            if should_prune_binary(current, sym) {
                continue;
            }

            current.push_with_table(sym, &config.symbol_table);
            if !generate_recursive_streaming(config, target, current, stack_depth - 1, callbacks) {
                current.pop_with_table(&config.symbol_table);
                return false;
            }
            current.pop_with_table(&config.symbol_table);
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
        match evaluate_fast_with_constants_and_functions(
            current,
            target,
            &config.user_constants,
            &config.user_functions,
        ) {
            Ok(result) => {
                // Use shared validation helper
                if should_include_expression(
                    &result,
                    config,
                    current.complexity(),
                    current.contains_x(),
                ) {
                    let expr = current.clone();
                    let eval_expr =
                        EvaluatedExpr::new(expr, result.value, result.derivative, result.num_type);

                    // Keep all LHS expressions; derivative≈0 cases handled in search
                    if current.contains_x() {
                        lhs_out.push(eval_expr);
                    } else {
                        rhs_out.push(eval_expr);
                    }
                }
            }
            Err(e) => {
                // Expression was pruned due to arithmetic error
                if config.show_pruned_arith {
                    eprintln!(
                        "  [pruned arith] expression=\"{}\" reason={:?}",
                        current.to_postfix(),
                        e
                    );
                }
            }
        }
    }

    // Check limits before recursing
    if current.len() >= config.max_length {
        return;
    }

    // Use shared helper for complexity limit calculation
    let max_complexity = get_max_complexity(config, current.contains_x());

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
        let sym_weight = config.symbol_table.weight(sym);
        if current.complexity() + sym_weight > max_complexity {
            continue;
        }
        if exceeds_symbol_limit(config, current, sym) {
            continue;
        }

        // Skip x if we only want RHS
        if sym == Symbol::X && !config.generate_lhs {
            continue;
        }

        current.push_with_table(sym, &config.symbol_table);
        generate_recursive(config, target, current, stack_depth + 1, lhs_out, rhs_out);
        current.pop_with_table(&config.symbol_table);
    }

    // Also add x for LHS generation
    if config.generate_lhs && !config.constants.contains(&Symbol::X) {
        let sym = Symbol::X;
        let sym_weight = config.symbol_table.weight(sym);
        if current.complexity() + sym_weight <= max_complexity
            && !exceeds_symbol_limit(config, current, sym)
        {
            current.push_with_table(sym, &config.symbol_table);
            generate_recursive(config, target, current, stack_depth + 1, lhs_out, rhs_out);
            current.pop_with_table(&config.symbol_table);
        }
    }

    // Unary operators (Seft::B) - need at least 1 on stack
    if stack_depth >= 1 {
        for &sym in &config.unary_ops {
            let sym_weight = config.symbol_table.weight(sym);
            if current.complexity() + sym_weight > max_complexity {
                continue;
            }
            if exceeds_symbol_limit(config, current, sym) {
                continue;
            }

            // Apply pruning rules
            if should_prune_unary(current, sym) {
                continue;
            }

            current.push_with_table(sym, &config.symbol_table);
            generate_recursive(config, target, current, stack_depth, lhs_out, rhs_out);
            current.pop_with_table(&config.symbol_table);
        }
    }

    // Binary operators (Seft::C) - need at least 2 on stack
    if stack_depth >= 2 {
        for &sym in &config.binary_ops {
            let sym_weight = config.symbol_table.weight(sym);
            if current.complexity() + sym_weight > max_complexity {
                continue;
            }
            if exceeds_symbol_limit(config, current, sym) {
                continue;
            }

            // Apply pruning rules
            if should_prune_binary(current, sym) {
                continue;
            }

            current.push_with_table(sym, &config.symbol_table);
            generate_recursive(config, target, current, stack_depth - 1, lhs_out, rhs_out);
            current.pop_with_table(&config.symbol_table);
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
        .map(|s| config.symbol_table.weight(*s))
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
            Seft::B => prev_depth,     // pop 1, push 1
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

/// Generate expressions in parallel using Rayon
#[cfg(feature = "parallel")]
pub fn generate_all_parallel(config: &GenConfig, target: f64) -> GeneratedExprs {
    use rayon::prelude::*;

    // Parallel path currently assumes shared LHS/RHS symbol sets.
    if has_rhs_symbol_overrides(config) {
        return generate_all(config, target);
    }

    // Generate valid prefixes of length 1 and 2 to create smaller,
    // more evenly distributed tasks for Rayon to schedule.
    let mut prefixes: Vec<(Expression, usize)> = Vec::new();
    let mut immediate_results_lhs = Vec::new();
    let mut immediate_results_rhs = Vec::new();

    let first_symbols: Vec<Symbol> = config
        .constants
        .iter()
        .copied()
        .chain(
            if config.generate_lhs && !config.constants.contains(&Symbol::X) {
                Some(Symbol::X)
            } else {
                None
            },
        )
        .filter(|&sym| {
            config
                .symbol_max_counts
                .get(&sym)
                .is_none_or(|&max| max > 0)
        })
        .collect();

    for sym1 in first_symbols {
        let mut expr1 = Expression::new();
        expr1.push_with_table(sym1, &config.symbol_table);

        let max_complexity = if expr1.contains_x() {
            config.max_lhs_complexity
        } else {
            std::cmp::max(config.max_lhs_complexity, config.max_rhs_complexity)
        };

        if expr1.complexity() > max_complexity {
            continue;
        }

        // 1. Evaluate length-1 prefix (simulate top of generate_recursive)
        if let Ok(result) = evaluate_fast_with_constants_and_functions(
            &expr1,
            target,
            &config.user_constants,
            &config.user_functions,
        ) {
            if result.value.is_finite()
                && result.value.abs() <= MAX_GENERATED_VALUE
                && result.num_type >= config.min_num_type
            {
                let eval_expr = EvaluatedExpr::new(
                    expr1.clone(),
                    result.value,
                    result.derivative,
                    result.num_type,
                );

                if expr1.contains_x() {
                    if config.generate_lhs && expr1.complexity() <= config.max_lhs_complexity {
                        immediate_results_lhs.push(eval_expr);
                    }
                } else if config.generate_rhs && expr1.complexity() <= config.max_rhs_complexity {
                    immediate_results_rhs.push(eval_expr);
                }
            }
        }

        if expr1.len() >= config.max_length {
            continue;
        }

        // 2. Add next symbols (simulate bottom of generate_recursive)

        // Constants (+1 stack)
        let mut next_constants = config.constants.clone();
        if config.generate_lhs && !next_constants.contains(&Symbol::X) {
            next_constants.push(Symbol::X);
        }

        for &sym2 in &next_constants {
            let sym2_weight = config.symbol_table.weight(sym2);
            let next_max = if expr1.contains_x() || sym2 == Symbol::X {
                config.max_lhs_complexity
            } else {
                std::cmp::max(config.max_lhs_complexity, config.max_rhs_complexity)
            };

            if expr1.complexity() + sym2_weight <= next_max
                && !exceeds_symbol_limit(config, &expr1, sym2)
            {
                let mut expr2 = expr1.clone();
                expr2.push_with_table(sym2, &config.symbol_table);
                // Min complexity check: for stack depth 2, we need at least 1 binary op
                let min_remaining = min_complexity_to_complete(2, config);
                if expr2.complexity() + min_remaining <= next_max {
                    prefixes.push((expr2, 2));
                }
            }
        }

        // Unary ops (+0 stack)
        for &sym2 in &config.unary_ops {
            let sym2_weight = config.symbol_table.weight(sym2);
            if expr1.complexity() + sym2_weight <= max_complexity
                && !exceeds_symbol_limit(config, &expr1, sym2)
                && !should_prune_unary(&expr1, sym2)
            {
                let mut expr2 = expr1.clone();
                expr2.push_with_table(sym2, &config.symbol_table);
                let min_remaining = min_complexity_to_complete(1, config);
                if expr2.complexity() + min_remaining <= max_complexity {
                    prefixes.push((expr2, 1));
                }
            }
        }
    }

    let results: Vec<(Vec<EvaluatedExpr>, Vec<EvaluatedExpr>)> = prefixes
        .into_par_iter()
        .map(|(mut expr, depth)| {
            let mut lhs = Vec::new();
            let mut rhs = Vec::new();
            generate_recursive(config, target, &mut expr, depth, &mut lhs, &mut rhs);
            (lhs, rhs)
        })
        .collect();

    // Merge results
    let mut lhs_raw = immediate_results_lhs;
    let mut rhs_raw = immediate_results_rhs;
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
                Symbol::One,
                Symbol::Two,
                Symbol::Three,
                Symbol::Four,
                Symbol::Five,
                Symbol::Pi,
                Symbol::E,
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
            show_pruned_arith: false,
            symbol_table: Arc::new(SymbolTable::new()),
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

    #[test]
    fn test_generate_all_with_limit_aborts_when_exceeded() {
        // Config with high complexity that will generate many expressions.
        // With new calibrated weights, even moderate complexity can generate 100+ expressions.
        let mut config = fast_test_config();
        config.max_lhs_complexity = 30;
        config.max_rhs_complexity = 30;

        // First, check how many expressions would be generated without limit.
        let unlimited = generate_all(&config, 2.5);
        let total_unlimited = unlimited.lhs.len() + unlimited.rhs.len();

        // The test only makes sense if we'd generate more than a handful.
        assert!(
            total_unlimited > 10,
            "Test config should generate >10 expressions"
        );

        // Now test with a limit less than the actual count — should return None.
        let limit = total_unlimited / 2; // Set limit to half of what would be generated
        let result = generate_all_with_limit(&config, 2.5, limit);

        assert!(
            result.is_none(),
            "generate_all_with_limit should return None when limit ({}) is exceeded (actual: {})",
            limit,
            total_unlimited
        );
    }

    #[test]
    fn test_generate_all_with_limit_succeeds_when_within_limit() {
        // Same config but with a generous limit that won't be hit.
        let mut config = fast_test_config();
        config.max_lhs_complexity = 30;
        config.max_rhs_complexity = 30;

        // Set limit much higher than expected expression count.
        let result = generate_all_with_limit(&config, 2.5, 10_000);

        assert!(
            result.is_some(),
            "generate_all_with_limit should return Some when limit is not exceeded"
        );

        let generated = result.unwrap();
        // Should have generated some expressions.
        assert!(!generated.lhs.is_empty() || !generated.rhs.is_empty());
    }

    // ==================== expression_respects_constraints tests ====================

    fn expr_from_postfix(s: &str) -> Expression {
        Expression::parse(s).expect("valid expression")
    }

    #[test]
    fn test_constraints_default_allows_all() {
        let opts = ExpressionConstraintOptions::default();

        // x^pi should be allowed with default options
        let expr = expr_from_postfix("xp^"); // x^pi
        assert!(
            expression_respects_constraints(&expr, opts),
            "x^pi should be allowed with default options"
        );

        // sinpi(e) should be allowed
        let expr = expr_from_postfix("eS"); // e then sinpi (S = SinPi)
        assert!(
            expression_respects_constraints(&expr, opts),
            "sinpi(e) should be allowed with default options"
        );
    }

    #[test]
    fn test_constraints_rational_exponents_rejects_transcendental() {
        let opts = ExpressionConstraintOptions {
            rational_exponents: true,
            ..Default::default()
        };

        // x^pi should be rejected (pi is transcendental)
        let expr = expr_from_postfix("xp^");
        assert!(
            !expression_respects_constraints(&expr, opts),
            "x^pi should be rejected with rational_exponents=true"
        );

        // x^e should be rejected
        let expr = expr_from_postfix("xe^");
        assert!(
            !expression_respects_constraints(&expr, opts),
            "x^e should be rejected with rational_exponents=true"
        );
    }

    #[test]
    fn test_constraints_rational_exponents_allows_integer() {
        let opts = ExpressionConstraintOptions {
            rational_exponents: true,
            ..Default::default()
        };

        // x^2 should be allowed (2 is integer)
        let expr = expr_from_postfix("x2^");
        assert!(
            expression_respects_constraints(&expr, opts),
            "x^2 should be allowed with rational_exponents=true"
        );

        // x^1 should be allowed
        let expr = expr_from_postfix("x1^");
        assert!(
            expression_respects_constraints(&expr, opts),
            "x^1 should be allowed with rational_exponents=true"
        );
    }

    #[test]
    fn test_constraints_rational_trig_args_rejects_irrational() {
        let opts = ExpressionConstraintOptions {
            rational_trig_args: true,
            ..Default::default()
        };

        // sinpi(e) should be rejected (e is irrational/transcendental)
        let expr = expr_from_postfix("eS"); // e then sinpi (S = SinPi)
        assert!(
            !expression_respects_constraints(&expr, opts),
            "sinpi(e) should be rejected with rational_trig_args=true"
        );

        // sinpi(pi) should be rejected (pi is transcendental)
        let expr = expr_from_postfix("pS"); // pi then sinpi
        assert!(
            !expression_respects_constraints(&expr, opts),
            "sinpi(pi) should be rejected with rational_trig_args=true"
        );
    }

    #[test]
    fn test_constraints_rational_trig_args_allows_rational() {
        let opts = ExpressionConstraintOptions {
            rational_trig_args: true,
            ..Default::default()
        };

        // sinpi(1) should be allowed (1 is integer, hence rational)
        let expr = expr_from_postfix("1S"); // 1 then sinpi (S = SinPi)
        assert!(
            expression_respects_constraints(&expr, opts),
            "sinpi(1) should be allowed with rational_trig_args=true"
        );

        // sinpi(2) should be allowed
        let expr = expr_from_postfix("2S");
        assert!(
            expression_respects_constraints(&expr, opts),
            "sinpi(2) should be allowed with rational_trig_args=true"
        );
    }

    #[test]
    fn test_constraints_rational_trig_args_rejects_x() {
        let opts = ExpressionConstraintOptions {
            rational_trig_args: true,
            ..Default::default()
        };

        // sinpi(x) should be rejected (x is not a constant rational)
        let expr = expr_from_postfix("xS"); // x then sinpi (S = SinPi)
        assert!(
            !expression_respects_constraints(&expr, opts),
            "sinpi(x) should be rejected with rational_trig_args=true"
        );
    }

    #[test]
    fn test_constraints_max_trig_cycles() {
        let opts = ExpressionConstraintOptions {
            max_trig_cycles: Some(2),
            ..Default::default()
        };

        // Single trig: sinpi(x) - should pass
        let expr = expr_from_postfix("xS"); // x then sinpi (S = SinPi)
        assert!(
            expression_respects_constraints(&expr, opts),
            "1 trig op should pass with max=2"
        );

        // Double nested: sinpi(cospi(x)) - should pass
        // x C S = sinpi(cospi(x)) where C = CosPi, S = SinPi
        let expr = expr_from_postfix("xCS");
        assert!(
            expression_respects_constraints(&expr, opts),
            "2 trig ops should pass with max=2"
        );

        // Triple nested: sinpi(cospi(tanpi(x))) - should fail
        // x T C S = sinpi(cospi(tanpi(x))) where T = TanPi
        let expr = expr_from_postfix("xTCS");
        assert!(
            !expression_respects_constraints(&expr, opts),
            "3 trig ops should fail with max=2"
        );
    }

    #[test]
    fn test_constraints_max_trig_cycles_none_unlimited() {
        let opts = ExpressionConstraintOptions {
            max_trig_cycles: None, // No limit
            ..Default::default()
        };

        // Even deeply nested trig should pass
        // x T C S T C S = 6 trig ops
        let expr = expr_from_postfix("xTCSTCS");
        assert!(
            expression_respects_constraints(&expr, opts),
            "Unlimited trig should pass any depth"
        );
    }

    #[test]
    fn test_constraints_combined() {
        let opts = ExpressionConstraintOptions {
            rational_exponents: true,
            rational_trig_args: true,
            max_trig_cycles: Some(1),
            ..Default::default()
        };

        // x^2 + sinpi(1) should pass
        let expr = expr_from_postfix("x2^1S+"); // S = SinPi
        assert!(
            expression_respects_constraints(&expr, opts),
            "x^2 + sinpi(1) should pass all constraints"
        );

        // x^pi should fail (rational_exponents)
        let expr = expr_from_postfix("xp^");
        assert!(
            !expression_respects_constraints(&expr, opts),
            "x^pi should fail rational_exponents"
        );

        // sinpi(x) should fail (rational_trig_args)
        let expr = expr_from_postfix("xS"); // S = SinPi
        assert!(
            !expression_respects_constraints(&expr, opts),
            "sinpi(x) should fail rational_trig_args"
        );

        // sinpi(cospi(1)) should fail (max_trig_cycles)
        let expr = expr_from_postfix("1CS"); // C = CosPi, S = SinPi
        assert!(
            !expression_respects_constraints(&expr, opts),
            "double trig should fail max_trig_cycles=1"
        );
    }

    #[test]
    fn test_constraints_malformed_expression() {
        let opts = ExpressionConstraintOptions::default();

        // Expression that would cause stack underflow
        let expr = expr_from_postfix("+"); // Just a binary op
        assert!(
            !expression_respects_constraints(&expr, opts),
            "Malformed expression should return false"
        );

        // Incomplete expression (too many values)
        let expr = expr_from_postfix("12");
        assert!(
            !expression_respects_constraints(&expr, opts),
            "Incomplete expression should return false"
        );
    }

    #[test]
    fn test_constraints_user_constant_types() {
        // Set user constant 0 to be Integer type
        let mut user_types = [NumType::Transcendental; 16];
        user_types[0] = NumType::Integer;

        let opts = ExpressionConstraintOptions {
            rational_exponents: true,
            user_constant_types: user_types,
            ..Default::default()
        };

        // If UserConstant0 is treated as Integer, x^UserConstant0 should be allowed
        // (We can't easily test this without actually having user constants in the expression,
        // but this verifies the options struct is properly configured)
        assert_eq!(opts.user_constant_types[0], NumType::Integer);
    }
}

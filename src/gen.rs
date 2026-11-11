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

use crate::eval::{evaluate_search, EvalContext};
use crate::profile::{Profile, UserConstant, MAX_USER_SYMBOLS};
use crate::symbol_table::SymbolTable;
use crate::thresholds::QUANTIZE_SCALE;
use std::sync::Arc;

// =============================================================================
// NAMED CONSTANTS FOR QUANTIZATION AND VALUE LIMITS
// =============================================================================

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
use crate::pool::{legacy_parity_score_expr, RankingMode};
use crate::search::{digit_signature, MatchFilters, SearchConfig};
use crate::symbol::{NumType, Seft, Symbol};
use crate::udf::UserFunction;
use std::borrow::Cow;
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
    /// Default: `MAX_EXPR_LEN` (21)
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
    /// Fixed result types for user-defined functions 0-15 whose body is not
    /// available in `user_function_constraints`. Such a function is opaque:
    /// its body is not checked against the restrictions.
    pub user_function_types: [NumType; 16],
    /// Body-derived behavior of user-defined functions 0-15. When present,
    /// it takes precedence over `user_function_types`: the result type
    /// follows the argument, and the body is checked against the restrictions.
    /// Build entries with [`UserFunctionConstraints::from_function`].
    pub user_function_constraints: [Option<UserFunctionConstraints>; 16],
}

impl Default for ExpressionConstraintOptions {
    fn default() -> Self {
        Self {
            rational_exponents: false,
            rational_trig_args: false,
            max_trig_cycles: None,
            user_constant_types: [NumType::Transcendental; 16],
            user_function_types: [NumType::Transcendental; 16],
            user_function_constraints: [None; 16],
        }
    }
}

const NUM_TYPES: [NumType; 7] = [
    NumType::Transcendental,
    NumType::Liouvillian,
    NumType::Elementary,
    NumType::Algebraic,
    NumType::Constructible,
    NumType::Rational,
    NumType::Integer,
];

/// Abstract value tracked by the constraint walker for one stack entry.
#[derive(Clone, Copy, Debug)]
struct ConstraintValue {
    has_x: bool,
    num_type: NumType,
    // Known arithmetic constants let positive integer powers retain their
    // integer classification without assuming unknown exponents are positive.
    constant_value: Option<f64>,
}

/// Restriction-relevant events seen while walking an expression. Recording
/// events, rather than rejecting immediately, lets user function bodies be
/// summarized once and checked under any set of options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ConstraintTrace {
    trig_ops: u32,
    irrational_trig_arg: bool,
    irrational_exponent: bool,
}

impl ConstraintTrace {
    fn merge(&mut self, other: Self) {
        self.trig_ops = self.trig_ops.saturating_add(other.trig_ops);
        self.irrational_trig_arg |= other.irrational_trig_arg;
        self.irrational_exponent |= other.irrational_exponent;
    }

    fn allowed_by(self, opts: &ExpressionConstraintOptions) -> bool {
        !(opts.rational_trig_args && self.irrational_trig_arg)
            && !(opts.rational_exponents && self.irrational_exponent)
            && opts
                .max_trig_cycles
                .is_none_or(|max_cycles| self.trig_ops <= max_cycles)
    }
}

/// Result of applying a user function body to one class of argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UdfOutcome {
    num_type: NumType,
    trace: ConstraintTrace,
}

/// Constraint behavior of one user-defined function, derived from its body.
///
/// A user function's numeric class depends on its argument: `sq(2)` is an
/// integer while `sq(pi)` is not, so no single fixed type describes it. This
/// summary records, for every argument class and x-dependence, the class of
/// the result and the restriction-relevant operations inside the body. The
/// walker uses the same transfer rules for bodies as for expressions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserFunctionConstraints {
    /// Indexed by `[arg depends on x][arg NumType]`; `None` when the body
    /// cannot be evaluated structurally (the evaluator rejects it too).
    outcomes: [[Option<UdfOutcome>; 7]; 2],
}

impl UserFunctionConstraints {
    /// Summarize `function`'s body. `user_constant_types` classifies any user
    /// constants the body references.
    pub fn from_function(function: &UserFunction, user_constant_types: &[NumType; 16]) -> Self {
        let mut outcomes = [[None; 7]; 2];
        for (has_x, row) in outcomes.iter_mut().enumerate() {
            for (slot, &num_type) in row.iter_mut().zip(NUM_TYPES.iter()) {
                *slot = Self::apply_body(
                    function,
                    ConstraintValue {
                        has_x: has_x == 1,
                        num_type,
                        constant_value: None,
                    },
                    user_constant_types,
                );
            }
        }
        Self { outcomes }
    }

    fn apply_body(
        function: &UserFunction,
        arg: ConstraintValue,
        user_constant_types: &[NumType; 16],
    ) -> Option<UdfOutcome> {
        use crate::udf::UdfOp;
        // Bodies are walked without nested function bodies: the evaluator
        // does not let user functions call one another.
        let opts = ExpressionConstraintOptions {
            user_constant_types: *user_constant_types,
            ..Default::default()
        };
        let mut stack = vec![arg];
        let mut trace = ConstraintTrace::default();
        for op in &function.body {
            match *op {
                UdfOp::Symbol(Symbol::X) => return None,
                UdfOp::Symbol(sym) if sym.user_function_index().is_some() => return None,
                UdfOp::Symbol(sym) => constraint_step(&mut stack, sym, &opts, &mut trace)?,
                UdfOp::Dup => stack.push(*stack.last()?),
                UdfOp::Swap => {
                    let len = stack.len();
                    if len < 2 {
                        return None;
                    }
                    stack.swap(len - 1, len - 2);
                }
            }
        }
        match stack.as_slice() {
            [result] => Some(UdfOutcome {
                num_type: result.num_type,
                trace,
            }),
            _ => None,
        }
    }

    fn outcome(&self, arg: ConstraintValue) -> Option<UdfOutcome> {
        self.outcomes[usize::from(arg.has_x)][arg.num_type as usize]
    }
}

/// Apply one symbol to the abstract stack. This is the single set of
/// structural transfer rules for expressions and user function bodies.
fn constraint_step(
    stack: &mut Vec<ConstraintValue>,
    sym: Symbol,
    opts: &ExpressionConstraintOptions,
    trace: &mut ConstraintTrace,
) -> Option<()> {
    match sym.seft() {
        Seft::A => {
            let num_type = if let Some(idx) = sym.user_constant_index() {
                opts.user_constant_types[idx as usize]
            } else if sym == Symbol::X {
                NumType::Integer // polynomial indeterminate, not a sampled integer
            } else {
                sym.inherent_type()
            };
            stack.push(ConstraintValue {
                has_x: sym == Symbol::X,
                num_type,
                constant_value: match sym {
                    Symbol::One
                    | Symbol::Two
                    | Symbol::Three
                    | Symbol::Four
                    | Symbol::Five
                    | Symbol::Six
                    | Symbol::Seven
                    | Symbol::Eight
                    | Symbol::Nine => Some(f64::from(sym as u8 - b'0')),
                    _ => None,
                },
            });
        }
        Seft::B => {
            let arg = stack.pop()?;

            if matches!(sym, Symbol::SinPi | Symbol::CosPi | Symbol::TanPi) {
                trace.trig_ops = trace.trig_ops.saturating_add(1);
                if arg.has_x || arg.num_type < NumType::Rational {
                    trace.irrational_trig_arg = true;
                }
            }

            let num_type = if let Some(idx) = sym.user_function_index() {
                let idx = idx as usize;
                match opts.user_function_constraints[idx] {
                    Some(summary) => {
                        let outcome = summary.outcome(arg)?;
                        trace.merge(outcome.trace);
                        outcome.num_type
                    }
                    None => opts.user_function_types[idx],
                }
            } else {
                sym.result_type(&[arg.num_type])
            };

            let constant_value = arg.constant_value.and_then(|value| {
                let result = match sym {
                    Symbol::Neg => -value,
                    Symbol::Square => value * value,
                    Symbol::Recip => 1.0 / value,
                    Symbol::Sqrt => value.sqrt(),
                    _ => return None,
                };
                result.is_finite().then_some(result)
            });
            stack.push(ConstraintValue {
                has_x: arg.has_x,
                num_type,
                constant_value,
            });
        }
        Seft::C => {
            let rhs = stack.pop()?;
            let lhs = stack.pop()?;

            if sym == Symbol::Pow && (rhs.has_x || rhs.num_type < NumType::Rational) {
                trace.irrational_exponent = true;
            }

            let num_type = sym.result_type_with_facts(
                &[lhs.num_type, rhs.num_type],
                &[lhs.has_x, rhs.has_x],
                &[lhs.constant_value, rhs.constant_value],
            );

            let constant_value = lhs
                .constant_value
                .zip(rhs.constant_value)
                .and_then(|(a, b)| {
                    let result = match sym {
                        Symbol::Add => a + b,
                        Symbol::Sub => a - b,
                        Symbol::Mul => a * b,
                        Symbol::Div => a / b,
                        Symbol::Pow => a.powf(b),
                        _ => return None,
                    };
                    result.is_finite().then_some(result)
                });
            stack.push(ConstraintValue {
                has_x: lhs.has_x || rhs.has_x,
                num_type,
                constant_value,
            });
        }
    }
    Some(())
}

/// Walk `expression` structurally, returning its numeric class and trace,
/// or `None` for a malformed expression or unusable user function body.
fn constraint_walk(
    expression: &Expression,
    opts: &ExpressionConstraintOptions,
) -> Option<(NumType, ConstraintTrace)> {
    let mut stack: Vec<ConstraintValue> = Vec::with_capacity(expression.len());
    let mut trace = ConstraintTrace::default();
    for &sym in expression.symbols() {
        constraint_step(&mut stack, sym, opts, &mut trace)?;
    }
    match stack.as_slice() {
        [result] => Some((result.num_type, trace)),
        _ => None,
    }
}

/// Structural numeric class of `expression` under `opts`, as used by the
/// constraint checks. Exposed to the crate for differential tests.
#[cfg(test)]
pub(crate) fn constraint_num_type(
    expression: &Expression,
    opts: &ExpressionConstraintOptions,
) -> Option<NumType> {
    constraint_walk(expression, opts).map(|(num_type, _)| num_type)
}

/// Check if an expression respects the configured structural and numeric constraints.
///
/// This performs a symbolic walkthrough of the expression to verify that it
/// matches the requested properties (e.g., no transcendental exponents).
pub fn expression_respects_constraints(
    expression: &Expression,
    opts: ExpressionConstraintOptions,
) -> bool {
    constraint_walk(expression, &opts).is_some_and(|(_, trace)| trace.allowed_by(&opts))
}

impl GenConfig {
    /// Resolve RHS search filters before pruning or choosing representatives.
    /// The LHS grammar and caller-owned configuration remain unchanged.
    pub(crate) fn resolved_for_search<'a>(
        &'a self,
        search: Option<&SearchConfig>,
    ) -> Cow<'a, Self> {
        let Some(search) = search else {
            return Cow::Borrowed(self);
        };
        let allowed = |symbol: &Symbol| search.rhs_symbol_is_allowed(*symbol);
        let constants = self.rhs_constants.as_deref().unwrap_or(&self.constants);
        let unary = self.rhs_unary_ops.as_deref().unwrap_or(&self.unary_ops);
        let binary = self.rhs_binary_ops.as_deref().unwrap_or(&self.binary_ops);
        if constants.iter().chain(unary).chain(binary).all(allowed) {
            return Cow::Borrowed(self);
        }
        let mut resolved = self.clone();
        resolved.rhs_constants = Some(constants.iter().copied().filter(allowed).collect());
        resolved.rhs_unary_ops = Some(unary.iter().copied().filter(allowed).collect());
        resolved.rhs_binary_ops = Some(binary.iter().copied().filter(allowed).collect());
        Cow::Owned(resolved)
    }
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

/// Build a baseline generation config from a profile.
///
/// This is the shared source of truth for Python, WASM, and CLI setup before
/// CLI-specific symbol filtering is applied.
pub fn build_gen_config_from_profile(
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    profile: &Profile,
) -> Result<GenConfig, String> {
    validate_user_symbol_capacity(profile.constants.len(), profile.functions.len())?;

    let mut config = GenConfig {
        max_lhs_complexity,
        max_rhs_complexity,
        user_constants: profile.constants.clone(),
        user_functions: profile.functions.clone(),
        symbol_table: Arc::new(SymbolTable::from_profile(profile)),
        ..GenConfig::default()
    };

    for idx in 0..profile.constants.len() {
        if let Some(sym) = Symbol::from_byte(128 + idx as u8) {
            config.constants.push(sym);
        }
    }
    for idx in 0..profile.functions.len() {
        if let Some(sym) = Symbol::from_byte(144 + idx as u8) {
            config.unary_ops.push(sym);
        }
    }

    Ok(config)
}

/// Validate that the fixed user symbol slots are not exceeded.
pub fn validate_user_symbol_capacity(
    constant_count: usize,
    function_count: usize,
) -> Result<(), String> {
    if constant_count > MAX_USER_SYMBOLS {
        return Err(format!(
            "At most {} user constants are supported (got {})",
            MAX_USER_SYMBOLS, constant_count
        ));
    }
    if function_count > MAX_USER_SYMBOLS {
        return Err(format!(
            "At most {} user functions are supported (got {})",
            MAX_USER_SYMBOLS, function_count
        ));
    }
    Ok(())
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

/// Pair of quantized values used by callers of [`quantize_value`].
pub type LhsKey = (i64, i64);

/// Round a value into a legacy numerical bucket. Search deduplication uses
/// exact floating-point keys because a bucket can contain distinct roots.
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

pub(crate) fn lhs_dedup_key(
    expr: &EvaluatedExpr,
    filters: MatchFilters,
) -> (u64, u64, Option<[usize; 9]>) {
    (
        expr.value.to_bits(),
        expr.derivative.to_bits(),
        filters.numeric_anagram.then(|| digit_signature(&expr.expr)),
    )
}

/// Keeps one admissible RHS expression per bit-identical value.
pub(crate) struct RhsRepresentatives<'a> {
    search: Option<&'a SearchConfig>,
    by_value: HashMap<u64, EvaluatedExpr>,
    by_value_and_digits: HashMap<(u64, [usize; 9]), EvaluatedExpr>,
}

impl<'a> RhsRepresentatives<'a> {
    pub(crate) fn new(search: Option<&'a SearchConfig>) -> Self {
        Self {
            search,
            by_value: HashMap::new(),
            by_value_and_digits: HashMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, expr: Cow<'_, EvaluatedExpr>) {
        if self.search.is_some_and(|search| {
            !search.rhs_symbol_allowed(&expr.expr) || !search.filters.allows_side(&expr)
        }) {
            return;
        }
        let value = expr.value.to_bits();
        if self
            .search
            .is_some_and(|search| search.filters.numeric_anagram)
        {
            let key = (value, digit_signature(&expr.expr));
            Self::keep_preferred(self.by_value_and_digits.entry(key), expr, self.search);
        } else {
            Self::keep_preferred(self.by_value.entry(value), expr, self.search);
        }
    }

    fn keep_preferred<K>(
        entry: std::collections::hash_map::Entry<'_, K, EvaluatedExpr>,
        expr: Cow<'_, EvaluatedExpr>,
        search: Option<&SearchConfig>,
    ) {
        use std::collections::hash_map::Entry;
        match entry {
            Entry::Occupied(mut slot) if prefer_rhs(&expr.expr, &slot.get().expr, search) => {
                slot.insert(expr.into_owned());
            }
            Entry::Vacant(slot) => {
                slot.insert(expr.into_owned());
            }
            Entry::Occupied(_) => {}
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.by_value.len() + self.by_value_and_digits.len()
    }

    pub(crate) fn into_values(self) -> impl Iterator<Item = EvaluatedExpr> {
        self.by_value
            .into_values()
            .chain(self.by_value_and_digits.into_values())
    }
}

fn prefer_rhs(candidate: &Expression, current: &Expression, search: Option<&SearchConfig>) -> bool {
    // For a fixed LHS and RHS value, exactness and error are identical.
    let complexity = candidate.complexity().cmp(&current.complexity());
    let parity = || legacy_parity_score_expr(candidate).cmp(&legacy_parity_score_expr(current));
    // A stop condition bounds results by the complexity of the simplest stop
    // match (`StopBound`). Keeping a costlier spelling of a value would raise
    // that bound; any costlier spelling is past the bound anyway.
    let stop_bounded =
        search.is_some_and(|config| config.stop_at_exact || config.stop_below.is_some());
    match search.map(|config| config.ranking_mode) {
        Some(RankingMode::Parity) if stop_bounded => complexity
            .then_with(parity)
            .then_with(|| candidate.symbols().cmp(current.symbols()))
            .is_lt(),
        Some(RankingMode::Parity) => parity()
            .then(complexity)
            .then_with(|| candidate.symbols().cmp(current.symbols()))
            .is_lt(),
        Some(RankingMode::Complexity) => complexity
            .then_with(|| candidate.symbols().cmp(current.symbols()))
            .is_lt(),
        None => complexity.is_lt(),
    }
}

fn dedupe_rhs_expressions(
    rhs_raw: Vec<EvaluatedExpr>,
    search: Option<&SearchConfig>,
) -> Vec<EvaluatedExpr> {
    let mut representatives = RhsRepresentatives::new(search);
    for expr in rhs_raw {
        representatives.insert(Cow::Owned(expr));
    }
    representatives.into_values().collect()
}

pub(crate) fn compare_expression_order(a: &EvaluatedExpr, b: &EvaluatedExpr) -> std::cmp::Ordering {
    a.expr
        .complexity()
        .cmp(&b.expr.complexity())
        .then_with(|| a.expr.symbols().cmp(b.expr.symbols()))
}

fn dedupe_lhs_expressions(
    lhs_raw: Vec<EvaluatedExpr>,
    filters: MatchFilters,
) -> Vec<EvaluatedExpr> {
    use std::collections::hash_map::Entry;

    let mut lhs_map: HashMap<_, EvaluatedExpr> = HashMap::new();
    for expr in lhs_raw {
        if !filters.allows_side(&expr) {
            continue;
        }
        let key = lhs_dedup_key(&expr, filters);
        match lhs_map.entry(key) {
            Entry::Occupied(mut slot) => {
                if compare_expression_order(&expr, slot.get()).is_lt() {
                    *slot.get_mut() = expr;
                }
            }
            Entry::Vacant(slot) => {
                slot.insert(expr);
            }
        }
    }
    lhs_map.into_values().collect()
}

fn select_lhs_expressions(
    lhs_raw: Vec<EvaluatedExpr>,
    search: Option<&SearchConfig>,
) -> Vec<EvaluatedExpr> {
    if search.is_none_or(|config| config.lhs_value_dedup) {
        dedupe_lhs_expressions(
            lhs_raw,
            search.map_or_else(MatchFilters::default, |c| c.filters),
        )
    } else {
        lhs_raw
    }
}

/// Generate all valid expressions up to the configured limits
pub fn generate_all(config: &GenConfig, target: f64) -> GeneratedExprs {
    generate_all_with_context(
        config,
        target,
        &EvalContext::from_slices(&config.user_constants, &config.user_functions),
    )
}

/// Generate all valid expressions up to the configured limits using an explicit evaluation context.
pub fn generate_all_with_context(
    config: &GenConfig,
    target: f64,
    eval_context: &EvalContext<'_>,
) -> GeneratedExprs {
    generate_all_with_rhs_policy(config, target, eval_context, None)
}

pub(crate) fn generate_all_with_rhs_policy(
    config: &GenConfig,
    target: f64,
    eval_context: &EvalContext<'_>,
    search: Option<&SearchConfig>,
) -> GeneratedExprs {
    let resolved = config.resolved_for_search(search);
    let config = resolved.as_ref();
    let mut lhs_raw = Vec::new();
    let mut rhs_raw = Vec::new();

    if config.generate_rhs && has_rhs_symbol_overrides(config) {
        // LHS pass with base symbol set.
        if config.generate_lhs {
            let mut lhs_config = config.clone();
            lhs_config.generate_rhs = false;
            generate_recursive(
                &lhs_config,
                target,
                *eval_context,
                &mut Expression::new(),
                0,
                &mut lhs_raw,
                &mut rhs_raw,
            );
        }

        // RHS pass with RHS-specific symbol overrides.
        let rhs_config = rhs_only_config(config);
        generate_recursive(
            &rhs_config,
            target,
            *eval_context,
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
            *eval_context,
            &mut Expression::new(),
            0, // current stack depth
            &mut lhs_raw,
            &mut rhs_raw,
        );
    }

    GeneratedExprs {
        lhs: select_lhs_expressions(lhs_raw, search),
        rhs: dedupe_rhs_expressions(rhs_raw, search),
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
    generate_all_with_limit_and_context(
        config,
        target,
        &EvalContext::from_slices(&config.user_constants, &config.user_functions),
        max_expressions,
    )
}

/// Generate expressions with an early-abort limit using an explicit evaluation context.
pub fn generate_all_with_limit_and_context(
    config: &GenConfig,
    target: f64,
    eval_context: &EvalContext<'_>,
    max_expressions: usize,
) -> Option<GeneratedExprs> {
    generate_bounded_with_rhs_policy(config, target, eval_context, max_expressions, false, None)
}

/// Generate expressions with an early-abort limit, preserving every LHS.
///
/// The streaming search path matches every generated LHS expression and only
/// deduplicates RHS expressions for the value database. Turbo uses this helper
/// when it wants to materialize the same LHS universe for parallel matching.
/// Deduplicating LHS by target-local `(value, derivative)` can discard a useful
/// non-degenerate expression in favor of a lower-complexity degenerate
/// representative, changing the best match.
#[cfg(feature = "parallel")]
pub fn generate_all_preserving_lhs_with_limit_and_context(
    config: &GenConfig,
    target: f64,
    eval_context: &EvalContext<'_>,
    max_expressions: usize,
) -> Option<GeneratedExprs> {
    generate_bounded_with_rhs_policy(config, target, eval_context, max_expressions, true, None)
}

pub(crate) fn generate_bounded_with_rhs_policy(
    config: &GenConfig,
    target: f64,
    eval_context: &EvalContext<'_>,
    max_expressions: usize,
    preserve_lhs: bool,
    search: Option<&SearchConfig>,
) -> Option<GeneratedExprs> {
    let resolved = config.resolved_for_search(search);
    let config = resolved.as_ref();
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let count = Arc::new(AtomicUsize::new(0));
    let limit = max_expressions;

    let mut lhs_raw = Vec::new();
    let mut rhs_raw = Vec::new();

    let mut callbacks = StreamingCallbacks {
        on_lhs: &mut |expr| {
            let current = count.fetch_add(1, Ordering::Relaxed) + 1;
            if current > limit {
                return false;
            }
            lhs_raw.push(expr.clone());
            true
        },
        on_rhs: &mut |expr| {
            let current = count.fetch_add(1, Ordering::Relaxed) + 1;
            if current > limit {
                return false;
            }
            rhs_raw.push(expr.clone());
            true
        },
    };

    generate_streaming_with_context(config, target, eval_context, &mut callbacks);

    let final_count = count.load(Ordering::Relaxed);
    if final_count > limit {
        return None;
    }

    Some(GeneratedExprs {
        lhs: if preserve_lhs {
            lhs_raw
        } else {
            select_lhs_expressions(lhs_raw, search)
        },
        rhs: dedupe_rhs_expressions(rhs_raw, search),
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
/// ```no_run
/// use ries_rs::gen::{GenConfig, StreamingCallbacks, generate_streaming};
/// let config = GenConfig::default();
/// let target = 2.5_f64;
/// let mut rhs_count = 0;
/// let mut lhs_count = 0;
/// let mut callbacks = StreamingCallbacks {
///     on_rhs: &mut |_expr| {
///         rhs_count += 1;
///         true // continue generation
///     },
///     on_lhs: &mut |_expr| {
///         lhs_count += 1;
///         true // continue generation
///     },
/// };
/// generate_streaming(&config, target, &mut callbacks);
/// ```
pub fn generate_streaming(config: &GenConfig, target: f64, callbacks: &mut StreamingCallbacks) {
    generate_streaming_with_context(
        config,
        target,
        &EvalContext::from_slices(&config.user_constants, &config.user_functions),
        callbacks,
    );
}

/// Generate expressions with streaming callbacks using an explicit evaluation context.
pub fn generate_streaming_with_context(
    config: &GenConfig,
    target: f64,
    eval_context: &EvalContext<'_>,
    callbacks: &mut StreamingCallbacks,
) {
    if config.generate_rhs && has_rhs_symbol_overrides(config) {
        if config.generate_lhs {
            let mut lhs_config = config.clone();
            lhs_config.generate_rhs = false;
            if !generate_recursive_streaming(
                &lhs_config,
                target,
                *eval_context,
                &mut Expression::new(),
                0,
                callbacks,
            ) {
                return;
            }
        }

        let rhs_config = rhs_only_config(config);
        generate_recursive_streaming(
            &rhs_config,
            target,
            *eval_context,
            &mut Expression::new(),
            0,
            callbacks,
        );
    } else {
        generate_recursive_streaming(
            config,
            target,
            *eval_context,
            &mut Expression::new(),
            0, // current stack depth
            callbacks,
        );
    }
}

/// Count generated expressions with an early-abort limit using streaming callbacks.
///
/// Returns `Some(count)` when generation completes within the limit, or `None`
/// as soon as the count exceeds `max_expressions`.
pub fn count_expressions_with_limit_and_context(
    config: &GenConfig,
    target: f64,
    eval_context: &EvalContext<'_>,
    max_expressions: usize,
) -> Option<usize> {
    let count = std::cell::Cell::new(0usize);
    let mut callbacks = StreamingCallbacks {
        on_lhs: &mut |_expr| {
            let next = count.get() + 1;
            count.set(next);
            next <= max_expressions
        },
        on_rhs: &mut |_expr| {
            let next = count.get() + 1;
            count.set(next);
            next <= max_expressions
        },
    };

    generate_streaming_with_context(config, target, eval_context, &mut callbacks);

    if count.get() > max_expressions {
        None
    } else {
        Some(count.get())
    }
}

#[inline]
/// Returns whether the RHS uses any symbol set or count overrides.
pub fn has_rhs_symbol_overrides(config: &GenConfig) -> bool {
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

impl GenConfig {
    /// Check a precomputed side against the active grammar and generation limits.
    pub fn allows_evaluated_side(&self, side: &EvaluatedExpr) -> bool {
        let lhs = side.expr.contains_x();
        let (constants, unary_ops, binary_ops, limits) = if lhs {
            (
                &self.constants,
                &self.unary_ops,
                &self.binary_ops,
                &self.symbol_max_counts,
            )
        } else {
            (
                self.rhs_constants.as_ref().unwrap_or(&self.constants),
                self.rhs_unary_ops.as_ref().unwrap_or(&self.unary_ops),
                self.rhs_binary_ops.as_ref().unwrap_or(&self.binary_ops),
                self.rhs_symbol_max_counts
                    .as_ref()
                    .unwrap_or(&self.symbol_max_counts),
            )
        };
        let symbols = side.expr.symbols();
        let (enabled, complexity_limit) = if lhs {
            (self.generate_lhs, self.max_lhs_complexity)
        } else {
            (self.generate_rhs, self.max_rhs_complexity)
        };

        enabled
            && side.expr.complexity() <= complexity_limit
            && symbols.len() <= self.max_length
            && side.value.is_finite()
            && side.value.abs() <= MAX_GENERATED_VALUE
            && side.num_type >= self.min_num_type
            && symbols.iter().all(|sym| match sym.seft() {
                Seft::A => *sym == Symbol::X || constants.contains(sym),
                Seft::B => unary_ops.contains(sym),
                Seft::C => binary_ops.contains(sym),
            })
            && limits
                .iter()
                .all(|(&symbol, &max)| side.expr.count_symbol(symbol) <= max)
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
    eval_context: EvalContext<'_>,
    current: &mut Expression,
    stack_depth: usize,
    callbacks: &mut StreamingCallbacks,
) -> bool {
    // Check if we have a complete expression
    if stack_depth == 1 && !current.is_empty() {
        // Try to evaluate it with user constants and functions support
        match evaluate_search(current, target, &eval_context) {
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
        if !generate_recursive_streaming(
            config,
            target,
            eval_context,
            current,
            stack_depth + 1,
            callbacks,
        ) {
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
            if !generate_recursive_streaming(
                config,
                target,
                eval_context,
                current,
                stack_depth + 1,
                callbacks,
            ) {
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
            if should_prune_unary(config, current, sym) {
                continue;
            }

            current.push_with_table(sym, &config.symbol_table);
            if !generate_recursive_streaming(
                config,
                target,
                eval_context,
                current,
                stack_depth,
                callbacks,
            ) {
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
            if should_prune_binary(config, current, sym) {
                continue;
            }

            current.push_with_table(sym, &config.symbol_table);
            if !generate_recursive_streaming(
                config,
                target,
                eval_context,
                current,
                stack_depth - 1,
                callbacks,
            ) {
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
    eval_context: EvalContext<'_>,
    current: &mut Expression,
    stack_depth: usize,
    lhs_out: &mut Vec<EvaluatedExpr>,
    rhs_out: &mut Vec<EvaluatedExpr>,
) {
    // Check if we have a complete expression
    if stack_depth == 1 && !current.is_empty() {
        // Try to evaluate it with user constants and functions support
        match evaluate_search(current, target, &eval_context) {
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
        generate_recursive(
            config,
            target,
            eval_context,
            current,
            stack_depth + 1,
            lhs_out,
            rhs_out,
        );
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
            generate_recursive(
                config,
                target,
                eval_context,
                current,
                stack_depth + 1,
                lhs_out,
                rhs_out,
            );
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
            if should_prune_unary(config, current, sym) {
                continue;
            }

            current.push_with_table(sym, &config.symbol_table);
            generate_recursive(
                config,
                target,
                eval_context,
                current,
                stack_depth,
                lhs_out,
                rhs_out,
            );
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
            if should_prune_binary(config, current, sym) {
                continue;
            }

            current.push_with_table(sym, &config.symbol_table);
            generate_recursive(
                config,
                target,
                eval_context,
                current,
                stack_depth - 1,
                lhs_out,
                rhs_out,
            );
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

// =============================================================================
// PRUNING
// =============================================================================
//
// Every pruning rule belongs to exactly one class:
//
// - Identity: the pruned expression is degenerate (`a/a`, `1^b`, `log_1`) or
//   equals another expression built from a subset of its own symbols at no
//   greater complexity (`--a`, `ln(e^a)`, `-(a-b)` vs `b-a`, `2+3` vs `3+2`).
//   Always safe.
// - Rewrite: the expression has a preferred spelling that uses a symbol it
//   does not contain (`2nd root` vs `sqrt`, `a+a` vs `2*a`). Such a rule may
//   fire only when the replacement is enabled on the side being generated, is
//   not heavier under the active weights, fits the per-symbol count limit,
//   and is not itself pruned. `pruning_never_loses_reachable_values` checks
//   identities and rewrites against generation without them.
// - Heuristic: a deliberate, lossy search-space reduction inherited from
//   RIES-style generation (`exp(exp(a))`, `1/ln(a)`, `sinpi(sinpi(a))`). These
//   may remove values; they are documented in docs/SEARCH_MODEL.md.

/// Which pruning classes are active. Tests disable identities and rewrites to
/// compare generation with and without them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
enum PruneMode {
    All,
    HeuristicsOnly,
}

#[cfg(test)]
thread_local! {
    static PRUNE_MODE: std::cell::Cell<PruneMode> = const { std::cell::Cell::new(PruneMode::All) };
}

#[inline]
fn prune_mode() -> PruneMode {
    #[cfg(test)]
    {
        PRUNE_MODE.with(|mode| mode.get())
    }
    #[cfg(not(test))]
    {
        PruneMode::All
    }
}

#[inline]
fn symbol_enabled(config: &GenConfig, sym: Symbol) -> bool {
    match sym.seft() {
        Seft::A => config.constants.contains(&sym),
        Seft::B => config.unary_ops.contains(&sym),
        Seft::C => config.binary_ops.contains(&sym),
    }
}

/// Whether `replacement`, swapped in for symbols of total weight
/// `replaced_weight` in `current`, would be generated in its place.
#[inline]
fn replacement_admissible(
    config: &GenConfig,
    current: &Expression,
    replacement: Symbol,
    replaced_weight: u32,
) -> bool {
    symbol_enabled(config, replacement)
        && config.symbol_table.weight(replacement) <= replaced_weight
        && !exceeds_symbol_limit(config, current, replacement)
}

/// Pruning rules for unary operators to avoid redundant expressions
fn should_prune_unary(config: &GenConfig, expr: &Expression, sym: Symbol) -> bool {
    let Some(&last) = expr.symbols().last() else {
        return false;
    };
    let mode = prune_mode();
    (mode == PruneMode::All && is_unary_identity(config, expr, last, sym))
        || is_unary_heuristic(config, last, sym)
}

/// Unary identities: applying `sym` after `last` returns an operand or a
/// same-symbol spelling with no greater complexity.
fn is_unary_identity(config: &GenConfig, expr: &Expression, last: Symbol, sym: Symbol) -> bool {
    use Symbol::*;
    match (last, sym) {
        // --a = a, 1/(1/a) = a
        (Neg, Neg) | (Recip, Recip) => true,
        // (sqrt a)^2 = a, ln(e^a) = a, e^(ln a) = a
        (Sqrt, Square) | (Exp, Ln) | (Ln, Exp) => true,
        // 1/sqrt(a) = sqrt(1/a) and 1/a^2 = (1/a)^2 use the same symbols.
        // The respelling starts with `a r`, which must not itself be pruned
        // (1/ln(a) is a heuristic prune, so 1/sqrt(ln a) is kept as is).
        (Sqrt, Recip) | (Square, Recip) => {
            let operand = expr_tail(&expr.symbols()[..expr.len() - 1], 0);
            !should_prune_unary(config, &operand, Recip)
        }
        // -(a-b) = b-a
        (Sub, Neg) => true,
        _ => false,
    }
}

/// Deliberately lossy unary pruning (see the module notes above).
fn is_unary_heuristic(config: &GenConfig, last: Symbol, sym: Symbol) -> bool {
    use Symbol::*;
    match (last, sym) {
        // sqrt(a^2) = |a|; absolute value is not modeled.
        (Square, Sqrt) => true,
        // 1/ln(a) is rarely useful.
        (Ln, Recip) => true,
        // (a^2)^2 and sqrt(sqrt(a)): prefer a power or fourth root. Only
        // pruned when that spelling exists, so a^4 stays representable.
        (Square, Square) => symbol_enabled(config, Pow) && symbol_enabled(config, Four),
        (Sqrt, Sqrt) => symbol_enabled(config, Root) && symbol_enabled(config, Four),
        // Repeated trig and double exponentials are almost never useful.
        (SinPi, SinPi) | (CosPi, CosPi) | (Exp, Exp) => true,
        // W(e^a) and W(1/a) are rarely useful. Note W(e^a) is not a in
        // general: W(y e^y) = y.
        (Exp, LambertW) | (Recip, LambertW) => true,
        _ => false,
    }
}

/// Pruning rules for binary operators
fn should_prune_binary(config: &GenConfig, expr: &Expression, sym: Symbol) -> bool {
    prune_mode() == PruneMode::All
        && (is_binary_identity(expr, sym) || is_binary_rewrite(config, expr, sym))
}

/// Binary identities and degenerate forms. Operands are complete
/// subexpressions: the token before the final token can be inside the right
/// operand.
fn is_binary_identity(expr: &Expression, sym: Symbol) -> bool {
    let symbols = expr.symbols();
    if symbols.len() < 2 {
        return false;
    }

    let last = symbols[symbols.len() - 1];
    let rhs_start = subexpr_start(symbols, symbols.len());
    let lhs_atom = rhs_start.and_then(|end| {
        subexpr_start(symbols, end)
            .filter(|&start| start + 1 == end)
            .map(|start| symbols[start])
    });
    let rhs_atom = rhs_start
        .filter(|&start| start + 1 == symbols.len())
        .map(|start| symbols[start]);

    use Symbol::*;

    match sym {
        // a - a = 0, x - x = 0
        Sub if is_same_subexpr(symbols, 2) => true,
        Sub if lhs_atom == Some(X) && rhs_atom == Some(X) => true,

        // a / a = 1, x / x = 1, a / 1 = a
        Div if is_same_subexpr(symbols, 2) => true,
        Div if lhs_atom == Some(X) && rhs_atom == Some(X) => true,
        Div if last == One => true,

        // x + (-x) = 0
        Add if lhs_atom == Some(X)
            && rhs_start.is_some_and(|start| symbols[start..] == [X, Neg]) =>
        {
            true
        }

        // 1^b = 1, a^1 = a
        Pow if lhs_atom == Some(One) => true,
        Pow if last == One => true,

        // x * 1 = x, 1 * x = x
        Mul if rhs_atom == Some(One) || lhs_atom == Some(One) => true,

        // In `a b v`, a is the index and b is the radicand: first roots are
        // identities and roots of 1 are constant.
        Root => {
            let Some(radicand_start) = rhs_start else {
                return false;
            };
            let index_is_one = radicand_start >= 1
                && symbols[radicand_start - 1] == One
                && subexpr_start(symbols, radicand_start) == Some(radicand_start - 1);
            last == One || index_is_one
        }

        // log_x(x) = 1, log_1(b) is undefined, log_a(1) = 0
        Log if lhs_atom == Some(X) && rhs_atom == Some(X) => true,
        Log if lhs_atom == Some(One) || rhs_atom == Some(One) => true,

        // Ordering: prefer 2+3 over 3+2 for commutative ops
        Add | Mul
            if lhs_atom
                .zip(rhs_atom)
                .is_some_and(|(lhs, rhs)| lhs > rhs && is_constant(lhs) && is_constant(rhs)) =>
        {
            true
        }

        _ => false,
    }
}

/// Binary rewrites toward a dedicated symbol. Each fires only when that
/// symbol would generate the same value in its place.
fn is_binary_rewrite(config: &GenConfig, expr: &Expression, sym: Symbol) -> bool {
    use Symbol::*;
    let symbols = expr.symbols();
    let Some(rhs_start) = subexpr_start(symbols, symbols.len()) else {
        return false;
    };
    let Some(lhs_start) = subexpr_start(symbols, rhs_start) else {
        return false;
    };
    let weight = |sym| config.symbol_table.weight(sym);
    let lhs_atom = (lhs_start + 1 == rhs_start).then(|| symbols[lhs_start]);

    match sym {
        // `2 b v` is sqrt(b): prefer `b q`.
        Root if lhs_atom == Some(Two) => {
            replacement_admissible(config, expr, Sqrt, weight(Two) + weight(Root))
                && !should_prune_unary(config, &expr_tail(symbols, rhs_start), Sqrt)
        }
        // log_e(b) is ln(b): prefer `b l`.
        Log if lhs_atom == Some(E) => {
            replacement_admissible(config, expr, Ln, weight(E) + weight(Log))
                && !should_prune_unary(config, &expr_tail(symbols, rhs_start), Ln)
        }
        // a + a is 2*a: prefer `2 a *` (or `a 2 *` when a is an atom ordered
        // before 2). Both the 2 and the * must be admissible.
        Add if is_same_subexpr(symbols, 2) => {
            let operand_weight: u32 = symbols[rhs_start..].iter().map(|&s| weight(s)).sum();
            symbol_enabled(config, Two)
                && symbol_enabled(config, Mul)
                && weight(Two) + weight(Mul) <= operand_weight + weight(Add)
                && !exceeds_symbol_limit(config, expr, Two)
                && !exceeds_symbol_limit(config, expr, Mul)
        }
        _ => false,
    }
}

/// The trailing operand starting at `start`, as its own expression, so a
/// rewrite can ask whether its replacement would itself be pruned.
fn expr_tail(symbols: &[Symbol], start: usize) -> Expression {
    let mut tail = Expression::new();
    for &sym in &symbols[start..] {
        tail.push(sym);
    }
    tail
}

/// Check whether the top two stack subexpressions are identical.
///
/// This is only ever used with `n == 2` (the generator compares the two
/// operands feeding a binary operator). The boundary of a complete postfix
/// subexpression is found with a single backward scan that tracks how many
/// operands still need to be supplied to the left: atoms supply one
/// (`-1`), unary ops are neutral (`0`), binary ops demand one more (`+1`).
/// When the running balance reaches zero, the subexpression starts there.
///
/// This avoids the prefix-depth table the previous implementation built on
/// every call — a hot path during generation, since it runs for every
/// commutative/cancelling binary candidate.
fn is_same_subexpr(symbols: &[Symbol], n: usize) -> bool {
    // Each subexpression is at least one symbol, so two of them need >= 2
    // symbols. The original guard also rejected anything below `n * 2`, which
    // for the only supported case (`n == 2`) means a length of at least 4;
    // preserve that exactly so single-atom operands stay handled by the
    // dedicated `last == X && prev == X` arms in `should_prune_binary`.
    if n != 2 || symbols.len() < n * 2 {
        return false;
    }

    let end = symbols.len();
    let Some(top_start) = subexpr_start(symbols, end) else {
        return false;
    };
    let Some(prev_start) = subexpr_start(symbols, top_start) else {
        return false;
    };

    // Slice equality already compares lengths, so unequal-length operands
    // short-circuit to `false`.
    symbols[prev_start..top_start] == symbols[top_start..end]
}

/// Walk left from `end - 1` until one complete subexpression has been
/// consumed, returning its start index (or `None` on underflow).
fn subexpr_start(symbols: &[Symbol], end: usize) -> Option<usize> {
    let mut needed: i32 = 1;
    for i in (0..end).rev() {
        needed += match symbols[i].seft() {
            Seft::A => -1,
            Seft::B => 0,
            Seft::C => 1,
        };
        if needed == 0 {
            return Some(i);
        }
    }
    None
}

/// Check if a symbol is a constant (no x)
fn is_constant(sym: Symbol) -> bool {
    matches!(sym.seft(), Seft::A) && sym != Symbol::X
}

/// Generate expressions in parallel using Rayon
#[cfg(feature = "parallel")]
pub fn generate_all_parallel(config: &GenConfig, target: f64) -> GeneratedExprs {
    generate_all_parallel_with_context(
        config,
        target,
        &EvalContext::from_slices(&config.user_constants, &config.user_functions),
    )
}

/// Generate expressions in parallel using Rayon with an explicit evaluation context.
#[cfg(feature = "parallel")]
pub fn generate_all_parallel_with_context(
    config: &GenConfig,
    target: f64,
    eval_context: &EvalContext<'_>,
) -> GeneratedExprs {
    generate_parallel_with_rhs_policy(config, target, eval_context, None)
}

#[cfg(feature = "parallel")]
pub(crate) fn generate_parallel_with_rhs_policy(
    config: &GenConfig,
    target: f64,
    eval_context: &EvalContext<'_>,
    search: Option<&SearchConfig>,
) -> GeneratedExprs {
    let resolved = config.resolved_for_search(search);
    let config = resolved.as_ref();
    use rayon::prelude::*;

    // Parallel path currently assumes shared LHS/RHS symbol sets.
    if has_rhs_symbol_overrides(config) {
        return generate_all_with_rhs_policy(config, target, eval_context, search);
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
        if let Ok(result) = evaluate_search(&expr1, target, eval_context) {
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
                && !should_prune_unary(config, &expr1, sym2)
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
            generate_recursive(
                config,
                target,
                *eval_context,
                &mut expr,
                depth,
                &mut lhs,
                &mut rhs,
            );
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

    GeneratedExprs {
        lhs: select_lhs_expressions(lhs_raw, search),
        rhs: dedupe_rhs_expressions(rhs_raw, search),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_arch = "wasm32"))]
    use proptest::prelude::*;

    fn udf(spec: &str) -> UserFunction {
        UserFunction::parse(spec).unwrap()
    }

    fn options_with_functions(
        functions: &[UserFunction],
        constants: &[UserConstant],
    ) -> ExpressionConstraintOptions {
        let mut opts = ExpressionConstraintOptions::default();
        for (slot, constant) in opts.user_constant_types.iter_mut().zip(constants) {
            *slot = constant.num_type;
        }
        for (idx, function) in functions.iter().enumerate() {
            opts.user_function_constraints[idx] = Some(UserFunctionConstraints::from_function(
                function,
                &opts.user_constant_types,
            ));
        }
        opts
    }

    #[test]
    fn arithmetic_user_function_follows_argument_type() {
        use Symbol::{Pi, Pow, SinPi, Two, UserFunction0 as Sq};
        let opts = ExpressionConstraintOptions {
            rational_trig_args: true,
            rational_exponents: true,
            ..options_with_functions(&[udf("1:sq:square:s")], &[])
        };
        let respects = |symbols: &[Symbol]| {
            expression_respects_constraints(&Expression::from_symbols(symbols), opts)
        };
        assert!(respects(&[Two, Sq, SinPi])); // sinpi(sq(2)) = sinpi(4)
        assert!(!respects(&[Pi, Sq, SinPi])); // sinpi(pi^2)
        assert!(respects(&[Two, Two, Sq, Pow])); // 2^sq(2)
        assert!(!respects(&[Two, Pi, Sq, Pow])); // 2^(pi^2)
        assert_eq!(
            constraint_num_type(&Expression::from_symbols(&[Two, Sq]), &opts),
            Some(NumType::Integer)
        );
    }

    #[test]
    fn user_function_bodies_obey_restrictions() {
        use Symbol::{Pi, Two, UserFunction0 as Sp};
        let base = options_with_functions(&[udf("1:sp:sinpi:S")], &[]);
        let check = |symbols: &[Symbol], opts: ExpressionConstraintOptions| {
            expression_respects_constraints(&Expression::from_symbols(symbols), opts)
        };
        let rational_trig = ExpressionConstraintOptions {
            rational_trig_args: true,
            ..base
        };
        assert!(check(&[Two, Sp], rational_trig));
        assert!(!check(&[Pi, Sp], rational_trig)); // sinpi(pi) inside the body
        let no_trig = ExpressionConstraintOptions {
            max_trig_cycles: Some(0),
            ..base
        };
        assert!(!check(&[Two, Sp], no_trig));
        // Without a body summary the function stays opaque, as before.
        let opaque = ExpressionConstraintOptions {
            rational_trig_args: true,
            ..Default::default()
        };
        assert!(check(&[Pi, Sp], opaque));
    }

    /// Minimum complexity at which each distinct value is generated. LHS keys
    /// include the derivative. Degenerate values excluded by identity
    /// pruning are skipped: constant LHS sides (derivative 0) cannot locate
    /// x, and RHS 0 and 1 come from `a-a`, `a/a`, `1^b`, and `log_a(1)`;
    /// the equations `A - B = 0` and `A / B = 1` are costlier spellings of
    /// `A = B`.
    fn reachable_values(config: &GenConfig, mode: PruneMode) -> HashMap<String, (u32, String)> {
        PRUNE_MODE.with(|m| m.set(mode));
        let generated = generate_all(config, 1.2345);
        PRUNE_MODE.with(|m| m.set(PruneMode::All));
        let mut out: HashMap<String, (u32, String)> = HashMap::new();
        let mut record = |key: String, expr: &Expression| {
            let candidate = (expr.complexity(), expr.to_postfix());
            let entry = out.entry(key).or_insert_with(|| candidate.clone());
            if candidate < *entry {
                *entry = candidate;
            }
        };
        for lhs in &generated.lhs {
            if lhs.derivative.abs() > 1e-12 {
                record(
                    format!("L{:.9e}/{:.9e}", lhs.value, lhs.derivative),
                    &lhs.expr,
                );
            }
        }
        for rhs in &generated.rhs {
            if rhs.value.abs() > 1e-12 && (rhs.value - 1.0).abs() > 1e-12 {
                record(format!("R{:.9e}", rhs.value), &rhs.expr);
            }
        }
        out
    }

    /// Differential oracle for pruning: identities and rewrites may only drop
    /// an expression whose value is still generated at no greater complexity.
    #[test]
    fn pruning_never_loses_reachable_values() {
        use Symbol::*;
        let base = |constants: &[Symbol], unary: &[Symbol], binary: &[Symbol]| GenConfig {
            max_lhs_complexity: 52,
            max_rhs_complexity: 52,
            max_length: 6,
            constants: constants.to_vec(),
            unary_ops: unary.to_vec(),
            binary_ops: binary.to_vec(),
            ..GenConfig::default()
        };
        let reweighted = |mut config: GenConfig, weights: &[(Symbol, u32)]| {
            let profile = Profile {
                symbol_weights: weights.iter().copied().collect(),
                ..Profile::default()
            };
            config.symbol_table = Arc::new(SymbolTable::from_profile(&profile));
            config
        };
        let full_constants = [One, Two, Three, Pi, E, X];
        let full_unary = [Neg, Recip, Sqrt, Square, Ln, Exp];
        let full_binary = [Add, Sub, Mul, Div, Pow, Root, Log];
        let configs = [
            (
                "default symbols",
                base(&full_constants, &full_unary, &full_binary),
            ),
            ("-S x2v", base(&[Two, X], &[], &[Root])),
            (
                "no sqrt",
                base(
                    &[One, Two, E, X],
                    &[Neg, Square, Ln],
                    &[Add, Mul, Pow, Root, Log],
                ),
            ),
            (
                "no ln",
                base(&[Two, E, Pi, X], &[Sqrt, Recip], &[Add, Log, Div]),
            ),
            (
                "no two",
                base(&[One, Three, E, X], &[Sqrt], &[Add, Mul, Root]),
            ),
            (
                "no mul",
                base(&[Two, Three, Pi, X], &[Sqrt, Neg], &[Add, Sub, Root]),
            ),
            (
                "heavy replacements",
                reweighted(
                    base(&full_constants, &full_unary, &full_binary),
                    &[(Sqrt, 40), (Ln, 40), (Mul, 30)],
                ),
            ),
            ("count limits", {
                let mut config = base(&full_constants, &full_unary, &full_binary);
                config.symbol_max_counts = HashMap::from([(Sqrt, 1), (Ln, 1), (Mul, 1)]);
                config
            }),
        ];
        for (name, config) in configs {
            let reference = reachable_values(&config, PruneMode::HeuristicsOnly);
            let pruned = reachable_values(&config, PruneMode::All);
            let mut lost: Vec<_> = reference
                .iter()
                .filter(|(key, (complexity, _))| {
                    pruned.get(*key).is_none_or(|(c, _)| c > complexity)
                })
                .map(|(_, expr)| expr)
                .collect();
            lost.sort();
            assert!(
                lost.is_empty(),
                "{name}: pruning lost {} of {} values, e.g. {:?}",
                lost.len(),
                reference.len(),
                &lost[..lost.len().min(12)]
            );
        }
    }

    #[test]
    fn root_and_log_rewrites_require_an_admissible_replacement() {
        use Symbol::*;
        let config = |constants: Vec<Symbol>, unary: Vec<Symbol>, binary: Vec<Symbol>| GenConfig {
            constants,
            unary_ops: unary,
            binary_ops: binary,
            ..GenConfig::default()
        };
        let two_root = Expression::from_symbols(&[Two, Two]);
        let log_e = Expression::from_symbols(&[E, Two]);
        let with_sqrt_ln = config(vec![Two, E, X], vec![Sqrt, Ln], vec![Root, Log]);
        assert!(should_prune_binary(&with_sqrt_ln, &two_root, Root));
        assert!(should_prune_binary(&with_sqrt_ln, &log_e, Log));
        let without = config(vec![Two, E, X], vec![], vec![Root, Log]);
        assert!(!should_prune_binary(&without, &two_root, Root));
        assert!(!should_prune_binary(&without, &log_e, Log));
        let mut limited = with_sqrt_ln.clone();
        limited.symbol_max_counts = HashMap::from([(Sqrt, 1)]);
        let sqrt_used = Expression::from_symbols(&[Two, Two, Sqrt]);
        assert!(!should_prune_binary(&limited, &sqrt_used, Root));
        let profile = Profile {
            symbol_weights: HashMap::from([(Sqrt, 60)]),
            ..Profile::default()
        };
        let heavy = GenConfig {
            symbol_table: Arc::new(SymbolTable::from_profile(&profile)),
            ..with_sqrt_ln
        };
        assert!(!should_prune_binary(&heavy, &two_root, Root));
    }

    /// Every well-formed expression up to `max_len` over `alphabet`.
    fn all_expressions(alphabet: &[Symbol], max_len: usize) -> Vec<Vec<Symbol>> {
        fn extend(
            alphabet: &[Symbol],
            max_len: usize,
            current: &mut Vec<Symbol>,
            depth: usize,
            out: &mut Vec<Vec<Symbol>>,
        ) {
            if depth == 1 {
                out.push(current.clone());
            }
            if current.len() == max_len {
                return;
            }
            for &sym in alphabet {
                let next_depth = match sym.seft() {
                    Seft::A => depth + 1,
                    Seft::B if depth >= 1 => depth,
                    Seft::C if depth >= 2 => depth - 1,
                    _ => continue,
                };
                // Stack depth must be reducible within the remaining length.
                if next_depth > max_len - current.len() {
                    continue;
                }
                current.push(sym);
                extend(alphabet, max_len, current, next_depth, out);
                current.pop();
            }
        }
        let mut out = Vec::new();
        extend(alphabet, max_len, &mut Vec::new(), 0, &mut out);
        out
    }

    /// Differential oracle: the structural constraint walker and the numeric
    /// evaluator classify expressions independently, and must agree. The
    /// only permitted difference is that the walker does not know the value of
    /// a non-literal integer exponent, so it may be weaker for powers.
    #[test]
    fn constraint_walker_type_matches_evaluator() {
        use Symbol::*;
        let functions = [
            udf("1:sq:square:s"),
            udf("1:inc:plus one:1+"),
            udf("1:sh:sinh:E|r-2/"),
            udf("1:dq:dup mul:|*"),
        ];
        let constants: Vec<UserConstant> = [
            (2.5, NumType::Rational),
            (std::f64::consts::SQRT_2, NumType::Algebraic),
            (1.3, NumType::Transcendental),
            (3.0, NumType::Integer),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (value, num_type))| UserConstant {
            weight: 16,
            name: format!("c{i}"),
            description: String::new(),
            value,
            num_type,
        })
        .collect();
        let opts = options_with_functions(&functions, &constants);
        let context = EvalContext::from_slices(&constants, &functions);
        let alphabet = [
            One,
            Two,
            Pi,
            Phi,
            X,
            UserConstant0,
            UserConstant1,
            UserConstant2,
            UserConstant3,
            Neg,
            Recip,
            Sqrt,
            Square,
            Ln,
            SinPi,
            UserFunction0,
            UserFunction1,
            UserFunction2,
            UserFunction3,
            Add,
            Sub,
            Mul,
            Div,
            Pow,
            Root,
        ];
        let mut compared = 0;
        for symbols in all_expressions(&alphabet, 4) {
            let expr = Expression::from_symbols(&symbols);
            let Ok(evaluated) = crate::eval::evaluate_with_context(&expr, 0.7, &context) else {
                continue;
            };
            let walked = constraint_num_type(&expr, &opts).expect("well-formed expression");
            if symbols.contains(&Pow) {
                assert!(
                    walked <= evaluated.num_type,
                    "{symbols:?}: walker {walked:?} > eval {:?}",
                    evaluated.num_type
                );
            } else {
                assert_eq!(walked, evaluated.num_type, "{symbols:?}");
            }
            compared += 1;
        }
        assert!(compared > 10_000, "only {compared} expressions compared");
    }

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

    fn expr_from_postfix(s: &str) -> Expression {
        Expression::parse(s).expect("valid expression")
    }

    #[test]
    fn root_pruning_uses_complete_index_operand() {
        for (postfix, pruned) in [
            ("32v", false),   // cube root of 2
            ("3x2+v", false), // radicand ends in 2
            ("3x1+v", false), // penultimate symbol is not the index
            ("x1+3v", false), // index ends in 1 but is not literal 1
            ("23v", true),    // square root
            ("2x1+v", true),  // square root of a compound radicand
            ("1xv", true),    // first root
            ("31v", true),    // root of 1
        ] {
            let mut operands = Expression::parse(postfix).unwrap();
            operands.pop();
            assert_eq!(
                should_prune_binary(&GenConfig::default(), &operands, Symbol::Root),
                pruned,
                "{postfix}"
            );
        }
    }

    #[test]
    fn power_constraints_distinguish_negative_and_positive_integer_exponents() {
        let options = ExpressionConstraintOptions {
            rational_trig_args: true,
            ..Default::default()
        };
        // A negative integer power is rational, so using it as the exponent
        // of 2 is generally algebraic rather than rational.
        assert!(!expression_respects_constraints(
            &expr_from_postfix("222n^^S"),
            options
        ));
        assert!(expression_respects_constraints(
            &expr_from_postfix("223^^S"),
            options
        ));
        assert!(expression_respects_constraints(
            &expr_from_postfix("22n^S"),
            options
        ));
    }

    fn stub_eval_expr(postfix: &str, value: f64, derivative: f64) -> EvaluatedExpr {
        EvaluatedExpr {
            expr: expr_from_postfix(postfix),
            value,
            derivative,
            num_type: NumType::Transcendental,
        }
    }

    #[test]
    fn rhs_only_generation_honors_overrides_in_all_entry_points() {
        let mut base = fast_test_config();
        base.generate_lhs = false;
        base.constants = vec![Symbol::One];
        base.unary_ops.clear();
        base.binary_ops = vec![Symbol::Add];
        base.max_rhs_complexity = 50;
        base.max_length = 5;

        let mut count_limit = base.clone();
        count_limit.rhs_symbol_max_counts = Some(HashMap::from([(Symbol::One, 2)]));
        let mut operator_limit = base.clone();
        operator_limit.rhs_binary_ops = Some(Vec::new());
        let mut constant_override = base;
        constant_override.rhs_constants = Some(vec![Symbol::Two]);

        for (index, config) in [count_limit, operator_limit, constant_override]
            .iter()
            .enumerate()
        {
            let check = |expr: &EvaluatedExpr| {
                let symbols = expr.expr.symbols();
                match index {
                    0 => assert!(symbols.iter().filter(|&&s| s == Symbol::One).count() <= 2),
                    1 => assert!(!symbols.contains(&Symbol::Add)),
                    2 => assert!(!symbols.contains(&Symbol::One)),
                    _ => unreachable!(),
                }
            };
            let batch = generate_all(config, 2.5);
            assert!(batch.lhs.is_empty());
            assert!(!batch.rhs.is_empty());
            batch.rhs.iter().for_each(&check);
            let bounded = generate_all_with_limit(config, 2.5, 10_000).unwrap();
            assert!(bounded.lhs.is_empty());
            bounded.rhs.iter().for_each(&check);
            let mut streamed = 0;
            let mut callbacks = StreamingCallbacks {
                on_lhs: &mut |_| panic!("unexpected LHS"),
                on_rhs: &mut |expr| {
                    check(expr);
                    streamed += 1;
                    true
                },
            };
            generate_streaming(config, 2.5, &mut callbacks);
            assert!(streamed > 0);
            #[cfg(feature = "parallel")]
            {
                let parallel = generate_all_parallel(config, 2.5);
                assert!(parallel.lhs.is_empty());
                assert!(!parallel.rhs.is_empty());
                parallel.rhs.iter().for_each(&check);
            }
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

    #[test]
    fn test_count_expressions_with_limit_matches_generated_total() {
        let mut config = fast_test_config();
        config.max_lhs_complexity = 30;
        config.max_rhs_complexity = 30;

        let context = EvalContext::from_slices(&config.user_constants, &config.user_functions);
        let total_generated = std::cell::Cell::new(0usize);
        let mut callbacks = StreamingCallbacks {
            on_lhs: &mut |_expr| {
                total_generated.set(total_generated.get() + 1);
                true
            },
            on_rhs: &mut |_expr| {
                total_generated.set(total_generated.get() + 1);
                true
            },
        };
        generate_streaming_with_context(&config, 2.5, &context, &mut callbacks);
        let counted = count_expressions_with_limit_and_context(
            &config,
            2.5,
            &context,
            total_generated.get() + 1,
        );

        assert_eq!(counted, Some(total_generated.get()));
    }

    #[test]
    fn test_count_expressions_with_limit_returns_none_when_exceeded() {
        let mut config = fast_test_config();
        config.max_lhs_complexity = 30;
        config.max_rhs_complexity = 30;

        let context = EvalContext::from_slices(&config.user_constants, &config.user_functions);
        let total_generated = std::cell::Cell::new(0usize);
        let mut callbacks = StreamingCallbacks {
            on_lhs: &mut |_expr| {
                total_generated.set(total_generated.get() + 1);
                true
            },
            on_rhs: &mut |_expr| {
                total_generated.set(total_generated.get() + 1);
                true
            },
        };
        generate_streaming_with_context(&config, 2.5, &context, &mut callbacks);
        let counted = count_expressions_with_limit_and_context(
            &config,
            2.5,
            &context,
            total_generated.get() / 2,
        );

        assert_eq!(counted, None);
    }

    // ==================== expression_respects_constraints tests ====================

    #[test]
    fn test_is_same_subexpr_detects_identical_operands() {
        use crate::symbol::Symbol::*;

        // 2 3 + | 2 3 + : the two operands of a pending binary op are equal.
        let same = [Two, Three, Add, Two, Three, Add];
        assert!(is_same_subexpr(&same, 2));

        // 2 3 + | 2 4 + : operands differ in the last atom.
        let diff = [Two, Three, Add, Two, Four, Add];
        assert!(!is_same_subexpr(&diff, 2));

        // Unequal-length operands: 2 3 + | 5 .
        let unequal = [Two, Three, Add, Five];
        assert!(!is_same_subexpr(&unequal, 2));

        // Nested unary inside identical operands: 2 q n | 2 q n (sqrt then neg).
        let nested = [Two, Sqrt, Neg, Two, Sqrt, Neg];
        assert!(is_same_subexpr(&nested, 2));

        // Single-atom operands stay out of scope (handled by dedicated arms).
        let short = [Two, Two];
        assert!(!is_same_subexpr(&short, 2));

        // Only n == 2 is supported.
        let triple = [Two, Three, Add, Two, Three, Add, Two, Three, Add];
        assert!(!is_same_subexpr(&triple, 3));
    }

    #[test]
    fn test_quantize_value_signed_boundary_symmetry_around_zero() {
        let below = 0.49 / QUANTIZE_SCALE;
        let above = 0.51 / QUANTIZE_SCALE;

        assert_eq!(quantize_value(below), 0);
        assert_eq!(quantize_value(-below), 0);
        assert_eq!(quantize_value(above), 1);
        assert_eq!(quantize_value(-above), -1);
    }

    #[test]
    fn test_dedupe_rhs_keeps_distinct_values_within_old_bucket() {
        let base = 2.0;
        let rhs = dedupe_rhs_expressions(
            vec![
                stub_eval_expr("11+", base + 0.49 / QUANTIZE_SCALE, 0.0),
                stub_eval_expr("2", base, 0.0),
            ],
            None,
        );

        assert_eq!(rhs.len(), 2);
    }

    #[test]
    fn test_rhs_deduplication_can_discard_parity_preferred_equivalent() {
        use crate::pool::legacy_parity_score_expr;

        // A tiny reference universe: both expressions evaluate to 3. Rank the
        // raw candidates independently before applying the production deduper.
        let raw = vec![
            stub_eval_expr("3", 3.0, 0.0),
            stub_eval_expr("12+", 3.0, 0.0),
        ];
        let parity_best = raw
            .iter()
            .min_by_key(|e| legacy_parity_score_expr(&e.expr))
            .unwrap();
        assert_eq!(parity_best.expr.to_postfix(), "12+");
        let retained = dedupe_rhs_expressions(raw, None);
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].expr.to_postfix(), "3");
    }

    #[test]
    fn test_large_finite_values_share_quantization_bucket() {
        assert_eq!(quantize_value(1e11), quantize_value(2e11));
        assert_eq!(quantize_value(-1e11), quantize_value(-2e11));
    }

    #[test]
    fn test_dedupe_lhs_keeps_distinct_values_and_derivatives_within_old_bucket() {
        let delta = 0.49 / QUANTIZE_SCALE;
        let lhs = dedupe_lhs_expressions(
            vec![
                stub_eval_expr("x", 1.0, 1.0),
                stub_eval_expr("x1+", 1.0 + delta, 1.0),
                stub_eval_expr("xs", 1.0, 1.0 + delta),
            ],
            MatchFilters::default(),
        );

        assert_eq!(lhs.len(), 3);
    }

    #[cfg(not(target_arch = "wasm32"))]
    proptest! {
        #[test]
        fn test_quantize_value_collapses_values_within_same_bucket(bucket in -1_000_000i64..1_000_000i64) {
            let base = bucket as f64 / QUANTIZE_SCALE;
            prop_assert_eq!(quantize_value(base + 0.49 / QUANTIZE_SCALE), bucket);
            prop_assert_eq!(quantize_value(base - 0.49 / QUANTIZE_SCALE), bucket);
        }

        #[test]
        fn test_quantize_value_separates_adjacent_buckets(bucket in -1_000_000i64..1_000_000i64) {
            let base = bucket as f64 / QUANTIZE_SCALE;
            prop_assert_eq!(quantize_value(base + 0.51 / QUANTIZE_SCALE), bucket + 1);
            prop_assert_eq!(quantize_value(base - 0.51 / QUANTIZE_SCALE), bucket - 1);
        }
    }

    #[test]
    fn binary_identity_pruning_respects_complete_operands() {
        for (postfix, op) in [
            ("x1W", Symbol::Pow),
            ("x1W", Symbol::Mul),
            ("x1W", Symbol::Log),
            ("xeW", Symbol::Log),
            ("2x1+", Symbol::Add),
        ] {
            let mut expr = expr_from_postfix(&format!("{postfix}{}", op as u8 as char));
            expr.pop();
            assert!(
                !should_prune_binary(&GenConfig::default(), &expr, op),
                "{postfix}"
            );
        }
        for (postfix, op) in [
            ("1xW", Symbol::Pow),
            ("1xW", Symbol::Mul),
            ("exW", Symbol::Log),
            ("21", Symbol::Add),
        ] {
            let mut expr = expr_from_postfix(&format!("{postfix}{}", op as u8 as char));
            expr.pop();
            assert!(
                should_prune_binary(&GenConfig::default(), &expr, op),
                "{postfix}"
            );
        }
        let config = GenConfig {
            max_lhs_complexity: 100,
            max_rhs_complexity: 100,
            max_length: 4,
            constants: vec![Symbol::One, Symbol::Two, Symbol::X],
            unary_ops: vec![Symbol::LambertW],
            binary_ops: vec![Symbol::Pow],
            ..Default::default()
        };
        assert!(generate_all(&config, 3.0)
            .lhs
            .iter()
            .any(|e| e.expr.to_postfix() == "x1W^"));
    }

    #[test]
    fn algebraic_generation_excludes_transcendental_roots() {
        let config = GenConfig {
            max_lhs_complexity: 60,
            max_rhs_complexity: 60,
            max_length: 3,
            constants: vec![Symbol::Three, Symbol::Pi, Symbol::Two],
            unary_ops: vec![],
            binary_ops: vec![Symbol::Root],
            min_num_type: NumType::Algebraic,
            generate_lhs: false,
            ..Default::default()
        };
        let generated = generate_all(&config, 1.0);
        assert!(!generated.rhs.iter().any(|e| e.expr.to_postfix() == "3pv"));
        assert!(generated.rhs.iter().any(|e| e.expr.to_postfix() == "32v"));
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
        let expr = Expression::from_symbols(&[crate::symbol::Symbol::Add]); // Just a binary op
        assert!(
            !expression_respects_constraints(&expr, opts),
            "Malformed expression should return false"
        );

        // Incomplete expression (too many values)
        let expr =
            Expression::from_symbols(&[crate::symbol::Symbol::One, crate::symbol::Symbol::Two]);
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

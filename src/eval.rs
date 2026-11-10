//! Expression evaluation with automatic differentiation
//!
//! Evaluates postfix expressions and computes derivatives using forward-mode AD.
//!
//! # Performance
//!
//! For hot loops (generation, Newton-Raphson), use `evaluate_with_workspace()` with
//! a reusable `EvalWorkspace` to avoid heap allocations on every call.

use crate::expr::Expression;
use crate::profile::UserConstant;
use crate::symbol::{NumType, Seft, Symbol};
use crate::udf::{UdfOp, UserFunction};

/// Result of evaluating an expression
#[derive(Debug, Clone, Copy)]
pub struct EvalResult {
    /// The computed value
    pub value: f64,
    /// Derivative with respect to x
    pub derivative: f64,
    /// Numeric type of the result
    pub num_type: NumType,
}

/// Evaluation error types
///
/// These errors indicate what went wrong during expression evaluation.
/// For more detailed context, use the error message methods.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    /// Stack underflow during evaluation
    #[error("Stack underflow: not enough operands on stack")]
    StackUnderflow,
    /// User constant slot referenced by the expression is not configured
    #[error("Missing user constant: slot u{0} is not configured")]
    MissingUserConstant(usize),
    /// Division by zero
    #[error("Division by zero: divisor was zero or near-zero")]
    DivisionByZero,
    /// Logarithm of non-positive number
    #[error("Logarithm domain error: argument was non-positive")]
    LogDomain,
    /// Square root of negative number
    #[error("Square root domain error: argument was negative")]
    SqrtDomain,
    /// Overflow or NaN result
    #[error("Overflow: result is infinite or NaN")]
    Overflow,
    /// Invalid expression
    #[error("Invalid expression: malformed or incomplete")]
    Invalid,
    /// Error with position context
    #[error("{err} at position {pos}")]
    WithPosition {
        #[source]
        err: Box<EvalError>,
        pos: usize,
    },
    /// Error with value context
    #[error("{err} (value: {val})")]
    WithValue {
        #[source]
        err: Box<EvalError>,
        val: ordered_float::OrderedFloat<f64>,
    },
    /// Error with expression context
    #[error("{err} in expression '{expr}'")]
    WithExpression {
        #[source]
        err: Box<EvalError>,
        expr: String,
    },
}

impl EvalError {
    /// Create a detailed error message with context (backward compatibility)
    pub fn with_context(self, position: Option<usize>, value: Option<f64>) -> Self {
        let mut err = self;
        if let Some(pos) = position {
            err = EvalError::WithPosition {
                err: Box::new(err),
                pos,
            };
        }
        if let Some(val) = value {
            err = EvalError::WithValue {
                err: Box::new(err),
                val: ordered_float::OrderedFloat(val),
            };
        }
        err
    }

    /// Add expression context
    pub fn with_expression(self, expr: String) -> Self {
        EvalError::WithExpression {
            err: Box::new(self),
            expr,
        }
    }
}

/// Mathematical constants
pub mod constants {
    pub const PI: f64 = std::f64::consts::PI;
    pub const E: f64 = std::f64::consts::E;
    pub const PHI: f64 = std::f64::consts::GOLDEN_RATIO; // Golden ratio
    /// Euler-Mascheroni constant γ
    pub const GAMMA: f64 = std::f64::consts::EULER_GAMMA;
    /// Plastic constant ρ (root of x³ = x + 1)
    pub const PLASTIC: f64 = 1.324_717_957_244_746;
    /// Apéry's constant ζ(3)
    pub const APERY: f64 = 1.202_056_903_159_594_2;
    /// Catalan's constant G
    pub const CATALAN: f64 = 0.915_965_594_177_219;
}

/// Default trig argument scale used by `sinpi/cospi/tanpi`.
///
/// This matches original `sinpi(x) = sin(πx)` semantics.
pub const DEFAULT_TRIG_ARGUMENT_SCALE: f64 = std::f64::consts::PI;

/// Explicit evaluation context for a single run.
///
/// This keeps trig scaling and user-defined symbols inside the function
/// signature instead of relying on process-global evaluator state.
#[derive(Clone, Copy, Debug)]
pub struct EvalContext<'a> {
    /// Argument scale for `sinpi/cospi/tanpi`.
    pub trig_argument_scale: f64,
    /// User-defined constants available during evaluation.
    pub user_constants: &'a [UserConstant],
    /// User-defined functions available during evaluation.
    pub user_functions: &'a [UserFunction],
}

impl Default for EvalContext<'static> {
    fn default() -> Self {
        Self {
            trig_argument_scale: DEFAULT_TRIG_ARGUMENT_SCALE,
            user_constants: &[],
            user_functions: &[],
        }
    }
}

impl EvalContext<'static> {
    /// Create a default context with built-in trig semantics and no user symbols.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<'a> EvalContext<'a> {
    /// Create a context from user-defined constants and functions.
    pub fn from_slices(
        user_constants: &'a [UserConstant],
        user_functions: &'a [UserFunction],
    ) -> Self {
        Self {
            trig_argument_scale: DEFAULT_TRIG_ARGUMENT_SCALE,
            user_constants,
            user_functions,
        }
    }

    /// Override the trig argument scale for this evaluation context.
    pub fn with_trig_argument_scale(mut self, scale: f64) -> Self {
        if scale.is_finite() && scale != 0.0 {
            self.trig_argument_scale = scale;
        }
        self
    }
}

/// Stack entry for evaluation with derivative tracking
#[derive(Debug, Clone, Copy)]
struct StackEntry {
    val: f64,
    deriv: f64,
    num_type: NumType,
}

impl StackEntry {
    fn new(val: f64, deriv: f64, num_type: NumType) -> Self {
        Self {
            val,
            deriv,
            num_type,
        }
    }

    fn constant(val: f64, num_type: NumType) -> Self {
        Self {
            val,
            deriv: 0.0,
            num_type,
        }
    }
}

/// Reusable workspace for expression evaluation.
///
/// Using a workspace avoids heap allocations on every `evaluate()` call,
/// which is critical for performance in hot loops (generation, Newton-Raphson).
///
/// # Example
///
/// ```no_run
/// use ries_rs::eval::{EvalWorkspace, evaluate_with_workspace};
/// use ries_rs::expr::Expression;
/// let mut workspace = EvalWorkspace::new();
/// let expressions: Vec<Expression> = vec![];
/// let x = 1.0_f64;
/// for expr in &expressions {
///     let result = evaluate_with_workspace(expr, x, &mut workspace)?;
///     // workspace is reused, no new allocations
/// }
/// # Ok::<(), ries_rs::eval::EvalError>(())
/// ```
pub struct EvalWorkspace {
    stack: Vec<StackEntry>,
}

impl EvalWorkspace {
    /// Create a new workspace with pre-allocated capacity.
    ///
    /// Capacity of 32 handles most expressions; grows automatically if needed.
    pub fn new() -> Self {
        Self {
            stack: Vec::with_capacity(32),
        }
    }

    /// Clear the workspace for reuse (keeps allocated capacity).
    #[inline]
    fn clear(&mut self) {
        self.stack.clear();
    }
}

impl Default for EvalWorkspace {
    fn default() -> Self {
        Self::new()
    }
}

/// Evaluate an expression at a given value of x, using a reusable workspace.
///
/// This is the hot-path version that avoids heap allocations.
/// Use this in loops where `evaluate()` is called many times.
///
/// Note: This is a convenience wrapper for the full `evaluate_with_workspace_and_constants_and_functions`
/// when you don't need user constants or functions. It's provided as a simpler API for common cases.
#[inline]
pub fn evaluate_with_workspace(
    expr: &Expression,
    x: f64,
    workspace: &mut EvalWorkspace,
) -> Result<EvalResult, EvalError> {
    let context = EvalContext::new();
    if expr.has_decimal_literals() {
        return evaluate_decimal_literal_expression(expr, x, workspace, &context);
    }
    evaluate_with_workspace_and_context(expr, x, workspace, &context)
}

/// Evaluate an expression with user constants, using a reusable workspace.
///
/// This is the hot-path version that avoids heap allocations.
/// The `user_constants` slice provides values for `UserConstant0..15` symbols.
///
/// Note: This is a convenience wrapper for the full `evaluate_with_workspace_and_constants_and_functions`
/// when you don't need user functions. It's provided as a simpler API for common cases.
#[inline]
pub fn evaluate_with_workspace_and_constants(
    expr: &Expression,
    x: f64,
    workspace: &mut EvalWorkspace,
    user_constants: &[UserConstant],
) -> Result<EvalResult, EvalError> {
    let context = EvalContext::from_slices(user_constants, &[]);
    if expr.has_decimal_literals() {
        return evaluate_decimal_literal_expression(expr, x, workspace, &context);
    }
    evaluate_with_workspace_and_context(expr, x, workspace, &context)
}

/// Evaluate an expression with user constants and user functions, using a reusable workspace.
///
/// This is the full hot-path version that avoids heap allocations.
/// The `user_constants` slice provides values for `UserConstant0..15` symbols.
/// The `user_functions` slice provides bodies for `UserFunction0..15` symbols.
#[inline]
pub fn evaluate_with_workspace_and_constants_and_functions(
    expr: &Expression,
    x: f64,
    workspace: &mut EvalWorkspace,
    user_constants: &[UserConstant],
    user_functions: &[UserFunction],
) -> Result<EvalResult, EvalError> {
    let context = EvalContext::from_slices(user_constants, user_functions);
    if expr.has_decimal_literals() {
        return evaluate_decimal_literal_expression(expr, x, workspace, &context);
    }
    evaluate_with_workspace_and_context(expr, x, workspace, &context)
}

/// Evaluate an expression using an explicit evaluation context and reusable workspace.
///
/// This is the preferred hot-path API for library consumers that need explicit
/// control over trig semantics or user-defined symbols.
#[inline]
pub fn evaluate_with_workspace_and_context(
    expr: &Expression,
    x: f64,
    workspace: &mut EvalWorkspace,
    context: &EvalContext<'_>,
) -> Result<EvalResult, EvalError> {
    workspace.clear();
    let stack = &mut workspace.stack;

    for &sym in expr.symbols() {
        match sym.seft() {
            Seft::A => {
                let entry = eval_constant_with_user(sym, x, context.user_constants)?;
                stack.push(entry);
            }
            Seft::B => {
                // Check if this is a user function
                if matches!(
                    sym,
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
                        | Symbol::UserFunction15
                ) {
                    // Evaluate user function
                    let a = stack.pop().ok_or(EvalError::StackUnderflow)?;
                    let result = eval_user_function(sym, a, context)?;
                    stack.push(result);
                } else {
                    let a = stack.pop().ok_or(EvalError::StackUnderflow)?;
                    let result = eval_unary(sym, a, context.trig_argument_scale)?;
                    stack.push(result);
                }
            }
            Seft::C => {
                let b = stack.pop().ok_or(EvalError::StackUnderflow)?;
                let a = stack.pop().ok_or(EvalError::StackUnderflow)?;
                let result = eval_binary(sym, a, b)?;
                stack.push(result);
            }
        }
    }

    if stack.len() != 1 {
        return Err(EvalError::Invalid);
    }

    // SAFETY: len == 1 check above guarantees pop succeeds
    let result = stack.pop().unwrap();

    // Check for invalid results
    if result.val.is_nan() || result.val.is_infinite() {
        return Err(EvalError::Overflow);
    }

    Ok(EvalResult {
        value: result.val,
        derivative: result.deriv,
        num_type: result.num_type,
    })
}

/// Evaluate a solver expression whose postfix contains an exact integer span.
///
/// The postfix under the span is only a scaffold. Decimal Horner products are
/// not exact for every integer (2^58 through 2^61 among them), and magnitudes
/// past 2^63 do not fit in i64. Push the recorded value and skip the span.
#[cold]
#[inline(never)]
fn evaluate_decimal_literal_expression(
    expr: &Expression,
    x: f64,
    workspace: &mut EvalWorkspace,
    context: &EvalContext<'_>,
) -> Result<EvalResult, EvalError> {
    workspace.clear();
    let stack = &mut workspace.stack;
    let mut index = 0;
    while index < expr.len() {
        if let Some((len, value)) = expr.decimal_literal_at(index) {
            stack.push(StackEntry::constant(value, NumType::Integer));
            index += len;
            continue;
        }
        let sym = expr.symbols()[index];
        index += 1;
        match sym.seft() {
            Seft::A => {
                let entry = eval_constant_with_user(sym, x, context.user_constants)?;
                stack.push(entry);
            }
            Seft::B => {
                if matches!(
                    sym,
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
                        | Symbol::UserFunction15
                ) {
                    let a = stack.pop().ok_or(EvalError::StackUnderflow)?;
                    let result = eval_user_function(sym, a, context)?;
                    stack.push(result);
                } else {
                    let a = stack.pop().ok_or(EvalError::StackUnderflow)?;
                    let result = eval_unary(sym, a, context.trig_argument_scale)?;
                    stack.push(result);
                }
            }
            Seft::C => {
                let b = stack.pop().ok_or(EvalError::StackUnderflow)?;
                let a = stack.pop().ok_or(EvalError::StackUnderflow)?;
                let result = eval_binary(sym, a, b)?;
                stack.push(result);
            }
        }
    }

    if stack.len() != 1 {
        return Err(EvalError::Invalid);
    }
    let result = stack.pop().unwrap();
    if result.val.is_nan() || result.val.is_infinite() {
        return Err(EvalError::Overflow);
    }
    Ok(EvalResult {
        value: result.val,
        derivative: result.deriv,
        num_type: result.num_type,
    })
}

/// Evaluate an expression at a given value of x.
///
/// Convenience wrapper that allocates a new workspace. For hot loops,
/// prefer `evaluate_with_workspace()` with a reusable `EvalWorkspace`.
///
/// Note: This is a convenience API for library users. Internal code uses
/// `evaluate_fast_with_constants_and_functions` for performance.
pub fn evaluate(expr: &Expression, x: f64) -> Result<EvalResult, EvalError> {
    evaluate_with_context(expr, x, &EvalContext::new())
}

/// Evaluate an expression at a given value of x with user constants.
///
/// Convenience wrapper that allocates a new workspace.
pub fn evaluate_with_constants(
    expr: &Expression,
    x: f64,
    user_constants: &[UserConstant],
) -> Result<EvalResult, EvalError> {
    let context = EvalContext::from_slices(user_constants, &[]);
    evaluate_with_context(expr, x, &context)
}

/// Evaluate an expression at a given value of x with user constants and user functions.
///
/// Convenience wrapper that allocates a new workspace.
pub fn evaluate_with_constants_and_functions(
    expr: &Expression,
    x: f64,
    user_constants: &[UserConstant],
    user_functions: &[UserFunction],
) -> Result<EvalResult, EvalError> {
    let context = EvalContext::from_slices(user_constants, user_functions);
    evaluate_with_context(expr, x, &context)
}

/// Evaluate an expression at a given value of x with an explicit evaluation context.
///
/// Convenience wrapper that allocates a new workspace.
pub fn evaluate_with_context(
    expr: &Expression,
    x: f64,
    context: &EvalContext<'_>,
) -> Result<EvalResult, EvalError> {
    let mut workspace = EvalWorkspace::new();
    if expr.has_decimal_literals() {
        return evaluate_decimal_literal_expression(expr, x, &mut workspace, context);
    }
    evaluate_with_workspace_and_context(expr, x, &mut workspace, context)
}

/// Evaluate an expression using a thread-local workspace (zero allocations after warmup).
///
/// This is ideal for parallel code where each thread needs its own workspace.
/// Note: This version does NOT support user constants. For user constants,
/// use `evaluate_with_constants()` or `evaluate_with_workspace_and_constants()`.
///
/// Note: This is a convenience wrapper for the full `evaluate_fast_with_constants_and_functions`
/// when you don't need user constants or functions. It's provided as a simpler API for common cases.
#[inline]
pub fn evaluate_fast(expr: &Expression, x: f64) -> Result<EvalResult, EvalError> {
    evaluate_fast_with_context(expr, x, &EvalContext::new())
}

/// Evaluate an expression using a thread-local workspace with user constants.
///
/// Note: This uses a global thread-local storage, so it's not safe to call recursively
/// with different user_constants. For recursive calls, use `evaluate_with_workspace_and_constants`.
///
/// Note: This is a convenience wrapper for the full `evaluate_fast_with_constants_and_functions`
/// when you don't need user functions. It's provided as a simpler API for common cases.
#[inline]
pub fn evaluate_fast_with_constants(
    expr: &Expression,
    x: f64,
    user_constants: &[UserConstant],
) -> Result<EvalResult, EvalError> {
    let context = EvalContext::from_slices(user_constants, &[]);
    evaluate_fast_with_context(expr, x, &context)
}

/// Evaluate an expression using a thread-local workspace with user constants and user functions.
///
/// # Thread-Local Workspace
///
/// This function uses a `thread_local!` static to cache an `EvalWorkspace` for each thread.
/// The workspace is created on first use and reused for all subsequent calls from the same thread.
/// This provides zero-allocation evaluation after the initial warmup, making it ideal for:
///
/// - Parallel code where each thread needs its own workspace
/// - Hot loops where allocation overhead matters
/// - High-throughput evaluation scenarios
///
/// # Limitations
///
/// - This uses a global thread-local storage, so it's not safe to call recursively
///   with different `user_constants` or `user_functions`. The same workspace is shared.
/// - For recursive calls or when user constants/functions vary per-call,
///   use [`evaluate_with_workspace_and_constants_and_functions`] instead.
///
/// # Example
///
/// ```no_run
/// use ries_rs::eval::evaluate_fast_with_constants_and_functions;
/// use ries_rs::expr::Expression;
/// let expr = Expression::new();
/// let x = 1.0_f64;
/// // First call allocates workspace (warmup)
/// let result = evaluate_fast_with_constants_and_functions(&expr, x, &[], &[]);
///
/// // Subsequent calls reuse the same workspace (no allocations)
/// for _ in 0..1000 {
///     let _ = evaluate_fast_with_constants_and_functions(&expr, x, &[], &[]);
/// }
/// ```
#[inline]
pub fn evaluate_fast_with_constants_and_functions(
    expr: &Expression,
    x: f64,
    user_constants: &[UserConstant],
    user_functions: &[UserFunction],
) -> Result<EvalResult, EvalError> {
    let context = EvalContext::from_slices(user_constants, user_functions);
    evaluate_fast_with_context(expr, x, &context)
}

/// Evaluate an expression using a thread-local workspace and explicit context.
#[inline]
pub fn evaluate_fast_with_context(
    expr: &Expression,
    x: f64,
    context: &EvalContext<'_>,
) -> Result<EvalResult, EvalError> {
    if expr.has_decimal_literals() {
        let mut workspace = EvalWorkspace::new();
        return evaluate_decimal_literal_expression(expr, x, &mut workspace, context);
    }
    evaluate_search(expr, x, context)
}

/// Search evaluation. Generated expressions have no exact-integer annotation,
/// so this skips that test. The check lives on the public entry points; leaving
/// it here kept release search a few percent slower.
#[inline]
pub(crate) fn evaluate_search(
    expr: &Expression,
    x: f64,
    context: &EvalContext<'_>,
) -> Result<EvalResult, EvalError> {
    thread_local! {
        /// Thread-local evaluation workspace.
        ///
        /// Each thread gets its own workspace instance that's lazily allocated
        /// on first use. The workspace maintains internal Vec storage that grows
        /// as needed but is never deallocated, providing zero-allocation hot paths.
        static WORKSPACE: std::cell::RefCell<EvalWorkspace> = std::cell::RefCell::new(EvalWorkspace::new());
    }

    WORKSPACE.with(|ws| {
        let mut workspace = ws.borrow_mut();
        evaluate_with_workspace_and_context(expr, x, &mut workspace, context)
    })
}

/// Evaluate a constant or variable symbol with user constant lookup.
fn eval_constant_with_user(
    sym: Symbol,
    x: f64,
    user_constants: &[UserConstant],
) -> Result<StackEntry, EvalError> {
    use Symbol::*;
    match sym {
        One => Ok(StackEntry::constant(1.0, NumType::Integer)),
        Two => Ok(StackEntry::constant(2.0, NumType::Integer)),
        Three => Ok(StackEntry::constant(3.0, NumType::Integer)),
        Four => Ok(StackEntry::constant(4.0, NumType::Integer)),
        Five => Ok(StackEntry::constant(5.0, NumType::Integer)),
        Six => Ok(StackEntry::constant(6.0, NumType::Integer)),
        Seven => Ok(StackEntry::constant(7.0, NumType::Integer)),
        Eight => Ok(StackEntry::constant(8.0, NumType::Integer)),
        Nine => Ok(StackEntry::constant(9.0, NumType::Integer)),
        Pi => Ok(StackEntry::constant(constants::PI, NumType::Transcendental)),
        E => Ok(StackEntry::constant(constants::E, NumType::Transcendental)),
        Phi => Ok(StackEntry::constant(constants::PHI, NumType::Algebraic)),
        // New constants
        Gamma => Ok(StackEntry::constant(
            constants::GAMMA,
            NumType::Transcendental,
        )),
        Plastic => Ok(StackEntry::constant(constants::PLASTIC, NumType::Algebraic)),
        Apery => Ok(StackEntry::constant(
            constants::APERY,
            NumType::Transcendental,
        )),
        Catalan => Ok(StackEntry::constant(
            constants::CATALAN,
            NumType::Transcendental,
        )),
        X => Ok(StackEntry::new(x, 1.0, NumType::Integer)), // x can be any value, including integer
        // User constants - look up value from the user_constants slice
        UserConstant0 | UserConstant1 | UserConstant2 | UserConstant3 | UserConstant4
        | UserConstant5 | UserConstant6 | UserConstant7 | UserConstant8 | UserConstant9
        | UserConstant10 | UserConstant11 | UserConstant12 | UserConstant13 | UserConstant14
        | UserConstant15 => {
            // Get the index from the symbol
            let idx = sym.user_constant_index().unwrap() as usize;
            user_constants
                .get(idx)
                .map(|uc| StackEntry::constant(uc.value, uc.num_type))
                .ok_or(EvalError::MissingUserConstant(idx))
        }
        _ => Err(EvalError::Invalid),
    }
}

/// Evaluate a user-defined function
///
/// Takes the input argument and the user_functions slice, looks up the function
/// definition, executes the body, and returns the result.
fn eval_user_function(
    sym: Symbol,
    input: StackEntry,
    context: &EvalContext<'_>,
) -> Result<StackEntry, EvalError> {
    // Get the function index
    let idx = sym.user_function_index().ok_or(EvalError::Invalid)? as usize;

    // Look up the function definition
    let udf = context.user_functions.get(idx).ok_or(EvalError::Invalid)?;

    // Reuse a thread-local scratch buffer rather than allocating a fresh Vec on every
    // call. eval_user_function is invoked in the inner generation loop (potentially
    // millions of times at high complexity), so avoiding the heap allocation matters.
    // UDFs do not call other UDFs, so the borrow is never re-entered.
    thread_local! {
        static UDF_STACK: std::cell::RefCell<Vec<StackEntry>> =
            std::cell::RefCell::new(Vec::with_capacity(16));
    }

    UDF_STACK.with(|cell| -> Result<StackEntry, EvalError> {
        let mut stack = cell.borrow_mut();
        stack.clear();
        stack.push(input);

        // Execute each operation in the function body
        for op in &udf.body {
            match op {
                // Rust callers can construct a body without the parser. A
                // function may depend on x only through its stack argument.
                UdfOp::Symbol(Symbol::X) => return Err(EvalError::Invalid),
                UdfOp::Symbol(sym) => {
                    match sym.seft() {
                        Seft::A => {
                            // Constant - push onto stack
                            let entry = eval_constant_with_user(*sym, 0.0, context.user_constants)?;
                            stack.push(entry);
                        }
                        Seft::B => {
                            // Unary operator - pop one, push result
                            let a = stack.pop().ok_or(EvalError::StackUnderflow)?;
                            let result = eval_unary(*sym, a, context.trig_argument_scale)?;
                            stack.push(result);
                        }
                        Seft::C => {
                            // Binary operator - pop two, push result
                            let b = stack.pop().ok_or(EvalError::StackUnderflow)?;
                            let a = stack.pop().ok_or(EvalError::StackUnderflow)?;
                            let result = eval_binary(*sym, a, b)?;
                            stack.push(result);
                        }
                    }
                }
                UdfOp::Dup => {
                    // Duplicate top of stack. Dereference immediately so the
                    // immutable borrow ends before the mutable push.
                    let top = *stack.last().ok_or(EvalError::StackUnderflow)?;
                    stack.push(top);
                }
                UdfOp::Swap => {
                    // Swap top two elements
                    let len = stack.len();
                    if len < 2 {
                        return Err(EvalError::StackUnderflow);
                    }
                    stack.swap(len - 1, len - 2);
                }
            }
        }

        // Function should leave exactly one value on the stack
        if stack.len() != 1 {
            return Err(EvalError::Invalid);
        }

        // SAFETY: len == 1 check above guarantees pop succeeds
        let result = stack.pop().unwrap();

        // Check for invalid results
        if result.val.is_nan() || result.val.is_infinite() {
            return Err(EvalError::Overflow);
        }

        Ok(result)
    })
}

/// `Some(true)` when `value` is an even integer, `Some(false)` when it is odd,
/// `None` when it is not a finite integer.
///
/// Magnitudes at or above 2^53 are multiples of at least 2, so every finite
/// value there is even. The low bit is taken from the significand. This never
/// casts to `i64`: that cast saturates, and `i64::MAX` is odd, so 2^63 and
/// every larger finite value would be reported as odd.
pub(crate) fn f64_exact_integer_is_even(value: f64) -> Option<bool> {
    if !value.is_finite() {
        return None;
    }
    let bits = value.abs().to_bits();
    let biased = ((bits >> 52) & 0x7ff) as i32;
    if biased == 0 {
        return (value == 0.0).then_some(true);
    }
    let unbiased = biased - 1023;
    if unbiased >= 53 {
        return Some(true);
    }
    if unbiased < 0 {
        return None;
    }
    let mantissa = bits & ((1u64 << 52) - 1);
    if unbiased < 52 {
        let fraction_bits = (52 - unbiased) as u32;
        if mantissa & ((1u64 << fraction_bits) - 1) != 0 {
            return None;
        }
    }
    if unbiased == 0 {
        return Some(false);
    }
    let parity_shift = (52 - unbiased) as u32;
    let odd = ((mantissa >> parity_shift) & 1) == 1;
    Some(!odd)
}

/// Evaluate a unary operator with derivative
fn eval_unary(
    sym: Symbol,
    a: StackEntry,
    trig_argument_scale: f64,
) -> Result<StackEntry, EvalError> {
    use Symbol::*;

    #[inline]
    fn is_default_trig_scale(scale: f64) -> bool {
        scale == DEFAULT_TRIG_ARGUMENT_SCALE
    }

    /// `i64::MAX as f64` is 2^63, which is not an i64. `as i64` saturates it
    /// to `i64::MAX` (odd) and made `tanpi(2^62)` look like a pole.
    #[inline]
    fn rounded_f64_to_i64(rounded: f64) -> Option<i64> {
        if !rounded.is_finite() {
            return None;
        }
        if rounded < i64::MIN as f64 || rounded >= i64::MAX as f64 {
            return None;
        }
        Some(rounded as i64)
    }

    /// Integer classification that keeps the small-argument tests in the same
    /// order as a plain `Option` snap. Non-integers return [`IntegerSnap::NotInteger`]
    /// at the tolerance check and do not inspect magnitude.
    enum IntegerSnap {
        Small(i64),
        /// Finite integer with magnitude >= 2^63. The ulp is at least 2048, so
        /// the value is even and not a half-integer.
        LargeEven,
        NotInteger,
    }

    #[inline]
    fn snap_integer(value: f64) -> IntegerSnap {
        let rounded = value.round();
        if value != rounded {
            return IntegerSnap::NotInteger;
        }
        if !rounded.is_finite() {
            return IntegerSnap::NotInteger;
        }
        if rounded < i64::MIN as f64 || rounded >= i64::MAX as f64 {
            return IntegerSnap::LargeEven;
        }
        IntegerSnap::Small(rounded as i64)
    }

    #[inline]
    fn snap_half_integer_index(value: f64) -> Option<i64> {
        let doubled = value * 2.0;
        let rounded = doubled.round();
        if doubled != rounded {
            return None;
        }
        let rounded_i = rounded_f64_to_i64(rounded)?;
        if rounded_i.rem_euclid(2) == 0 {
            return None;
        }
        Some((rounded_i - 1) / 2)
    }

    #[inline]
    fn alternating_sign(n: i64) -> f64 {
        if n.rem_euclid(2) == 0 {
            1.0
        } else {
            -1.0
        }
    }

    // Reduce before multiplying by π so large noninteger inputs retain their
    // fractional part. Noninteger f64 values fit exactly within i64.
    #[inline]
    fn reduced_sin_cos(value: f64) -> (f64, f64) {
        let nearest = value.round();
        let sign = alternating_sign(nearest as i64);
        let fraction = value - nearest;
        let (sin, cos) = if fraction.abs() > 0.25 {
            // Measure from the half-integer to retain accuracy near a pole.
            let (sin_delta, cos_delta) = ((0.5 - fraction.abs()) * constants::PI).sin_cos();
            (cos_delta.copysign(fraction), sin_delta)
        } else {
            (fraction * constants::PI).sin_cos()
        };
        (sign * sin, sign * cos)
    }

    let (val, deriv, num_type) = match sym {
        // Negation: -a, d(-a)/dx = -da/dx
        Neg => (-a.val, -a.deriv, a.num_type),

        // Reciprocal: 1/a, d(1/a)/dx = -da/dx / a²
        Recip => {
            if a.val.abs() < f64::MIN_POSITIVE {
                return Err(EvalError::DivisionByZero);
            }
            let val = 1.0 / a.val;
            let deriv = -a.deriv / (a.val * a.val);
            let num_type = if a.num_type == NumType::Integer {
                NumType::Rational
            } else {
                a.num_type
            };
            (val, deriv, num_type)
        }

        // Square root: sqrt(a), d(sqrt(a))/dx = da/dx / (2*sqrt(a))
        Sqrt => {
            if a.val < 0.0 {
                return Err(EvalError::SqrtDomain);
            }
            let val = a.val.sqrt();
            let deriv = if val.abs() > f64::MIN_POSITIVE {
                a.deriv / (2.0 * val)
            } else {
                0.0
            };
            let num_type = if a.num_type >= NumType::Constructible {
                NumType::Constructible
            } else {
                a.num_type
            };
            (val, deriv, num_type)
        }

        // Square: a², d(a²)/dx = 2*a*da/dx
        Square => {
            let val = a.val * a.val;
            let deriv = 2.0 * a.val * a.deriv;
            (val, deriv, a.num_type)
        }

        // Natural log: ln(a), d(ln(a))/dx = da/dx / a
        Ln => {
            if a.val <= 0.0 {
                return Err(EvalError::LogDomain);
            }
            let val = a.val.ln();
            let deriv = a.deriv / a.val;
            (val, deriv, NumType::Transcendental)
        }

        // Exponential: e^a, d(e^a)/dx = e^a * da/dx
        Exp => {
            let val = a.val.exp();
            if val.is_infinite() {
                return Err(EvalError::Overflow);
            }
            let deriv = val * a.deriv;
            (val, deriv, NumType::Transcendental)
        }

        // sin(π*a), d(sin(πa))/dx = π*cos(πa)*da/dx
        SinPi => {
            if is_default_trig_scale(trig_argument_scale) {
                match snap_integer(a.val) {
                    IntegerSnap::Small(n) => (
                        0.0,
                        trig_argument_scale * alternating_sign(n) * a.deriv,
                        NumType::Transcendental,
                    ),
                    IntegerSnap::LargeEven => {
                        (0.0, trig_argument_scale * a.deriv, NumType::Transcendental)
                    }
                    IntegerSnap::NotInteger => {
                        if let Some(n) = snap_half_integer_index(a.val) {
                            (alternating_sign(n), 0.0, NumType::Transcendental)
                        } else {
                            let (val, cos) = reduced_sin_cos(a.val);
                            let deriv = trig_argument_scale * cos * a.deriv;
                            (val, deriv, NumType::Transcendental)
                        }
                    }
                }
            } else {
                let val = (trig_argument_scale * a.val).sin();
                let deriv = trig_argument_scale * (trig_argument_scale * a.val).cos() * a.deriv;
                (val, deriv, NumType::Transcendental)
            }
        }

        // cos(π*a), d(cos(πa))/dx = -π*sin(πa)*da/dx
        CosPi => {
            if is_default_trig_scale(trig_argument_scale) {
                match snap_integer(a.val) {
                    IntegerSnap::Small(n) => (alternating_sign(n), 0.0, NumType::Transcendental),
                    IntegerSnap::LargeEven => (1.0, 0.0, NumType::Transcendental),
                    IntegerSnap::NotInteger => {
                        if let Some(n) = snap_half_integer_index(a.val) {
                            (
                                0.0,
                                -trig_argument_scale * alternating_sign(n) * a.deriv,
                                NumType::Transcendental,
                            )
                        } else {
                            let (sin, val) = reduced_sin_cos(a.val);
                            let deriv = -trig_argument_scale * sin * a.deriv;
                            (val, deriv, NumType::Transcendental)
                        }
                    }
                }
            } else {
                let val = (trig_argument_scale * a.val).cos();
                let deriv = -trig_argument_scale * (trig_argument_scale * a.val).sin() * a.deriv;
                (val, deriv, NumType::Transcendental)
            }
        }

        // tan(π*a), d(tan(πa))/dx = π*sec²(πa)*da/dx
        TanPi => {
            if is_default_trig_scale(trig_argument_scale) {
                if snap_half_integer_index(a.val).is_some() {
                    return Err(EvalError::Overflow);
                }
                match snap_integer(a.val) {
                    IntegerSnap::Small(_) | IntegerSnap::LargeEven => {
                        (0.0, trig_argument_scale * a.deriv, NumType::Transcendental)
                    }
                    IntegerSnap::NotInteger => {
                        let (sin_val, cos_val) = reduced_sin_cos(a.val);
                        let val = sin_val / cos_val;
                        let deriv = trig_argument_scale * a.deriv / (cos_val * cos_val);
                        (val, deriv, NumType::Transcendental)
                    }
                }
            } else {
                let cos_val = (trig_argument_scale * a.val).cos();
                if cos_val.abs() < 1e-10 {
                    return Err(EvalError::Overflow);
                }
                let val = (trig_argument_scale * a.val).tan();
                let deriv = trig_argument_scale * a.deriv / (cos_val * cos_val);
                (val, deriv, NumType::Transcendental)
            }
        }

        // Lambert W function (principal branch)
        LambertW => {
            let val = lambert_w(a.val)?;
            // d(W(a))/dx = W(a) / (a * (1 + W(a))) * da/dx
            // Special case: W'(0) = 1 (by L'Hôpital's rule, since W(x) ≈ x near 0)
            let deriv = if a.val.abs() < 1e-10 {
                a.deriv // W'(0) = 1
            } else {
                let denom = a.val * (1.0 + val);
                if denom.abs() > f64::MIN_POSITIVE {
                    val / denom * a.deriv
                } else {
                    if a.deriv == 0.0 {
                        0.0
                    } else {
                        a.deriv.signum() * f64::INFINITY
                    }
                }
            };
            (val, deriv, NumType::Transcendental)
        }

        // User functions are handled at the main evaluation loop level, not here
        // If we reach this point, return an error
        UserFunction0 | UserFunction1 | UserFunction2 | UserFunction3 | UserFunction4
        | UserFunction5 | UserFunction6 | UserFunction7 | UserFunction8 | UserFunction9
        | UserFunction10 | UserFunction11 | UserFunction12 | UserFunction13 | UserFunction14
        | UserFunction15 => {
            // This indicates a bug in the evaluation loop - user functions should be
            // handled before calling eval_unary
            return Err(EvalError::Invalid);
        }

        // Non-unary symbols should never be passed to this function
        _ => return Err(EvalError::Invalid),
    };

    Ok(StackEntry::new(val, deriv, num_type))
}

/// Evaluate a binary operator with derivative
fn eval_binary(sym: Symbol, a: StackEntry, b: StackEntry) -> Result<StackEntry, EvalError> {
    use Symbol::*;

    let (val, deriv, num_type) = match sym {
        // Addition: a + b
        Add => {
            let val = a.val + b.val;
            let deriv = a.deriv + b.deriv;
            let num_type = a.num_type.combine(b.num_type);
            (val, deriv, num_type)
        }

        // Subtraction: a - b
        Sub => {
            let val = a.val - b.val;
            let deriv = a.deriv - b.deriv;
            let num_type = a.num_type.combine(b.num_type);
            (val, deriv, num_type)
        }

        // Multiplication: a * b, d(ab)/dx = a*db/dx + b*da/dx
        Mul => {
            let val = a.val * b.val;
            let deriv = a.val * b.deriv + b.val * a.deriv;
            let num_type = a.num_type.combine(b.num_type);
            (val, deriv, num_type)
        }

        // Division: a / b, d(a/b)/dx = (b*da/dx - a*db/dx) / b²
        Div => {
            if b.val.abs() < f64::MIN_POSITIVE {
                return Err(EvalError::DivisionByZero);
            }
            let val = a.val / b.val;
            let deriv = (a.deriv - val * b.deriv) / b.val;
            let mut num_type = a.num_type.combine(b.num_type);
            if num_type == NumType::Integer {
                num_type = NumType::Rational;
            }
            (val, deriv, num_type)
        }

        // Power: a^b, d(a^b)/dx = a^b * (b*da/dx/a + ln(a)*db/dx)
        Pow => {
            if a.val <= 0.0 && b.val.fract() != 0.0 {
                return Err(EvalError::SqrtDomain);
            }
            let val = a.val.powf(b.val);
            if val.is_infinite() || val.is_nan() {
                return Err(EvalError::Overflow);
            }
            // Guard for near-zero base to avoid numerical issues
            let deriv = if a.val > f64::MIN_POSITIVE {
                val * (b.val * a.deriv / a.val + a.val.ln() * b.deriv)
            } else if a.val == 0.0 && b.val > 0.0 {
                if b.val == 1.0 {
                    a.deriv
                } else if b.val > 1.0 || a.deriv == 0.0 {
                    0.0
                } else {
                    a.deriv.signum() * f64::INFINITY
                }
            } else {
                // Negative base, integer exponent (or near-zero base treated as 0).
                // Full formula: val * (b * a.deriv/a + ln(a) * b.deriv).
                // The ln(a) * b.deriv term is intentionally dropped here: ln(negative) is
                // undefined in the reals (NaN), so it cannot contribute to Newton-Raphson.
                // Dropping it gives 0 for the derivative w.r.t. x-in-the-exponent path,
                // which is the correct safe fallback when x appears in the exponent of a
                // negative base (e.g., (-2)^x is only real-valued at integer x).
                if a.val.abs() < f64::MIN_POSITIVE {
                    0.0
                } else {
                    val * b.val * a.deriv / a.val
                }
            };
            let num_type = if b.num_type == NumType::Integer {
                a.num_type
            } else {
                NumType::Transcendental
            };
            (val, deriv, num_type)
        }

        // a-th root of b: b^(1/a)
        Root => {
            if a.val.abs() < f64::MIN_POSITIVE {
                return Err(EvalError::DivisionByZero);
            }
            let exp = 1.0 / a.val;

            // For negative radicands, we need to check if the index is an odd integer
            // Non-integer indices of negative numbers have no real value
            if b.val < 0.0 {
                // Only exact odd integer indices define a real root here.
                let rounded = a.val.round();
                let is_integer = a.val == rounded;

                if !is_integer {
                    // Non-integer index of negative number - no real value
                    return Err(EvalError::SqrtDomain);
                }

                // Odd roots of negatives are real. Below 2^52 the index fits in
                // i64 exactly. At and above 2^52, `as i64` saturates (2^63
                // becomes i64::MAX, which is odd) so parity comes from the
                // significand instead.
                let even = if rounded.abs() >= 4503599627370496.0 {
                    f64_exact_integer_is_even(rounded) != Some(false)
                } else {
                    let int_val = rounded as i64;
                    int_val % 2 == 0
                };
                if even {
                    return Err(EvalError::SqrtDomain);
                }
            }

            let val = if b.val < 0.0 {
                // Odd root of negative number
                -((-b.val).powf(exp))
            } else {
                b.val.powf(exp)
            };
            if val.is_infinite() || val.is_nan() {
                return Err(EvalError::Overflow);
            }
            // d(b^(1/a))/dx = b^(1/a) * (db/dx/(a*b) - ln(b)*da/dx/a²)
            let deriv = if b.val < 0.0 && a.deriv != 0.0 {
                // The value exists at this odd index, but varying the index
                // leaves the real domain arbitrarily close to the point.
                f64::NAN
            } else if b.val.abs() > f64::MIN_POSITIVE {
                val * (b.deriv / (a.val * b.val) - b.val.abs().ln() * a.deriv / (a.val * a.val))
            } else if b.val == 0.0 && exp == 1.0 {
                b.deriv
            } else if b.val == 0.0 && exp > 0.0 && exp < 1.0 && b.deriv != 0.0 {
                b.deriv.signum() * f64::INFINITY
            } else {
                0.0
            };
            (val, deriv, NumType::Algebraic)
        }

        // Logarithm base a of b: ln(b) / ln(a)
        Log => {
            if a.val <= 0.0 || a.val == 1.0 || b.val <= 0.0 {
                return Err(EvalError::LogDomain);
            }
            let ln_a = a.val.ln();
            let ln_b = b.val.ln();
            let val = ln_b / ln_a;
            // d(log_a(b))/dx = (db/dx/(b*ln(a)) - ln(b)*da/dx/(a*ln(a)²))
            let deriv = b.deriv / (b.val * ln_a) - ln_b * a.deriv / (a.val * ln_a * ln_a);
            (val, deriv, NumType::Transcendental)
        }

        // atan2(a, b) = angle of point (b, a) from origin
        Atan2 => {
            let val = a.val.atan2(b.val);
            // d(atan2(a,b))/dx = (b*da/dx - a*db/dx) / (a² + b²)
            let denom = a.val * a.val + b.val * b.val;
            let deriv = if denom.abs() > f64::MIN_POSITIVE {
                (b.val * a.deriv - a.val * b.deriv) / denom
            } else {
                0.0
            };
            (val, deriv, NumType::Transcendental)
        }

        // Non-binary symbols should never be passed to this function
        _ => return Err(EvalError::Invalid),
    };

    Ok(StackEntry::new(val, deriv, num_type))
}

/// Compute the Lambert W function (principal branch) using Halley's method
///
/// The Lambert W function satisfies W(x) * exp(W(x)) = x.
/// This implementation handles the principal branch (W₀) for x ≥ -1/e.
fn lambert_w(x: f64) -> Result<f64, EvalError> {
    // Branch point: x = -1/e gives W = -1
    const INV_E: f64 = 1.0 / std::f64::consts::E;
    const NEG_INV_E: f64 = -INV_E; // -0.36787944117144233...

    // Domain check
    if x < NEG_INV_E {
        return Err(EvalError::LogDomain);
    }

    // Special cases
    if x == 0.0 {
        return Ok(0.0); // W(0) = 0
    }
    if x == NEG_INV_E {
        return Ok(-1.0); // Closest representable branch point.
    }
    if x - NEG_INV_E < 1e-8 {
        // The stored branch point is slightly below the real -1/e. Keeping
        // this correction avoids cancellation in e*x + 1 near the branch.
        let delta = (x - NEG_INV_E) - 1.242_875_367_278_836_3e-17;
        let p = (2.0 * constants::E * delta).sqrt();
        return Ok(-1.0 + p * (1.0 + p * (-1.0 / 3.0 + p * (11.0 / 72.0 - p * 43.0 / 540.0))));
    }
    if x == constants::E {
        return Ok(1.0); // W(e) = 1
    }

    // Initial guess - different approximations for different regimes
    let mut w = if x < -0.3 {
        // Near the branch point, use a series expansion around -1/e
        // W(x) ≈ -1 + p - p²/3 + 11p³/72 where p = sqrt(2(ex + 1))
        let p = (2.0 * (constants::E * x + 1.0)).sqrt();
        -1.0 + p * (1.0 - p / 3.0 * (1.0 - 11.0 * p / 72.0))
    } else if x < 0.25 {
        // Near zero, use a polynomial approximation
        // W(x) ≈ x - x² + 3x³/2 - 8x⁴/3 + ...
        // For numerical stability, use a rational approximation
        let x2 = x * x;
        x * (1.0 - x + x2 * (1.5 - 2.6667 * x))
    } else if x < 4.0 {
        // Moderate range: use log-based approximation
        // W(x) ≈ ln(x) - ln(ln(x)) + ln(ln(x))/ln(x)
        let lnx = x.ln();
        if lnx > 0.0 {
            let lnlnx = lnx.ln().max(0.0);
            lnx - lnlnx + lnlnx / lnx.max(1.0)
        } else {
            x // fallback for x near 1
        }
    } else {
        // Large x: W(x) ≈ ln(x) - ln(ln(x)) + ln(ln(x))/ln(x)
        let l1 = x.ln();
        let l2 = l1.ln();
        l1 - l2 + l2 / l1
    };

    // Halley's method iteration
    // For well-chosen initial guesses, 10-15 iterations are usually enough
    for _ in 0..25 {
        let ew = w.exp();

        // Handle potential overflow
        if !ew.is_finite() {
            // Back off to a more stable approach
            w = x.ln() - w.ln().max(1e-10);
            continue;
        }

        let wew = w * ew;
        let diff = wew - x;

        // Convergence check with relative tolerance
        let tol = 1e-15 * (1.0 + w.abs().max(x.abs()));
        if diff.abs() < tol {
            break;
        }

        let w1 = w + 1.0;
        // Halley's correction
        let denom = ew * w1 - 0.5 * (w + 2.0) * diff / w1;
        if denom.abs() < f64::MIN_POSITIVE {
            break;
        }

        let delta = diff / denom;

        // Damping for stability near branch point
        let correction = if w < -0.5 && delta.abs() > 0.5 {
            delta * 0.5 // Damped update near branch point
        } else {
            delta
        };

        w -= correction;
    }

    // Final validation
    if !w.is_finite() {
        return Err(EvalError::Overflow);
    }

    Ok(w)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-10
    }

    #[test]
    fn trig_reduces_large_arguments_without_snapping_neighbors() {
        let sin = Expression::parse("xS").unwrap();
        let cos = Expression::parse("xC").unwrap();
        let tan = Expression::parse("xT").unwrap();
        let small = 1e-13;
        assert!(
            (evaluate(&sin, small).unwrap().value / (constants::PI * small) - 1.0).abs() < 1e-14
        );
        let x = 1_000_000_000_000.25;
        let expected = std::f64::consts::FRAC_1_SQRT_2;
        assert!((evaluate(&sin, x).unwrap().value - expected).abs() < 1e-15);
        assert!((evaluate(&cos, x).unwrap().value - expected).abs() < 1e-15);
        assert!((evaluate(&tan, x).unwrap().value - 1.0).abs() < 1e-15);
        let near_pole = 0.5_f64.next_down();
        let tan_near_pole = evaluate(&tan, near_pole).unwrap().value;
        let expected_pole = 1.0 / (constants::PI * (0.5 - near_pole));
        assert!((tan_near_pole / expected_pole - 1.0).abs() < 1e-15);
    }

    #[test]
    fn lambert_w_preserves_near_branch_values_and_slopes() {
        let expr = Expression::parse("xW").unwrap();
        let x = -0.367_879_441_171_441_8;
        let result = evaluate(&expr, x).unwrap();
        assert!(
            (result.value - (-0.999_999_945_683_066_6)).abs() < 1e-15,
            "{}",
            result.value
        );
        assert!(result.derivative > 1e7 && result.derivative.is_finite());
        assert!(evaluate(&expr, (-1.0 / constants::E).next_down()).is_err());
    }

    #[test]
    fn derivatives_at_zero_and_tiny_divisors() {
        for postfix in ["x1^", "1xv"] {
            let result = evaluate(&Expression::parse(postfix).unwrap(), 0.0).unwrap();
            assert_eq!(result.derivative, 1.0, "{postfix}");
        }
        let quotient = Expression::parse("xx/").unwrap();
        assert_eq!(evaluate(&quotient, 1e-200).unwrap().derivative, 0.0);
    }

    #[test]
    fn test_basic_eval() {
        let expr = Expression::parse("32+").unwrap();
        let result = evaluate(&expr, 0.0).unwrap();
        assert!(approx_eq(result.value, 5.0));
        assert!(approx_eq(result.derivative, 0.0));
    }

    #[test]
    fn test_variable() {
        let expr = Expression::parse("x").unwrap();
        let result = evaluate(&expr, 3.5).unwrap();
        assert!(approx_eq(result.value, 3.5));
        assert!(approx_eq(result.derivative, 1.0));
    }

    #[test]
    fn test_x_squared() {
        let expr = Expression::parse("xs").unwrap(); // x^2
        let result = evaluate(&expr, 3.0).unwrap();
        assert!(approx_eq(result.value, 9.0));
        assert!(approx_eq(result.derivative, 6.0)); // 2x
    }

    #[test]
    fn test_sqrt_pi() {
        let expr = Expression::parse("pq").unwrap(); // sqrt(pi)
        let result = evaluate(&expr, 0.0).unwrap();
        assert!(approx_eq(result.value, constants::PI.sqrt()));
    }

    #[test]
    fn test_e_to_x() {
        let expr = Expression::parse("xE").unwrap(); // e^x
        let result = evaluate(&expr, 1.0).unwrap();
        assert!(approx_eq(result.value, constants::E));
        assert!(approx_eq(result.derivative, constants::E)); // d(e^x)/dx = e^x
    }

    #[test]
    fn test_complex_expr() {
        // x^2 + 2*x + 1 = (x+1)^2
        let expr = Expression::parse("xs2x*+1+").unwrap();
        let result = evaluate(&expr, 3.0).unwrap();
        assert!(approx_eq(result.value, 16.0)); // (3+1)^2
        assert!(approx_eq(result.derivative, 8.0)); // 2x + 2 = 8
    }

    #[test]
    fn test_lambert_w() {
        // W(1) ≈ 0.5671432904
        let w = lambert_w(1.0).unwrap();
        assert!((w - 0.5671432904).abs() < 1e-9);

        // W(e) = 1
        let w = lambert_w(constants::E).unwrap();
        assert!((w - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_sinpi_snaps_exact_integer_and_half_integer_arguments() {
        let integer_expr = Expression::parse("xS").unwrap();
        let integer_result = evaluate(&integer_expr, 1.0).unwrap();
        assert_eq!(integer_result.value, 0.0);
        assert!(approx_eq(integer_result.derivative, -constants::PI));

        let half_integer_expr = Expression::parse("12/S").unwrap();
        let half_integer_result = evaluate(&half_integer_expr, 0.0).unwrap();
        assert_eq!(half_integer_result.value, 1.0);
        assert_eq!(half_integer_result.derivative, 0.0);
    }

    #[test]
    fn test_cospi_snaps_exact_integer_and_half_integer_arguments() {
        let integer_expr = Expression::parse("xC").unwrap();
        let integer_result = evaluate(&integer_expr, 1.0).unwrap();
        assert_eq!(integer_result.value, -1.0);
        assert_eq!(integer_result.derivative, 0.0);

        let half_integer_expr = Expression::parse("xC").unwrap();
        let half_integer_result = evaluate(&half_integer_expr, 0.5).unwrap();
        assert_eq!(half_integer_result.value, 0.0);
        assert!(approx_eq(half_integer_result.derivative, -constants::PI));
    }

    #[test]
    fn test_tanpi_snaps_exact_integer_and_half_integer_arguments() {
        let integer_expr = Expression::parse("xT").unwrap();
        let integer_result = evaluate(&integer_expr, 1.0).unwrap();
        assert_eq!(integer_result.value, 0.0);
        assert!(approx_eq(integer_result.derivative, constants::PI));

        let half_integer_expr = Expression::parse("12/T").unwrap();
        assert!(matches!(
            evaluate(&half_integer_expr, 0.0),
            Err(EvalError::Overflow)
        ));
    }

    #[test]
    fn test_trig_pi_large_integers_keep_parity_without_saturating_casts() {
        let sin_expr = Expression::parse("xS").unwrap();
        let cos_expr = Expression::parse("xC").unwrap();
        let tan_expr = Expression::parse("xT").unwrap();
        // +2^62 used to make tanpi overflow: 2 * 2^62 saturates to i64::MAX,
        // which is odd, so the half-integer snap treated it as a pole.
        // -2^62 saturated to i64::MIN (even) and was accepted.
        for exp in [52, 53, 58, 62, 63, 64, 100, 1023] {
            let magnitude = 2.0_f64.powi(exp);
            for x in [magnitude, -magnitude] {
                let sin_result = evaluate(&sin_expr, x).unwrap();
                assert_eq!(sin_result.value, 0.0, "sinpi(2^{exp}) at {x}");
                assert!(
                    approx_eq(sin_result.derivative, constants::PI),
                    "sinpi derivative at {x}: {}",
                    sin_result.derivative
                );
                let cos_result = evaluate(&cos_expr, x).unwrap();
                assert_eq!(cos_result.value, 1.0, "cospi at {x}");
                assert_eq!(cos_result.derivative, 0.0);
                let tan_result = evaluate(&tan_expr, x).unwrap();
                assert_eq!(tan_result.value, 0.0, "tanpi at {x}");
                assert!(approx_eq(tan_result.derivative, constants::PI));
            }
        }
        // 2^52 + 1 is the last odd integer that is an exact f64.
        let odd = 2.0_f64.powi(52) + 1.0;
        assert_eq!(evaluate(&sin_expr, odd).unwrap().value, 0.0);
        assert!(approx_eq(
            evaluate(&sin_expr, odd).unwrap().derivative,
            -constants::PI
        ));
        assert_eq!(evaluate(&cos_expr, odd).unwrap().value, -1.0);
        assert_eq!(evaluate(&tan_expr, odd).unwrap().value, 0.0);
        // A real half-integer is still a pole. A large finite value is not.
        assert!(matches!(
            evaluate(&tan_expr, 10.5),
            Err(EvalError::Overflow)
        ));
        assert!(matches!(
            evaluate(&tan_expr, -1.5),
            Err(EvalError::Overflow)
        ));
    }

    #[test]
    fn negative_root_requires_a_fixed_exact_odd_index() {
        let variable_index = Expression::parse("x8nv").unwrap();
        assert!(evaluate(&variable_index, 3.00000000005).is_err());
        let at_odd_index = evaluate(&variable_index, 3.0).unwrap();
        assert!((at_odd_index.value + 2.0).abs() < 1e-12);
        assert!(!at_odd_index.derivative.is_finite());
        let fixed_index = Expression::parse("38nv").unwrap();
        assert!((evaluate(&fixed_index, 0.0).unwrap().value + 2.0).abs() < 1e-12);
    }

    #[test]
    fn test_even_root_index_of_negative_is_rejected_past_i64() {
        // 3rd root of -8 is -2.
        let cube = Expression::parse("38nv").unwrap();
        let cube_root = evaluate(&cube, 0.0).unwrap().value;
        assert!((cube_root + 2.0).abs() < 1e-9, "{cube_root}");

        // Index is x. 2^63 is even, but `as i64` saturates to i64::MAX (odd)
        // and used to accept the root.
        let indexed = Expression::parse("x8nv").unwrap();
        assert!(matches!(
            evaluate(&indexed, 2.0_f64.powi(63)),
            Err(EvalError::SqrtDomain)
        ));
        assert!(matches!(
            evaluate(&indexed, -(2.0_f64.powi(62))),
            Err(EvalError::SqrtDomain)
        ));
        let odd = evaluate(&indexed, 2.0_f64.powi(52) + 1.0).unwrap();
        assert!(odd.value < 0.0, "odd root of a negative stays real");
        assert!(!odd.derivative.is_finite());
        assert!(matches!(
            evaluate(&indexed, 4.0),
            Err(EvalError::SqrtDomain)
        ));
    }

    #[test]
    fn test_f64_integer_parity_matches_small_values_and_large_powers_of_two() {
        assert_eq!(f64_exact_integer_is_even(0.0), Some(true));
        assert_eq!(f64_exact_integer_is_even(1.0), Some(false));
        assert_eq!(f64_exact_integer_is_even(-7.0), Some(false));
        assert_eq!(f64_exact_integer_is_even(8.0), Some(true));
        assert_eq!(f64_exact_integer_is_even(2.5), None);
        assert_eq!(f64_exact_integer_is_even(0.5), None);
        let odd = 2.0_f64.powi(52) + 1.0;
        assert_eq!(f64_exact_integer_is_even(odd), Some(false));
        assert_eq!(f64_exact_integer_is_even(-odd), Some(false));
        assert_eq!(f64_exact_integer_is_even(2.0_f64.powi(53)), Some(true));
        assert_eq!(f64_exact_integer_is_even(2.0_f64.powi(63)), Some(true));
        assert_eq!(f64_exact_integer_is_even(f64::MAX), Some(true));
        assert_eq!(f64_exact_integer_is_even(f64::INFINITY), None);
    }

    #[test]
    fn test_custom_trig_argument_scale_preserves_raw_trig_behavior() {
        let expr = Expression::parse("xS").unwrap();
        let context = EvalContext::new().with_trig_argument_scale(1.0);
        let result = evaluate_with_context(&expr, 1.0, &context).unwrap();

        assert!(approx_eq(result.value, 1.0_f64.sin()));
        assert!(approx_eq(result.derivative, 1.0_f64.cos()));
    }

    #[test]
    fn test_exact_trig_zero_rejects_pathological_negative_power_branch() {
        let expr = Expression::parse("1Sxn^S").unwrap();
        assert!(matches!(
            evaluate(&expr, 2.506314),
            Err(EvalError::SqrtDomain)
        ));
    }

    #[test]
    // Ten-digit user-constant fixture, not std::f64::consts::EULER_GAMMA.
    #[allow(clippy::approx_constant)]
    fn test_user_constant_evaluation() {
        use crate::profile::UserConstant;

        // Create a user constant (Euler-Mascheroni gamma ≈ 0.57721)
        let user_constants = vec![UserConstant {
            weight: 8,
            name: "g".to_string(),
            description: "gamma".to_string(),
            value: 0.5772156649,
            num_type: NumType::Transcendental,
        }];

        // Create expression with UserConstant0 (byte 128)
        let expr = Expression::from_symbols(&[Symbol::UserConstant0]);

        // Evaluate with user constants
        let result = evaluate_with_constants(&expr, 0.0, &user_constants).unwrap();

        // Should match the user constant value
        assert!(approx_eq(result.value, 0.5772156649));
        // Derivative should be 0 (it's a constant)
        assert!(approx_eq(result.derivative, 0.0));
    }

    #[test]
    fn test_user_constant_in_expression() {
        use crate::profile::UserConstant;

        // Create two user constants
        let user_constants = vec![
            UserConstant {
                weight: 8,
                name: "a".to_string(),
                description: "constant a".to_string(),
                value: 2.0,
                num_type: NumType::Integer,
            },
            UserConstant {
                weight: 8,
                name: "b".to_string(),
                description: "constant b".to_string(),
                value: 3.0,
                num_type: NumType::Integer,
            },
        ];

        // Create expression: u0 * x + u1 (in postfix: u0 x * u1 +)
        let expr = Expression::from_symbols(&[
            Symbol::UserConstant0,
            Symbol::X,
            Symbol::Mul,
            Symbol::UserConstant1,
            Symbol::Add,
        ]);

        // At x=4, should be 2*4 + 3 = 11
        let result = evaluate_with_constants(&expr, 4.0, &user_constants).unwrap();
        assert!(approx_eq(result.value, 11.0));
        // Derivative should be 2 (from u0 * x)
        assert!(approx_eq(result.derivative, 2.0));
    }

    #[test]
    fn test_user_constant_missing_returns_error() {
        // Missing user constant slots must fail explicitly instead of silently
        // changing the expression's meaning.
        let expr = Expression::from_symbols(&[Symbol::UserConstant0]);

        let result = evaluate_with_constants(&expr, 0.0, &[]);
        assert!(matches!(result, Err(EvalError::MissingUserConstant(0))));
    }

    #[test]
    fn test_user_function_sinh() {
        use crate::udf::UserFunction;

        // sinh(x) = (e^x - e^-x) / 2
        // In postfix: E|r-2/ (exp, dup, recip, subtract, 2, divide)
        let user_functions = vec![UserFunction::parse("4:sinh:hyperbolic sine:E|r-2/").unwrap()];

        // Create expression: sinh(x) (in postfix: xF0 where F0 = UserFunction0)
        let expr = Expression::from_symbols(&[Symbol::X, Symbol::UserFunction0]);

        // sinh(1) = (e - e^-1) / 2 ≈ 1.1752
        let result =
            evaluate_with_constants_and_functions(&expr, 1.0, &[], &user_functions).unwrap();
        let expected = (constants::E - 1.0 / constants::E) / 2.0;
        assert!(approx_eq(result.value, expected));

        // Derivative: d(sinh(x))/dx = cosh(x) = (e^x + e^-x) / 2
        let expected_deriv = (constants::E + 1.0 / constants::E) / 2.0;
        assert!((result.derivative - expected_deriv).abs() < 1e-10);
    }

    #[test]
    fn test_user_function_xex() {
        use crate::udf::UserFunction;

        // XeX(x) = x * e^x
        // In postfix: |E* (dup, exp, multiply)
        let user_functions = vec![UserFunction::parse("4:XeX:x*exp(x):|E*").unwrap()];

        // Create expression: XeX(x) (in postfix: xF0 where F0 = UserFunction0)
        let expr = Expression::from_symbols(&[Symbol::X, Symbol::UserFunction0]);

        // XeX(1) = 1 * e^1 = e
        let result =
            evaluate_with_constants_and_functions(&expr, 1.0, &[], &user_functions).unwrap();
        assert!(approx_eq(result.value, constants::E));

        // Derivative: d(x*e^x)/dx = e^x + x*e^x = e^x * (1 + x) = e * 2
        let expected_deriv = constants::E * 2.0;
        assert!((result.derivative - expected_deriv).abs() < 1e-10);
    }

    #[test]
    fn test_user_function_missing_returns_error() {
        // When no user functions are provided, user function evaluation should fail
        let expr = Expression::from_symbols(&[Symbol::X, Symbol::UserFunction0]);

        let result = evaluate_with_constants_and_functions(&expr, 1.0, &[], &[]);
        assert!(result.is_err());
    }
}

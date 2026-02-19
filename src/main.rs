//! RIES-RS: Find algebraic equations given their solution
//!
//! A Rust implementation of Robert Munafo's RIES program.

// Allow field reassignment with default in test code - common pattern for config building
#![cfg_attr(test, allow(clippy::field_reassign_with_default))]
// Allow unused code in main - some helper functions are kept for future use
#![allow(dead_code)]

mod eval;
mod expr;
mod fast_match;
mod gen;
mod highprec_verify;
mod manifest;
mod metrics;
mod pool;
#[cfg(feature = "highprec")]
mod precision;
mod presets;
mod profile;
mod pslq;
mod report;
mod search;
mod stability;
mod symbol;
mod symbol_table;
mod thresholds;
mod udf;

mod cli;

use clap::Parser;
use cli::{
    build_gen_config, canon_reduction_enabled, compute_significant_digits_tolerance, format_value,
    parse_diagnostics, parse_display_format, parse_memory_size_bytes, parse_symbol_names_from_cli,
    parse_symbol_sets, parse_symbol_weights_from_cli, parse_user_constant_from_cli,
    parse_user_function_from_cli, print_footer, print_header, print_match_absolute,
    print_match_relative, print_show_work_details, print_symbol_table, run_search, Args,
    DisplayFormat,
};
use manifest::{MatchInfo, RunManifest, SearchConfigInfo};
use profile::Profile;
use report::{Report, ReportConfig};
use std::time::Instant;

// Args struct is now imported from cli::Args

/// Evaluate an expression string and return the result
fn eval_expression(
    expr_str: &str,
    x: f64,
    user_constants: &[profile::UserConstant],
    user_functions: &[udf::UserFunction],
) -> Result<eval::EvalResult, eval::EvalError> {
    use expr::Expression;
    let expr = Expression::parse(expr_str).ok_or(eval::EvalError::Invalid)?;
    eval::evaluate_with_constants_and_functions(&expr, x, user_constants, user_functions)
}

fn match_in_equate_bounds(
    m: &search::Match,
    min_equate_value: Option<f64>,
    max_equate_value: Option<f64>,
) -> bool {
    let lhs = m.lhs.value;
    let rhs = m.rhs.value;
    let min_ok = min_equate_value.is_none_or(|min| lhs >= min && rhs >= min);
    let max_ok = max_equate_value.is_none_or(|max| lhs <= max && rhs <= max);
    min_ok && max_ok
}

#[derive(Debug, Clone, Copy)]
struct ExpressionConstraintOptions {
    rational_exponents: bool,
    rational_trig_args: bool,
    max_trig_cycles: Option<u32>,
    user_constant_types: [symbol::NumType; 16],
    user_function_types: [symbol::NumType; 16],
}

fn expression_respects_constraints(
    expression: &expr::Expression,
    opts: ExpressionConstraintOptions,
) -> bool {
    use symbol::{NumType, Seft, Symbol};

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

#[derive(Clone)]
enum SolveNodeKind {
    Atom,
    Unary(symbol::Symbol, Box<SolveNode>),
    Binary(symbol::Symbol, Box<SolveNode>, Box<SolveNode>),
}

#[derive(Clone)]
struct SolveNode {
    expr: expr::Expression,
    x_count: u32,
    kind: SolveNodeKind,
}

fn append_unary_expression(base: &expr::Expression, op: symbol::Symbol) -> expr::Expression {
    let mut out = base.clone();
    out.push(op);
    out
}

fn combine_binary_expressions(
    lhs: &expr::Expression,
    rhs: &expr::Expression,
    op: symbol::Symbol,
) -> expr::Expression {
    let mut out = expr::Expression::new();
    for &sym in lhs.symbols() {
        out.push(sym);
    }
    for &sym in rhs.symbols() {
        out.push(sym);
    }
    out.push(op);
    out
}

fn build_solve_ast(expression: &expr::Expression) -> Option<SolveNode> {
    use symbol::{Seft, Symbol};

    let mut stack: Vec<SolveNode> = Vec::with_capacity(expression.len());

    for &sym in expression.symbols() {
        match sym.seft() {
            Seft::A => {
                let mut e = expr::Expression::new();
                e.push(sym);
                stack.push(SolveNode {
                    expr: e,
                    x_count: u32::from(sym == Symbol::X),
                    kind: SolveNodeKind::Atom,
                });
            }
            Seft::B => {
                let arg = stack.pop()?;
                let mut e = arg.expr.clone();
                e.push(sym);
                stack.push(SolveNode {
                    expr: e,
                    x_count: arg.x_count,
                    kind: SolveNodeKind::Unary(sym, Box::new(arg)),
                });
            }
            Seft::C => {
                let rhs = stack.pop()?;
                let lhs = stack.pop()?;
                let mut e = expr::Expression::new();
                for &s in lhs.expr.symbols() {
                    e.push(s);
                }
                for &s in rhs.expr.symbols() {
                    e.push(s);
                }
                e.push(sym);
                stack.push(SolveNode {
                    expr: e,
                    x_count: lhs.x_count.saturating_add(rhs.x_count),
                    kind: SolveNodeKind::Binary(sym, Box::new(lhs), Box::new(rhs)),
                });
            }
        }
    }

    if stack.len() == 1 {
        stack.pop()
    } else {
        None
    }
}

fn constant_expression(sym: symbol::Symbol) -> expr::Expression {
    let mut out = expr::Expression::new();
    out.push(sym);
    out
}

fn unary_inverse_expression(
    op: symbol::Symbol,
    rhs_value: &expr::Expression,
) -> Option<expr::Expression> {
    use symbol::Symbol;

    Some(match op {
        Symbol::Neg => append_unary_expression(rhs_value, Symbol::Neg),
        Symbol::Recip => append_unary_expression(rhs_value, Symbol::Recip),
        Symbol::Square => append_unary_expression(rhs_value, Symbol::Sqrt),
        Symbol::Sqrt => append_unary_expression(rhs_value, Symbol::Square),
        Symbol::Ln => append_unary_expression(rhs_value, Symbol::Exp),
        Symbol::Exp => append_unary_expression(rhs_value, Symbol::Ln),
        Symbol::TanPi => {
            // x = atan(rhs) / pi = atan2(rhs, 1) / pi
            let one = constant_expression(symbol::Symbol::One);
            let atan = combine_binary_expressions(rhs_value, &one, symbol::Symbol::Atan2);
            let pi = constant_expression(symbol::Symbol::Pi);
            combine_binary_expressions(&atan, &pi, symbol::Symbol::Div)
        }
        Symbol::SinPi => {
            // x = asin(rhs)/pi = atan2(rhs, sqrt(1-rhs^2))/pi
            let one = constant_expression(symbol::Symbol::One);
            let rhs_sq = append_unary_expression(rhs_value, symbol::Symbol::Square);
            let inner = combine_binary_expressions(&one, &rhs_sq, symbol::Symbol::Sub);
            let denom = append_unary_expression(&inner, symbol::Symbol::Sqrt);
            let atan = combine_binary_expressions(rhs_value, &denom, symbol::Symbol::Atan2);
            let pi = constant_expression(symbol::Symbol::Pi);
            combine_binary_expressions(&atan, &pi, symbol::Symbol::Div)
        }
        Symbol::CosPi => {
            // x = acos(rhs)/pi = atan2(sqrt(1-rhs^2), rhs)/pi
            let one = constant_expression(symbol::Symbol::One);
            let rhs_sq = append_unary_expression(rhs_value, symbol::Symbol::Square);
            let inner = combine_binary_expressions(&one, &rhs_sq, symbol::Symbol::Sub);
            let numer = append_unary_expression(&inner, symbol::Symbol::Sqrt);
            let atan = combine_binary_expressions(&numer, rhs_value, symbol::Symbol::Atan2);
            let pi = constant_expression(symbol::Symbol::Pi);
            combine_binary_expressions(&atan, &pi, symbol::Symbol::Div)
        }
        Symbol::LambertW => {
            // x = W(y)  =>  y = x * exp(x)
            let exp_rhs = append_unary_expression(rhs_value, symbol::Symbol::Exp);
            combine_binary_expressions(rhs_value, &exp_rhs, symbol::Symbol::Mul)
        }
        _ => return None,
    })
}

fn invert_binary_left(
    op: symbol::Symbol,
    rhs_value: &expr::Expression,
    known_right: &expr::Expression,
) -> Option<expr::Expression> {
    use symbol::Symbol;
    Some(match op {
        Symbol::Add => combine_binary_expressions(rhs_value, known_right, Symbol::Sub),
        Symbol::Sub => combine_binary_expressions(rhs_value, known_right, Symbol::Add),
        Symbol::Mul => combine_binary_expressions(rhs_value, known_right, Symbol::Div),
        Symbol::Div => combine_binary_expressions(rhs_value, known_right, Symbol::Mul),
        Symbol::Pow => combine_binary_expressions(known_right, rhs_value, Symbol::Root),
        Symbol::Root => combine_binary_expressions(rhs_value, known_right, Symbol::Log),
        Symbol::Log => combine_binary_expressions(rhs_value, known_right, Symbol::Root),
        _ => return None,
    })
}

fn invert_binary_right(
    op: symbol::Symbol,
    known_left: &expr::Expression,
    rhs_value: &expr::Expression,
) -> Option<expr::Expression> {
    use symbol::Symbol;
    Some(match op {
        Symbol::Add => combine_binary_expressions(rhs_value, known_left, Symbol::Sub),
        Symbol::Sub => combine_binary_expressions(known_left, rhs_value, Symbol::Sub),
        Symbol::Mul => combine_binary_expressions(rhs_value, known_left, Symbol::Div),
        Symbol::Div => combine_binary_expressions(known_left, rhs_value, Symbol::Div),
        Symbol::Pow => combine_binary_expressions(known_left, rhs_value, Symbol::Log),
        Symbol::Root => combine_binary_expressions(rhs_value, known_left, Symbol::Pow),
        Symbol::Log => combine_binary_expressions(known_left, rhs_value, Symbol::Pow),
        _ => return None,
    })
}

fn is_x_atom(expression: &expr::Expression) -> bool {
    use symbol::Symbol;
    expression.len() == 1
        && expression
            .symbols()
            .first()
            .is_some_and(|sym| *sym == Symbol::X)
}

fn solve_for_x_rhs_expression(
    lhs: &expr::Expression,
    rhs: &expr::Expression,
) -> Option<expr::Expression> {
    use symbol::Symbol;

    if lhs.count_symbol(Symbol::X) != 1 {
        return None;
    }

    let mut node = build_solve_ast(lhs)?;
    if node.x_count != 1 {
        return None;
    }
    let mut rhs_expr = rhs.clone();

    loop {
        match node.kind {
            SolveNodeKind::Atom => return is_x_atom(&node.expr).then_some(rhs_expr),
            SolveNodeKind::Unary(op, child) => {
                if child.x_count != 1 {
                    return None;
                }
                rhs_expr = unary_inverse_expression(op, &rhs_expr)?;
                node = *child;
            }
            SolveNodeKind::Binary(op, left, right) => {
                let lx = left.x_count;
                let rx = right.x_count;
                if lx + rx != 1 {
                    return None;
                }

                if lx == 1 {
                    rhs_expr = invert_binary_left(op, &rhs_expr, &right.expr)?;
                    node = *left;
                } else {
                    rhs_expr = invert_binary_right(op, &left.expr, &rhs_expr)?;
                    node = *right;
                }
            }
        }
    }
}

fn symbol_key(sym: symbol::Symbol) -> String {
    let byte = sym as u8;
    if byte.is_ascii_graphic() {
        (byte as char).to_string()
    } else {
        format!("#{}", byte)
    }
}

fn canonical_node_key(node: &SolveNode) -> String {
    use symbol::Symbol;

    match &node.kind {
        SolveNodeKind::Atom => node.expr.to_postfix(),
        SolveNodeKind::Unary(op, child) => {
            format!("{}({})", symbol_key(*op), canonical_node_key(child))
        }
        SolveNodeKind::Binary(op, left, right) => {
            let mut lk = canonical_node_key(left);
            let mut rk = canonical_node_key(right);
            if matches!(op, Symbol::Add | Symbol::Mul) && lk > rk {
                std::mem::swap(&mut lk, &mut rk);
            }
            format!("({}{}{})", lk, symbol_key(*op), rk)
        }
    }
}

fn canonical_expression_key(expression: &expr::Expression) -> Option<String> {
    let node = build_solve_ast(expression)?;
    Some(canonical_node_key(&node))
}

fn digit_signature(expression: &expr::Expression) -> String {
    let mut digits: Vec<char> = expression
        .symbols()
        .iter()
        .filter_map(|sym| {
            let b = *sym as u8;
            (b'1'..=b'9').contains(&b).then_some(b as char)
        })
        .collect();
    digits.sort_unstable();
    digits.into_iter().collect()
}

fn match_is_numeric_anagram(m: &search::Match) -> bool {
    let lhs = digit_signature(&m.lhs.expr);
    let rhs = digit_signature(&m.rhs.expr);
    !lhs.is_empty() && lhs == rhs
}

fn main() {
    let args = Args::parse();

    if args.list_options {
        let opts = [
            "--list-options",
            "-p",
            "--include",
            "--any-exponents",
            "--any-subexpressions",
            "--any-trig-args",
            "--canon-reduction",
            "--canon-simplify",
            "--derivative-margin",
            "--eval-expression",
            "--explicit-multiply",
            "--find-expression",
            "--match-all-digits",
            "--mad",
            "--max-equate-value",
            "--max-match-distance",
            "--min-match-distance",
            "--max-matches",
            "--max-memory",
            "--memory-abort-threshold",
            "-X",
            "--constant",
            "--define",
            "--min-equate-value",
            "--max-trig-cycles",
            "--min-memory",
            "--no-canon-simplify",
            "--no-refinement",
            "--no-slow-messages",
            "--no-solve-for-x",
            "--numeric-anagram",
            "--one-sided",
            "--complexity-ranking",
            "--parity-ranking",
            "--rational-exponents",
            "--rational-trig-args",
            "--show-work",
            "--significance-loss-margin",
            "--symbol-weights",
            "--symbol-names",
            "--trig-argument-scale",
            "-s",
            "--try-solve-for-x",
            "--version",
            "--wide",
            "--wide-output",
            "-a",
            "--algebraic-subexpressions",
            "-c",
            "--constructible-subexpressions",
            "-D",
            "-E",
            "-F",
            "-i",
            "--integer-subexpressions",
            "-l",
            "--liouvillian-subexpressions",
            "-N",
            "-O",
            "-r",
            "--rational-subexpressions",
            "-S",
            "-x",
            "--absolute-roots",
            "--relative-roots",
            "--N-RHS",
            "--O-RHS",
            "--S-RHS",
            "--E-RHS",
        ];
        for opt in opts {
            println!("{}", opt);
        }
        return;
    }

    // Handle --list-presets (print available presets and exit)
    if args.list_presets {
        presets::print_presets();
        return;
    }

    // Handle -S without argument (print symbol table and exit)
    // When -S is used with num_args=0..=1, bare -S gives Some("") and -S with value gives Some(value)
    // Also check if target is None to distinguish from "-S symbols target"
    // Note: clap's num_args=0..=1 with a positional arg means -S alone could also give None
    // if the positional target is consumed instead
    let is_bare_s = (args.only_symbols.as_ref().is_some_and(|s| s.is_empty())
        && args.target.is_none())
        || (args.only_symbols.is_none()
            && args.target.is_none()
            && std::env::args().any(|a| a == "-S"));
    if is_bare_s {
        print_symbol_table();
        return;
    }

    let _compat_noop = (args.wide, args.wide_output, args.relative_roots);
    let diagnostics = parse_diagnostics(args.diagnostics.as_deref(), args.show_work, args.stats);

    if !args.no_slow_messages && !diagnostics.unsupported_channels.is_empty() {
        let unsupported: String = diagnostics.unsupported_channels.iter().collect();
        eprintln!(
            "Warning: -D channels not implemented in ries-rs yet: {}",
            unsupported
        );
    }

    // Check precision flag - warn if highprec feature not enabled
    #[cfg(not(feature = "highprec"))]
    if !args.no_slow_messages && args.precision.is_some() {
        eprintln!(
            "Warning: --precision flag specified but ries-rs was not compiled with 'highprec' feature."
        );
        eprintln!("         Recompile with: cargo build --features highprec");
        eprintln!("         Using standard f64 precision (~15 digits) for verification.");
    }

    if let Some(scale) = args.trig_argument_scale {
        if scale.is_finite() && scale != 0.0 {
            eval::set_trig_argument_scale(scale);
        } else if !args.no_slow_messages {
            eprintln!(
                "Warning: --trig-argument-scale must be finite and non-zero (got {}).",
                scale
            );
        }
    }

    // Handle -p legacy semantics: if profile looks like a number and no target, treat as target
    // Original ries behavior: "ries -p 2.5" means "use default profile and search for 2.5"
    let (profile_arg, resolved_target) = if let Some(ref profile_path) = args.profile {
        if args.target.is_none() {
            // Check if profile argument looks like a target (numeric)
            if let Ok(val) = profile_path.to_string_lossy().parse::<f64>() {
                // It's a number, treat as target and use default profile
                (None, Some(val))
            } else {
                // Not a number, use as profile path
                (args.profile.clone(), args.target)
            }
        } else {
            // Both -p and target provided, use both normally
            (args.profile.clone(), args.target)
        }
    } else {
        (None, args.target)
    };

    // Handle -E legacy semantics: if enable looks like a number and no target, treat as target
    // Original ries behavior: "ries -E 2.5" means "enable all and search for 2.5"
    let (enable_arg, resolved_target) = if let Some(ref enable_str) = args.enable {
        if resolved_target.is_none() {
            // Check if enable argument looks like a target (numeric)
            if let Ok(val) = enable_str.parse::<f64>() {
                // It's a number, treat as target and use "all" for enable
                (Some("all".to_string()), Some(val))
            } else {
                // Not a number, use as enable string
                (args.enable.clone(), resolved_target)
            }
        } else {
            // Both -E and target provided, use both normally
            (args.enable.clone(), resolved_target)
        }
    } else {
        (None, resolved_target)
    };

    // Handle -l legacy semantics: if level looks like a float and no target, treat as target + liouvillian
    // Original ries: "-l 2.5" means liouvillian mode + target 2.5
    // "-l3" or "--level 3" with an explicit target means level 3
    let (level_value, liouvillian_override, final_target) = if resolved_target.is_some() {
        // Target was explicitly provided, use -l as level
        let level = args.level.parse::<f32>().unwrap_or(2.0);
        (level, None, resolved_target)
    } else {
        // No explicit target - check if "level" looks like a target (has decimal point)
        if args.level.contains('.') {
            // Legacy: -l 2.5 means liouvillian + target 2.5
            if let Ok(target_val) = args.level.parse::<f64>() {
                (2.0, Some(true), Some(target_val))
            } else {
                // Parse error, let it fail later with proper error
                let level = args.level.parse::<f32>().unwrap_or(2.0);
                (level, None, None)
            }
        } else {
            // It's an integer level, but no target - still an error later
            let level = args.level.parse::<f32>().unwrap_or(2.0);
            (level, None, None)
        }
    };

    // Use final_target instead of resolved_target from here on
    let resolved_target = final_target;

    // Load profile early (needed for both --eval-expression and search modes)
    let mut profile = if let Some(profile_path) = profile_arg.as_deref() {
        match Profile::from_file(profile_path) {
            Ok(profile) => profile,
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(2);
            }
        }
    } else {
        Profile::load_default()
    };

    // Apply preset if specified (before includes, so includes can override)
    if let Some(preset_name) = &args.preset {
        if let Some(preset) = presets::Preset::from_str(preset_name) {
            let preset_profile = preset.to_profile();
            profile = profile.merge(preset_profile);
        } else {
            eprintln!(
                "Error: Unknown preset '{}'. Use --list-presets to see available presets.",
                preset_name
            );
            std::process::exit(1);
        }
    }

    // Include additional profiles
    for include_path in &args.include {
        match Profile::from_file(include_path) {
            Ok(included) => profile = profile.merge(included),
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(2);
            }
        }
    }

    // Parse user constants from CLI
    for constant_spec in &args.user_constant {
        if let Err(e) = parse_user_constant_from_cli(&mut profile, constant_spec) {
            eprintln!(
                "Warning: Failed to parse user constant '{}': {}",
                constant_spec, e
            );
        }
    }

    // Parse user-defined functions from CLI
    for func_spec in &args.define {
        if let Err(e) = parse_user_function_from_cli(&mut profile, func_spec) {
            eprintln!(
                "Warning: Failed to parse user function '{}': {}",
                func_spec, e
            );
        }
    }

    // Parse CLI symbol weight overrides
    if let Some(spec) = &args.symbol_weights {
        if let Err(e) = parse_symbol_weights_from_cli(&mut profile, spec) {
            eprintln!(
                "Warning: Failed to parse --symbol-weights '{}': {}",
                spec, e
            );
        }
    }
    if let Some(spec) = &args.symbol_names {
        if let Err(e) = parse_symbol_names_from_cli(&mut profile, spec) {
            eprintln!("Warning: Failed to parse --symbol-names '{}': {}", spec, e);
        }
    }

    // Build the symbol table from the profile for per-run configuration
    let symbol_table = symbol_table::SymbolTable::from_profile(&profile);

    // Handle --eval-expression mode (evaluate and exit)
    if let Some(expr_str) = &args.find_expression {
        let x = args.at.or(resolved_target).unwrap_or(1.0);
        match eval_expression(expr_str, x, &profile.constants, &profile.functions) {
            Ok(result) => {
                println!("Expression: {}", expr_str);
                println!("At x = {}", x);
                println!("Value = {:.15}", result.value);
                println!("Derivative = {:.15}", result.derivative);
            }
            Err(e) => {
                eprintln!("Error evaluating expression: {:?}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    // Handle --eval-expression mode (evaluate and exit)
    if let Some(expr_str) = &args.eval_expression {
        let x = args.at.unwrap_or(1.0);
        match eval_expression(expr_str, x, &profile.constants, &profile.functions) {
            Ok(result) => {
                println!("Expression: {}", expr_str);
                println!("At x = {}", x);
                println!("Value = {:.15}", result.value);
                println!("Derivative = {:.15}", result.derivative);
            }
            Err(e) => {
                eprintln!("Error evaluating expression: {:?}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    // Handle --pslq mode (integer relation detection)
    if args.pslq {
        let target = match resolved_target {
            Some(t) => t,
            None => {
                eprintln!("Error: TARGET is required for PSLQ");
                std::process::exit(1);
            }
        };

        let config = pslq::PslqConfig {
            max_coefficient: args.pslq_max_coeff,
            max_iterations: 10000,
            tolerance: 1e-10,
            extended_constants: args.pslq_extended,
        };

        println!();
        println!("   PSLQ Integer Relation Detection");
        println!("   Target: {:.15}", target);
        println!("   Max coefficient: {}", config.max_coefficient);
        if config.extended_constants {
            println!("   Using extended constant set");
        }
        println!();

        // Try to find rational approximation first
        if let Some((num, den)) = pslq::find_rational_approximation(target, config.max_coefficient)
        {
            let approx = num as f64 / den as f64;
            let error = (approx - target).abs();
            println!("   Rational approximation:");
            println!(
                "   {} / {} = {:.15}  (error: {:.2e})",
                num, den, approx, error
            );
            println!();
        }

        // Try PSLQ
        match pslq::find_integer_relation(target, &config) {
            Some(relation) => {
                println!("   Integer relation found:");
                println!("   {}", relation.format());
                println!("   Residual: {:.2e}", relation.residual);
                if relation.is_exact {
                    println!("   (exact match)");
                }
            }
            None => {
                println!("   No integer relation found within coefficient bounds.");
                println!("   Try increasing --pslq-max-coeff or using --pslq-extended.");
            }
        }
        println!();
        return;
    }

    // Target is required when not using --eval-expression
    let target = match resolved_target {
        Some(t) => t,
        None => {
            eprintln!("Error: TARGET is required unless using --eval-expression");
            std::process::exit(1);
        }
    };

    // Validate that target is finite
    if !target.is_finite() {
        eprintln!("Error: TARGET must be a finite number (got {})", target);
        std::process::exit(1);
    }

    // Print header
    println!();
    println!(
        "   Your target value: T = {:<20}  ries-rs v0.1.0",
        format_value(target)
    );
    println!();

    // Convert level to complexity limits
    let base_lhs: f32 = 10.0;
    let base_rhs: f32 = 12.0;
    let level_factor = 4.0 * level_value;
    let max_lhs_complexity = (base_lhs + level_factor) as u32;
    let max_rhs_complexity = (base_rhs + level_factor) as u32;

    // Handle -i/-ie/-r/-re flags
    // --ie = integer exact mode (stops at first exact match)
    // --re = rational exact mode (stops at first exact match)
    let (integer_mode, rational_mode, exact_mode) = if args.integer_exact {
        (true, false, true)
    } else if args.rational_exact {
        (false, true, true)
    } else if args.integer {
        if target.fract() != 0.0 {
            if !args.no_slow_messages {
                eprintln!("ries: Replacing -i with -r because target isn't an integer.");
            }
            (false, true, false) // Fallback to rational mode
        } else {
            (true, false, false)
        }
    } else {
        (args.integer, args.rational, false)
    };

    // Determine numeric type restriction
    // Check liouvillian_override first (from -l legacy semantics)
    let mut min_type = if integer_mode {
        symbol::NumType::Integer
    } else if rational_mode {
        symbol::NumType::Rational
    } else if args.constructible {
        symbol::NumType::Constructible
    } else if args.algebraic {
        symbol::NumType::Algebraic
    } else if args.liouvillian || liouvillian_override.unwrap_or(false) {
        symbol::NumType::Liouvillian
    } else {
        symbol::NumType::Transcendental
    };
    if args.any_subexpressions {
        min_type = symbol::NumType::Transcendental;
    }

    // Build generation config with CLI options
    let mut gen_config = match build_gen_config(
        max_lhs_complexity,
        max_rhs_complexity,
        min_type,
        args.exclude.as_deref(),
        enable_arg.as_deref(),
        args.only_symbols.as_deref(),
        args.exclude_rhs.as_deref(),
        args.enable_rhs.as_deref(),
        args.only_symbols_rhs.as_deref(),
        args.op_limits.as_deref(),
        args.op_limits_rhs.as_deref(),
        profile.constants.clone(),
        profile.functions.clone(),
        diagnostics.show_pruned_arith,
    ) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(2);
        }
    };

    // Set the symbol table from the profile (for per-run weights and names)
    gen_config.symbol_table = std::sync::Arc::new(symbol_table);

    // Determine pool size based on mode
    let use_report = args.report && !args.classic;
    let effective_max_matches = if use_report {
        args.max_matches.max(args.top_k * 10)
    } else {
        args.max_matches
    };
    let pool_size = if use_report {
        effective_max_matches * 10
    } else {
        effective_max_matches
    };

    // Classic mode = "sniper mode": stop early like original RIES
    // Also stop at exact for --ie/--re exact modes
    let stop_at_exact = args.classic || exact_mode || args.stop_at_exact;

    let stop_below = if args.classic && args.stop_below.is_none() {
        Some(1e-10_f64.max(target.abs() * 1e-12))
    } else {
        args.stop_below
    };

    let (allowed_effective, excluded_effective) = parse_symbol_sets(
        args.only_symbols.as_deref(),
        args.exclude.as_deref(),
        enable_arg.as_deref(),
    );
    let (rhs_allowed_symbols, rhs_excluded_symbols) = parse_symbol_sets(
        args.only_symbols_rhs.as_deref(),
        args.exclude_rhs.as_deref(),
        args.enable_rhs.as_deref(),
    );

    let ranking_mode = if args.complexity_ranking {
        pool::RankingMode::Complexity
    } else if args.parity_ranking || args.classic {
        // Classic mode defaults to original-style parity ordering.
        pool::RankingMode::Parity
    } else {
        pool::RankingMode::Complexity
    };

    let mut search_config = search::SearchConfig {
        target,
        max_matches: pool_size,
        max_error: args
            .max_match_distance
            .unwrap_or((target.abs() * 0.01).max(1e-12)),
        stop_at_exact,
        stop_below,
        zero_value_threshold: args
            .zero_threshold
            .unwrap_or(search::SearchConfig::default().zero_value_threshold),
        newton_iterations: args.newton_iterations,
        user_constants: gen_config.user_constants.clone(),
        user_functions: gen_config.user_functions.clone(),
        refine_with_newton: !args.no_refinement,
        rhs_allowed_symbols,
        rhs_excluded_symbols,
        show_newton: diagnostics.show_newton,
        show_match_checks: diagnostics.show_match_checks,
        show_pruned_arith: diagnostics.show_pruned_arith,
        show_pruned_range: diagnostics.show_pruned_range,
        show_db_adds: diagnostics.show_db_adds,
        match_all_digits: args.match_all_digits,
        derivative_margin: args
            .derivative_margin
            .or(args.significance_loss_margin)
            .unwrap_or(thresholds::DEGENERATE_DERIVATIVE),
        ranking_mode,
    };

    // When --match-all-digits is enabled, set tolerance based on target's significant digits
    if args.match_all_digits && args.max_match_distance.is_none() {
        search_config.max_error = compute_significant_digits_tolerance(target);
    }

    if args.one_sided {
        // One-sided mode ranks direct x = RHS matches, so keep only display count.
        search_config.max_matches = effective_max_matches;
    }

    let explicit_streaming = args.streaming;
    let mut use_streaming = args.streaming;
    let parsed_max_memory = args.max_memory.as_deref().and_then(parse_memory_size_bytes);
    let parsed_min_memory = args.min_memory.as_deref().and_then(parse_memory_size_bytes);

    // Only apply memory-based heuristics if streaming wasn't explicitly requested
    if !explicit_streaming {
        if let Some(max_bytes) = parsed_max_memory {
            if max_bytes <= 512 * 1024 * 1024 {
                use_streaming = true;
            }
        }
        // Check memory abort threshold for auto-switching to streaming
        if let (Some(max_bytes), Some(threshold)) = (parsed_max_memory, args.memory_abort_threshold)
        {
            if (0.0..=1.0).contains(&threshold) {
                let budget = (max_bytes as f64 * threshold) as u64;
                let estimate = (pool_size as u64).saturating_mul(4096).saturating_add(
                    (max_lhs_complexity as u64 + max_rhs_complexity as u64)
                        .saturating_mul(1_000_000),
                );
                if estimate > budget {
                    use_streaming = true;
                }
            }
        }
    }

    // --min-memory can disable auto-streaming, but not explicit --streaming
    if use_streaming && !explicit_streaming {
        if let Some(min_bytes) = parsed_min_memory {
            if min_bytes >= 2 * 1024 * 1024 * 1024 {
                use_streaming = false;
            }
        }
    }

    let start = Instant::now();

    // Build symbol filters for fast path
    let mut excluded_symbols: std::collections::HashSet<u8> =
        excluded_effective.unwrap_or_default();
    if let Some(rhs_excluded) = &search_config.rhs_excluded_symbols {
        excluded_symbols.extend(rhs_excluded.iter().copied());
    }

    let fast_allowed_storage: Option<std::collections::HashSet<u8>> = match (
        allowed_effective.as_ref(),
        search_config.rhs_allowed_symbols.as_ref(),
    ) {
        (Some(all_set), Some(rhs_set)) => Some(all_set.intersection(rhs_set).copied().collect()),
        (Some(all_set), None) => Some(all_set.clone()),
        (None, Some(rhs_set)) => Some(rhs_set.clone()),
        (None, None) => None,
    };

    // Build fast match config
    let fast_config = fast_match::FastMatchConfig {
        excluded_symbols: &excluded_symbols,
        allowed_symbols: fast_allowed_storage.as_ref(),
        min_num_type: min_type,
    };

    // Fast path: check for simple exact matches before expensive generation
    // This handles cases like pi, e, sqrt(2), phi, integers, etc. instantly
    let (matches, stats) = if stop_at_exact || args.classic {
        // Only use fast path when we're looking for quick results
        if let Some(fast_match) =
            fast_match::find_fast_match(target, &profile.constants, &fast_config, &gen_config.symbol_table)
        {
            use search::SearchStats;
            let fast_stats = SearchStats {
                lhs_count: 1,
                rhs_count: 1,
                search_time: std::time::Duration::from_micros(1),
                ..Default::default()
            };
            (vec![fast_match], fast_stats)
        } else {
            // No fast match found, do full search
            // Deterministic mode disables parallelism for reproducible results
            let use_parallel = !args.deterministic && args.parallel;
            let result = run_search(
                &gen_config,
                &search_config,
                use_streaming,
                use_parallel,
                args.one_sided,
            );
            (result.matches, result.stats)
        }
    } else {
        // Not in quick mode, always do full search
        // Deterministic mode disables parallelism for reproducible results
        let use_parallel = !args.deterministic && args.parallel;
        let result = run_search(
            &gen_config,
            &search_config,
            use_streaming,
            use_parallel,
            args.one_sided,
        );
        (result.matches, result.stats)
    };

    let mut matches = matches;

    // Deterministic mode: apply stable sorting to ensure reproducible order
    // This handles any remaining non-determinism from pool ordering
    if args.deterministic {
        matches.sort_by(|a, b| pool::compare_matches(a, b, ranking_mode));
    }

    // Stability check: run multiple passes with different tolerances
    let stability_results = if args.stability_check {
        let config = if args.stability_thorough {
            stability::StabilityConfig::thorough()
        } else {
            stability::StabilityConfig::default()
        };
        let tolerance_factors = config.tolerance_factors.clone();
        let mut analyzer = stability::StabilityAnalyzer::new(config);

        // Add the base matches
        analyzer.add_level(matches.clone());

        // Run additional levels with tighter tolerances
        let base_error = search_config.max_error;
        let use_parallel = !args.deterministic && args.parallel;

        for factor in tolerance_factors.into_iter().skip(1) {
            let mut tighter_config = search_config.clone();
            tighter_config.max_error = base_error * factor;

            let result = run_search(
                &gen_config,
                &tighter_config,
                use_streaming,
                use_parallel,
                args.one_sided,
            );
            analyzer.add_level(result.matches);
        }

        Some(analyzer.analyze())
    } else {
        None
    };

    if args.min_equate_value.is_some() || args.max_equate_value.is_some() {
        matches.retain(|m| match_in_equate_bounds(m, args.min_equate_value, args.max_equate_value));
    }
    if let Some(min_match_distance) = args.min_match_distance {
        matches.retain(|m| m.error.abs() >= min_match_distance);
    }
    let mut user_constant_types = [symbol::NumType::Transcendental; 16];
    for (idx, uc) in profile.constants.iter().take(16).enumerate() {
        user_constant_types[idx] = uc.num_type;
    }
    let mut user_function_types = [symbol::NumType::Transcendental; 16];
    for (idx, uf) in profile.functions.iter().take(16).enumerate() {
        user_function_types[idx] = uf.num_type;
    }

    let expression_constraints = ExpressionConstraintOptions {
        rational_exponents: args.rational_exponents && !args.any_exponents,
        rational_trig_args: args.rational_trig_args && !args.any_trig_args,
        max_trig_cycles: args.max_trig_cycles,
        user_constant_types,
        user_function_types,
    };
    if expression_constraints.rational_exponents
        || expression_constraints.rational_trig_args
        || expression_constraints.max_trig_cycles.is_some()
    {
        matches.retain(|m| {
            expression_respects_constraints(&m.lhs.expr, expression_constraints)
                && expression_respects_constraints(&m.rhs.expr, expression_constraints)
        });
    }
    if args.numeric_anagram {
        matches.retain(match_is_numeric_anagram);
    }
    let canon_enabled = (args.canon_simplify
        || canon_reduction_enabled(args.canon_reduction.as_deref()))
        && !args.no_canon_simplify;
    if canon_enabled {
        let mut seen = std::collections::HashSet::<(String, String)>::new();
        matches.retain(|m| {
            let lhs_key =
                canonical_expression_key(&m.lhs.expr).unwrap_or_else(|| m.lhs.expr.to_postfix());
            let rhs_key =
                canonical_expression_key(&m.rhs.expr).unwrap_or_else(|| m.rhs.expr.to_postfix());
            seen.insert((lhs_key, rhs_key))
        });
    }

    let elapsed = start.elapsed();

    // Print verbose header if requested
    if args.verbose {
        print_header(target, level_value as i32);
    }

    // Print expression counts (always shown)
    println!(
        "Generated {} LHS and {} RHS expressions",
        stats.lhs_count, stats.rhs_count
    );

    // Display matches
    // Parse the output format once for both classic and report modes
    let output_format = parse_display_format(&args.format);

    // Capture matches for manifest before Report::generate consumes them
    let manifest_matches: Vec<search::Match> = if args.emit_manifest.is_some() {
        matches.clone()
    } else {
        Vec::new()
    };

    if matches.is_empty() {
        println!("   No matches found.");
    } else if !use_report {
        // Classic mode: single list sorted by complexity
        let shown: Vec<&search::Match> = matches.iter().take(effective_max_matches).collect();
        for m in shown.iter().copied() {
            let show_solve = args.solve && !args.no_solve;
            if args.absolute {
                print_match_absolute(m, show_solve, output_format, args.explicit_multiply, None, Some(&gen_config.symbol_table));
            } else {
                print_match_relative(m, show_solve, output_format, args.explicit_multiply, None, Some(&gen_config.symbol_table));
            }
        }

        if diagnostics.show_work {
            print_show_work_details(
                &shown,
                output_format,
                args.explicit_multiply,
                &profile.constants,
                &profile.functions,
                Some(&gen_config.symbol_table),
            );
        }

        // Print footer
        println!();
        if matches.len() >= effective_max_matches {
            let next_level = (level_value + 1.0) as i32;
            println!(
                "                  (for more results, use the option '-l{}')",
                next_level
            );
        }
    } else {
        // Report mode: categorized output
        if diagnostics.show_work {
            eprintln!("Warning: --show-work/-Ds is currently only available with --report false.");
        }
        let mut report_config = ReportConfig::default()
            .with_top_k(args.top_k)
            .with_target(target);

        if args.no_stable {
            report_config = report_config.without_stable();
        }

        // Convert main.rs DisplayFormat to report::DisplayFormat
        let report_format = match output_format {
            DisplayFormat::Infix(fmt) => report::DisplayFormat::Infix(fmt),
            DisplayFormat::PostfixCompact => report::DisplayFormat::PostfixCompact,
            DisplayFormat::PostfixVerbose => report::DisplayFormat::PostfixVerbose,
            DisplayFormat::Condensed => report::DisplayFormat::Condensed,
        };

        let report = Report::generate(matches, target, &report_config);
        report.print(args.absolute, args.solve && !args.no_solve, report_format);
    }

    // Print footer - verbose or standard
    if args.verbose {
        print_footer(&stats, elapsed);
    } else {
        println!();
        println!("  Search completed in {:.3}s", elapsed.as_secs_f64());
    }

    // Print detailed stats if requested
    if diagnostics.show_stats {
        stats.print();
    }

    // Print stability analysis if requested
    if let Some(ref results) = stability_results {
        println!();
        println!("  === Stability Analysis ===");
        print!(
            "{}",
            stability::format_stability_report(results, effective_max_matches)
        );
    }

    // High-precision verification if requested
    if let Some(precision_bits) = args.precision {
        println!();
        println!(
            "  === High-Precision Verification ({} bits) ===",
            precision_bits
        );
        let hp_results = highprec_verify::verify_matches_highprec(
            manifest_matches.clone(),
            target,
            precision_bits,
            &profile.constants,
        );
        print!(
            "{}",
            highprec_verify::format_verification_report(&hp_results, effective_max_matches)
        );
    }

    // Emit manifest if requested
    if let Some(manifest_path) = &args.emit_manifest {
        let manifest = build_manifest(
            target,
            level_value,
            max_lhs_complexity,
            max_rhs_complexity,
            args.deterministic,
            args.parallel,
            search_config.max_error,
            effective_max_matches,
            ranking_mode,
            &profile.constants,
            &args.exclude,
            &args.only_symbols,
            &manifest_matches,
        );

        match manifest.to_json() {
            Ok(json) => {
                if let Err(e) = std::fs::write(manifest_path, json) {
                    eprintln!("Error writing manifest: {}", e);
                } else if !args.no_slow_messages {
                    eprintln!("Manifest written to {}", manifest_path.display());
                }
            }
            Err(e) => {
                eprintln!("Error serializing manifest: {}", e);
            }
        }
    }
}

/// Build a manifest from the search results
#[allow(clippy::too_many_arguments)]
fn build_manifest(
    target: f64,
    level: f32,
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    deterministic: bool,
    parallel: bool,
    max_error: f64,
    max_matches: usize,
    ranking_mode: pool::RankingMode,
    user_constants: &[profile::UserConstant],
    excluded_symbols: &Option<String>,
    allowed_symbols: &Option<String>,
    matches: &[search::Match],
) -> RunManifest {
    let config = SearchConfigInfo {
        target,
        level,
        max_lhs_complexity,
        max_rhs_complexity,
        deterministic,
        parallel: !deterministic && parallel,
        max_error,
        max_matches,
        ranking_mode: match ranking_mode {
            pool::RankingMode::Complexity => "complexity".to_string(),
            pool::RankingMode::Parity => "parity".to_string(),
        },
        user_constants: user_constants
            .iter()
            .map(|uc| manifest::UserConstantInfo {
                name: uc.name.clone(),
                value: uc.value,
                description: uc.description.clone(),
            })
            .collect(),
        excluded_symbols: excluded_symbols
            .as_ref()
            .map(|s| s.chars().map(|c| c.to_string()).collect())
            .unwrap_or_default(),
        allowed_symbols: allowed_symbols
            .as_ref()
            .map(|s| s.chars().map(|c| c.to_string()).collect()),
    };

    let results: Vec<MatchInfo> = matches
        .iter()
        .take(max_matches)
        .map(|m| {
            let stability = crate::metrics::MatchMetrics::from_match(m, None).stability;
            MatchInfo {
                lhs_postfix: m.lhs.expr.to_postfix(),
                rhs_postfix: m.rhs.expr.to_postfix(),
                lhs_infix: m.lhs.expr.to_infix(),
                rhs_infix: m.rhs.expr.to_infix(),
                error: m.error.abs(),
                is_exact: m.error.abs() < thresholds::EXACT_MATCH_TOLERANCE,
                complexity: m.complexity,
                x_value: m.x_value,
                stability: Some(stability),
            }
        })
        .collect();

    RunManifest::new(config, results)
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
    fn test_solve_for_x_linear_add() {
        let lhs = expr::Expression::parse("x1+").unwrap();
        let rhs = expr::Expression::parse("3").unwrap();
        let solved = solve_for_x_rhs_expression(&lhs, &rhs).expect("solvable linear add");
        assert_eq!(solved.to_postfix(), "31-");
    }

    #[test]
    fn test_solve_for_x_linear_mul() {
        let lhs = expr::Expression::parse("2x*").unwrap();
        let rhs = expr::Expression::parse("5").unwrap();
        let solved = solve_for_x_rhs_expression(&lhs, &rhs).expect("solvable linear multiply");
        assert_eq!(solved.to_postfix(), "52/");
    }

    #[test]
    fn test_solve_for_x_unary_inverse() {
        let lhs = expr::Expression::parse("xq").unwrap(); // sqrt(x)
        let rhs = expr::Expression::parse("2").unwrap();
        let solved = solve_for_x_rhs_expression(&lhs, &rhs).expect("solvable unary inverse");
        assert_eq!(solved.to_postfix(), "2s");
    }

    #[test]
    fn test_solve_for_x_tan_inverse_supported() {
        let lhs = expr::Expression::parse("xT").unwrap(); // tanpi(x)
        let rhs = expr::Expression::parse("2").unwrap();
        let solved =
            solve_for_x_rhs_expression(&lhs, &rhs).expect("tan inverse should be supported");
        let postfix = solved.to_postfix();
        assert!(postfix.contains('A') && postfix.contains('p') && postfix.contains('/'));
    }

    #[test]
    fn test_solve_for_x_lambert_inverse_supported() {
        let lhs = expr::Expression::parse("xW").unwrap(); // W(x)
        let rhs = expr::Expression::parse("2").unwrap();
        let solved =
            solve_for_x_rhs_expression(&lhs, &rhs).expect("Lambert W inverse should be supported");
        assert_eq!(solved.to_postfix(), "22E*");
    }

    #[test]
    fn test_solve_for_x_unsupported_falls_back() {
        let lhs = expr::Expression::parse("xH").unwrap(); // user function (unsupported inverse)
        let rhs = expr::Expression::parse("2").unwrap();
        assert!(
            solve_for_x_rhs_expression(&lhs, &rhs).is_none(),
            "unsupported inverses should fall back to equation form"
        );
    }
}

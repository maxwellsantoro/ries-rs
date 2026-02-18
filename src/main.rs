//! RIES-RS: Find algebraic equations given their solution
//!
//! A Rust implementation of Robert Munafo's RIES program.

// Allow field reassignment with default in test code - common pattern for config building
#![cfg_attr(test, allow(clippy::field_reassign_with_default))]

mod eval;
mod expr;
mod fast_match;
mod gen;
mod metrics;
mod pool;
#[cfg(feature = "highprec")]
mod precision;
mod profile;
mod report;
mod search;
mod symbol;
mod thresholds;
mod udf;

use clap::{ArgAction, Parser};
use profile::Profile;
use report::{Report, ReportConfig};
use std::path::PathBuf;
use std::time::Instant;
use thresholds::EXACT_MATCH_TOLERANCE;

/// Find algebraic equations given their solution
#[derive(Parser, Debug)]
#[command(name = "ries-rs")]
#[command(author = "RIES Contributors")]
#[command(version = "0.1.0")]
#[command(about = "Find algebraic equations given their solution", long_about = None)]
struct Args {
    /// Target value to find equations for (optional if using --eval-expression)
    target: Option<f64>,

    /// Search level (each increment ≈ 10x more equations)
    /// Level 0 ≈ 89M equations, Level 2 ≈ 11B, Level 5 ≈ 15T
    /// Legacy: "-l 2.5" (with no explicit target) means Liouvillian mode + target 2.5
    #[arg(short = 'l', long, default_value = "2")]
    level: String,

    /// Maximum number of matches to display
    #[arg(short = 'n', long = "max-matches", default_value = "16")]
    max_matches: usize,

    /// Show absolute x values instead of T ± error
    #[arg(short = 'x', long, alias = "absolute-roots")]
    absolute: bool,

    /// Try to solve for x (show x = ... form)
    #[arg(short = 's', long, alias = "try-solve-for-x")]
    solve: bool,

    /// Disable solve-for-x presentation even if requested elsewhere
    #[arg(long = "no-solve-for-x")]
    no_solve: bool,

    /// Symbols to never use (e.g., "+-" to exclude add/subtract)
    #[arg(short = 'N', long)]
    exclude: Option<String>,

    /// Re-enable symbols disabled by exclude/only options, or enable all if no argument
    #[arg(short = 'E', long = "enable", num_args = 0..=1, default_missing_value = "all")]
    enable: Option<String>,

    /// Only use these symbols, or print symbol table if no argument
    /// Using -S alone prints the full symbol table and exits
    #[arg(short = 'S', long, num_args = 0..=1)]
    only_symbols: Option<String>,

    /// Operator/symbol count limits (C RIES -O semantics).
    /// Example: "-Ox" means at most one x per side; "-O2+" means at most two '+'.
    #[arg(short = 'O', long)]
    op_limits: Option<String>,

    /// Only use these symbols on RHS (right-hand side)
    #[arg(long = "S-RHS")]
    only_symbols_rhs: Option<String>,

    /// Exclude these symbols on RHS
    #[arg(long = "N-RHS")]
    exclude_rhs: Option<String>,

    /// Re-enable symbols on RHS
    #[arg(long = "E-RHS")]
    enable_rhs: Option<String>,

    /// RHS-only symbol count limits (like -O but RHS only)
    #[arg(long = "O-RHS")]
    op_limits_rhs: Option<String>,

    /// Custom symbol weights (e.g., ":W:20" sets Lambert W weight to 20)
    #[arg(long)]
    symbol_weights: Option<String>,

    /// Custom symbol names (e.g., ":p:PI" renames pi to PI in output)
    #[arg(long)]
    symbol_names: Option<String>,

    /// Restrict to algebraic solutions
    #[arg(short = 'a', long, alias = "algebraic-subexpressions")]
    algebraic: bool,

    /// Restrict to constructible solutions
    #[arg(short = 'c', long, alias = "constructible-subexpressions")]
    constructible: bool,

    /// Restrict to rational solutions
    #[arg(short = 'r', long, alias = "rational-subexpressions")]
    rational: bool,

    /// Restrict to integer solutions
    #[arg(short = 'i', long, alias = "integer-subexpressions")]
    integer: bool,

    /// Integer exact mode (equivalent to -i --stop-at-exact)
    /// Stops at first integer match
    #[arg(long = "ie")]
    integer_exact: bool,

    /// Rational exact mode (equivalent to -r --stop-at-exact)
    /// Stops at first rational match
    #[arg(long = "re")]
    rational_exact: bool,

    /// Restrict to Liouvillian subexpressions
    #[arg(long = "liouvillian-subexpressions")]
    liouvillian: bool,

    /// Use parallel search (default: true)
    #[arg(long, default_value = "true")]
    parallel: bool,

    /// Use streaming search for lower memory usage at high complexity levels
    /// Streaming processes expressions on-the-fly instead of accumulating in memory
    #[arg(long)]
    streaming: bool,

    /// Use report mode with categorized output (default: true)
    /// Shows top matches in each category: exact, best, elegant, interesting, stable
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    report: bool,

    /// Classic/sniper mode: single list sorted by complexity (like original RIES)
    /// Implies --stop-at-exact and aggressive early exit for speed
    #[arg(long)]
    classic: bool,

    /// Output format for expressions
    /// Options: default, pretty (Unicode), mathematica, sympy
    #[arg(short = 'F', long, num_args = 0..=1, default_missing_value = "3", default_value = "2")]
    format: String,

    /// Compatibility diagnostics flag (-D, -Dy, etc.)
    #[arg(short = 'D', num_args = 0..=1, default_missing_value = "")]
    diagnostics: Option<String>,

    /// Number of matches per category in report mode
    #[arg(short = 'k', long = "top-k", default_value = "8")]
    top_k: usize,

    /// Exclude stability category from the report
    #[arg(long)]
    no_stable: bool,

    /// Show detailed search statistics
    #[arg(long)]
    stats: bool,

    /// Print list of supported options and exit
    #[arg(long = "list-options")]
    list_options: bool,

    /// Stop search when an exact match is found
    #[arg(long)]
    stop_at_exact: bool,

    /// Stop search when error goes below this threshold
    #[arg(long)]
    stop_below: Option<f64>,

    /// Load profile file for custom constants and symbol settings
    #[arg(short = 'p', long)]
    profile: Option<PathBuf>,

    /// Include additional profile file (can be used multiple times)
    #[arg(long)]
    include: Vec<PathBuf>,

    /// Add a user-defined constant
    /// Format: "weight:name:description:value"
    /// Example: -X "4:gamma:Euler's constant:0.5772156649"
    #[arg(short = 'X', long = "user-constant", alias = "constant")]
    user_constant: Vec<String>,

    /// Define a custom operation/function
    /// Format: "weight:name:description:formula"
    /// Formula uses postfix notation with | for dup and @ for swap
    /// Example: --define "4:sinh:hyperbolic sine:E|r-2/"
    #[arg(long)]
    define: Vec<String>,

    /// Maximum acceptable error for matches (default: 1% of |target|)
    #[arg(long, alias = "mad")]
    max_match_distance: Option<f64>,

    /// Minimum error threshold (exclude matches closer than this)
    #[arg(long)]
    min_match_distance: Option<f64>,

    /// Use one-sided mode: only generate RHS expressions, compare directly to target
    #[arg(long)]
    one_sided: bool,

    /// Skip Newton-Raphson refinement of matches
    #[arg(long)]
    no_refinement: bool,

    /// Evaluate an expression at a given x value and exit
    /// Example: --eval-expression "xq" --at 2.5
    #[arg(long)]
    eval_expression: Option<String>,

    /// Evaluate and display a specific expression (compatibility option)
    #[arg(long)]
    find_expression: Option<String>,

    /// X value for --eval-expression
    #[arg(long)]
    at: Option<f64>,

    /// Maximum Newton-Raphson iterations for root refinement (default: 15)
    #[arg(long, default_value = "15")]
    newton_iterations: usize,

    /// Precision in bits for high-precision mode (e.g., 256 for ~77 digits)
    /// Note: High-precision mode is not yet implemented; this flag is reserved
    #[arg(long)]
    precision: Option<u32>,

    /// Threshold for pruning LHS expressions with near-zero values (default: 1e-4)
    #[arg(long)]
    zero_threshold: Option<f64>,

    /// Compatibility no-op for wide output formatting
    #[arg(long)]
    wide: bool,

    /// Compatibility no-op for wide output formatting
    #[arg(long = "wide-output")]
    wide_output: bool,

    /// Compatibility option: prefer relative roots display
    #[arg(long = "relative-roots")]
    relative_roots: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "any-exponents")]
    any_exponents: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "any-subexpressions")]
    any_subexpressions: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "any-trig-args")]
    any_trig_args: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "canon-reduction")]
    canon_reduction: Option<String>,

    /// Compatibility option (currently no-op)
    #[arg(long = "canon-simplify")]
    canon_simplify: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "derivative-margin")]
    derivative_margin: Option<f64>,

    /// Compatibility option (currently no-op)
    #[arg(long = "explicit-multiply")]
    explicit_multiply: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "match-all-digits")]
    match_all_digits: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "max-equate-value")]
    max_equate_value: Option<f64>,

    /// Compatibility option (currently no-op)
    #[arg(long = "max-memory")]
    max_memory: Option<String>,

    /// Compatibility option (currently no-op)
    #[arg(long = "memory-abort-threshold")]
    memory_abort_threshold: Option<f64>,

    /// Compatibility option (currently no-op)
    #[arg(long = "max-trig-cycles")]
    max_trig_cycles: Option<u32>,

    /// Compatibility option (currently no-op)
    #[arg(long = "min-equate-value")]
    min_equate_value: Option<f64>,

    /// Compatibility option (currently no-op)
    #[arg(long = "min-memory")]
    min_memory: Option<String>,

    /// Compatibility option (currently no-op)
    #[arg(long = "no-canon-simplify")]
    no_canon_simplify: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "no-slow-messages")]
    no_slow_messages: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "numeric-anagram")]
    numeric_anagram: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "rational-exponents")]
    rational_exponents: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "rational-trig-args")]
    rational_trig_args: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "show-work")]
    show_work: bool,

    /// Compatibility option (currently no-op)
    #[arg(long = "significance-loss-margin")]
    significance_loss_margin: Option<f64>,

    /// Compatibility option (currently no-op)
    #[arg(long = "trig-argument-scale")]
    trig_argument_scale: Option<f64>,

    /// Show verbose output with header and footer details
    #[arg(long)]
    verbose: bool,
}

/// Parse a user constant from CLI argument
/// Format: "weight:name:description:value"
fn parse_user_constant_from_cli(profile: &mut Profile, spec: &str) -> Result<(), String> {
    use profile::UserConstant;
    use symbol::NumType;

    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() != 4 {
        return Err(format!(
            "Expected 4 colon-separated parts, got {}",
            parts.len()
        ));
    }

    let weight: u32 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid weight: {}", parts[0]))?;

    let name = parts[1].to_string();
    if name.is_empty() {
        return Err("Constant name cannot be empty".to_string());
    }

    let description = parts[2].to_string();

    let value: f64 = parts[3]
        .parse()
        .map_err(|_| format!("Invalid value: {}", parts[3]))?;

    // Validate that the value is finite (not NaN or infinity)
    if !value.is_finite() {
        return Err(format!("Constant value must be finite (got {})", value));
    }

    // Determine numeric type based on value characteristics
    let num_type = if value.fract() == 0.0 && value.abs() < 1e10 {
        NumType::Integer
    } else if is_rational(value) {
        NumType::Rational
    } else {
        NumType::Transcendental
    };

    profile.constants.push(UserConstant {
        weight,
        name,
        description,
        value,
        num_type,
    });

    Ok(())
}

/// Check if a value is likely rational (simple fraction)
fn is_rational(v: f64) -> bool {
    if !v.is_finite() || v == 0.0 {
        return true;
    }

    for denom in 1..=100_u32 {
        let numer = v * denom as f64;
        if (numer.round() - numer).abs() < 1e-10 {
            return true;
        }
    }
    false
}

/// Parse a user-defined function from CLI argument
/// Format: "weight:name:description:formula"
fn parse_user_function_from_cli(profile: &mut Profile, spec: &str) -> Result<(), String> {
    let udf = udf::UserFunction::parse(spec)?;
    profile.functions.push(udf);
    Ok(())
}

/// Parse symbol names from CLI argument
/// Format: ":p:PI :e:EULER"
fn parse_symbol_names_from_cli(profile: &mut Profile, spec: &str) -> Result<(), String> {
    for part in spec.split_whitespace() {
        if !part.starts_with(':') {
            continue;
        }

        let inner = &part[1..];
        let Some(colon_pos) = inner.find(':') else {
            return Err(format!("Invalid symbol name format: {}", part));
        };

        let symbol_char = inner[..colon_pos]
            .chars()
            .next()
            .ok_or_else(|| "Empty symbol in --symbol-names".to_string())?;
        let display_name = inner[colon_pos + 1..].to_string();
        if display_name.is_empty() {
            return Err(format!(
                "Empty replacement name in --symbol-names: {}",
                part
            ));
        }

        let Some(symbol) = symbol::Symbol::from_byte(symbol_char as u8) else {
            return Err(format!(
                "Unknown symbol in --symbol-names: {}",
                symbol_char
            ));
        };

        profile.symbol_names.insert(symbol, display_name);
    }

    Ok(())
}

/// Parse symbol weights from CLI argument
/// Format: ":W:20 :p:25"
fn parse_symbol_weights_from_cli(profile: &mut Profile, spec: &str) -> Result<(), String> {
    for part in spec.split_whitespace() {
        if !part.starts_with(':') {
            continue;
        }

        let inner = &part[1..];
        let Some(colon_pos) = inner.find(':') else {
            return Err(format!("Invalid symbol weight format: {}", part));
        };

        let symbol_char = inner[..colon_pos]
            .chars()
            .next()
            .ok_or_else(|| "Empty symbol in --symbol-weights".to_string())?;
        let weight: u32 = inner[colon_pos + 1..]
            .parse()
            .map_err(|_| format!("Invalid weight in --symbol-weights: {}", part))?;

        let Some(symbol) = symbol::Symbol::from_byte(symbol_char as u8) else {
            return Err(format!(
                "Unknown symbol in --symbol-weights: {}",
                symbol_char
            ));
        };

        profile.symbol_weights.insert(symbol, weight);
    }

    Ok(())
}

/// Parse -O/--op-limits into per-symbol maximum counts.
fn parse_symbol_count_limits(spec: &str) -> Result<std::collections::HashMap<symbol::Symbol, u32>, String> {
    let mut limits = std::collections::HashMap::new();
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }

        let mut saw_digits = false;
        let mut count: u32 = 0;
        while i < chars.len() && chars[i].is_ascii_digit() {
            saw_digits = true;
            count = count
                .saturating_mul(10)
                .saturating_add((chars[i] as u8 - b'0') as u32);
            i += 1;
        }

        if i >= chars.len() {
            return Err("Trailing count with no symbol in -O/--op-limits".to_string());
        }

        let symbol_char = chars[i];
        i += 1;
        if symbol_char.is_whitespace() {
            continue;
        }

        let max_count = if saw_digits { count } else { 1 };
        if max_count == 0 {
            return Err(format!("Invalid zero count for symbol '{}'", symbol_char));
        }

        let Some(sym) = symbol::Symbol::from_byte(symbol_char as u8) else {
            return Err(format!("Unknown symbol '{}' in -O/--op-limits", symbol_char));
        };
        limits.insert(sym, max_count);
    }

    Ok(limits)
}

/// Print the symbol table (for -S without argument)
fn print_symbol_table() {
    println!("Explicit values:");
    println!(" sym seft wght name description");
    for sym in symbol::Symbol::constants() {
        let byte = *sym as u8;
        if byte < 128 {
            // Skip user constants
            println!(
                "  {:<2}   {:<1}   {:<3} {:<6} {}",
                byte as char,
                match sym.seft() {
                    symbol::Seft::A => "a",
                    symbol::Seft::B => "b",
                    symbol::Seft::C => "c",
                },
                sym.weight(),
                sym.name(),
                sym_description(*sym)
            );
        }
    }

    println!("\nFunctions of one argument:");
    println!(" sym seft wght name description");
    for sym in symbol::Symbol::unary_ops() {
        let byte = *sym as u8;
        println!(
            "  {:<2}   {:<1}   {:<3} {:<6} {}",
            byte as char,
            match sym.seft() {
                symbol::Seft::A => "a",
                symbol::Seft::B => "b",
                symbol::Seft::C => "c",
            },
            sym.weight(),
            sym.name(),
            sym_description(*sym)
        );
    }

    println!("\nFunctions of two arguments:");
    println!(" sym seft wght name description");
    for sym in symbol::Symbol::binary_ops() {
        let byte = *sym as u8;
        println!(
            "  {:<2}   {:<1}   {:<3} {:<6} {}",
            byte as char,
            match sym.seft() {
                symbol::Seft::A => "a",
                symbol::Seft::B => "b",
                symbol::Seft::C => "c",
            },
            sym.weight(),
            sym.name(),
            sym_description(*sym)
        );
    }
}

/// Get a description for a symbol (for -S symbol table output)
fn sym_description(sym: symbol::Symbol) -> &'static str {
    use symbol::Symbol;
    match sym {
        Symbol::One => "integer",
        Symbol::Two => "integer",
        Symbol::Three => "integer",
        Symbol::Four => "integer",
        Symbol::Five => "integer",
        Symbol::Six => "integer",
        Symbol::Seven => "integer",
        Symbol::Eight => "integer",
        Symbol::Nine => "integer",
        Symbol::Pi => "pi = 3.14159...",
        Symbol::E => "e = base of natural logarithms, 2.71828...",
        Symbol::Phi => "phi = the golden ratio, (1+sqrt(5))/2",
        Symbol::Gamma => "Euler-Mascheroni constant gamma",
        Symbol::Plastic => "plastic constant",
        Symbol::Apery => "Apery's constant zeta(3)",
        Symbol::Catalan => "Catalan's constant",
        Symbol::X => "the variable of the equation",
        Symbol::Neg => "negate",
        Symbol::Recip => "reciprocal",
        Symbol::Sqrt => "sqrt(x) = square root",
        Symbol::Square => "^2 = square",
        Symbol::Ln => "ln(x) = natural logarithm or log base e",
        Symbol::Exp => "natural exponent function",
        Symbol::SinPi => "sinpi(X) = sin(pi * x)",
        Symbol::CosPi => "cospi(X) = cos(pi * x)",
        Symbol::TanPi => "tanpi(X) = tan(pi * x)",
        Symbol::LambertW => "Lambert W function",
        Symbol::Add => "add",
        Symbol::Sub => "subtract",
        Symbol::Mul => "multiply",
        Symbol::Div => "divide",
        Symbol::Pow => "power",
        Symbol::Root => "a-th root of b",
        Symbol::Log => "log base a of b",
        Symbol::Atan2 => "2-argument arctangent",
        _ => "",
    }
}

/// Parse effective allowed/excluded symbol sets with optional re-enable set.
fn parse_symbol_sets(
    only_symbols: Option<&str>,
    exclude_symbols: Option<&str>,
    enable_symbols: Option<&str>,
) -> (
    Option<std::collections::HashSet<u8>>,
    Option<std::collections::HashSet<u8>>,
) {
    let mut allowed: Option<std::collections::HashSet<u8>> =
        only_symbols.map(|s| s.bytes().collect());
    let mut excluded: Option<std::collections::HashSet<u8>> =
        exclude_symbols.map(|s| s.bytes().collect());

    if let Some(enabled) = enable_symbols {
        if enabled == "all" {
            // Special value "all" clears all exclusions
            excluded = None;
        } else {
            for b in enabled.bytes() {
                if let Some(excl) = excluded.as_mut() {
                    excl.remove(&b);
                }
                if let Some(allow) = allowed.as_mut() {
                    allow.insert(b);
                }
            }
        }
    }

    (allowed, excluded)
}

/// Build a GenConfig from CLI options
#[allow(clippy::too_many_arguments)]
#[allow(clippy::field_reassign_with_default)]
fn build_gen_config(
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    min_type: symbol::NumType,
    exclude: Option<&str>,
    enable: Option<&str>,
    only_symbols: Option<&str>,
    exclude_rhs: Option<&str>,
    enable_rhs: Option<&str>,
    only_symbols_rhs: Option<&str>,
    op_limits: Option<&str>,
    op_limits_rhs: Option<&str>,
    user_constants: Vec<profile::UserConstant>,
    user_functions: Vec<udf::UserFunction>,
    show_pruned_arith: bool,
) -> Result<gen::GenConfig, String> {
    let mut config = gen::GenConfig::default();
    config.max_lhs_complexity = max_lhs_complexity;
    config.max_rhs_complexity = max_rhs_complexity;
    config.min_num_type = min_type;
    config.user_constants = user_constants.clone();
    config.user_functions = user_functions.clone();
    config.show_pruned_arith = show_pruned_arith;

    // Helper to filter symbols
    fn filter_symbols(
        symbols: &[symbol::Symbol],
        allowed: Option<&std::collections::HashSet<u8>>,
        excluded: Option<&std::collections::HashSet<u8>>,
    ) -> Vec<symbol::Symbol> {
        let mut result: Vec<symbol::Symbol> = symbols.to_vec();

        if let Some(allow_set) = allowed {
            result.retain(|s| allow_set.contains(&(*s as u8)));
        }

        if let Some(excl_set) = excluded {
            result.retain(|s| !excl_set.contains(&(*s as u8)));
        }

        result
    }

    // Parse effective symbol sets (with -E/--enable support).
    let (allowed, excluded) = parse_symbol_sets(only_symbols, exclude, enable);
    let (allowed_rhs, excluded_rhs) =
        parse_symbol_sets(only_symbols_rhs, exclude_rhs, enable_rhs);

    // Apply LHS symbol filtering
    config.constants = filter_symbols(
        symbol::Symbol::constants(),
        allowed.as_ref(),
        excluded.as_ref(),
    );
    config.unary_ops = filter_symbols(
        symbol::Symbol::unary_ops(),
        allowed.as_ref(),
        excluded.as_ref(),
    );
    config.binary_ops = filter_symbols(
        symbol::Symbol::binary_ops(),
        allowed.as_ref(),
        excluded.as_ref(),
    );

    // Parse -O/--op-limits into per-expression max symbol counts.
    if let Some(spec) = op_limits {
        config.symbol_max_counts = parse_symbol_count_limits(spec)?;
    }
    if let Some(spec_rhs) = op_limits_rhs {
        config.rhs_symbol_max_counts = Some(parse_symbol_count_limits(spec_rhs)?);
    }

    // Add user constant symbols to the constants pool
    // Map each user constant to its corresponding symbol (UserConstant0, UserConstant1, etc.)
    for (idx, _uc) in user_constants.iter().enumerate() {
        if idx < 16 {
            if let Some(sym) = symbol::Symbol::from_byte(128 + idx as u8) {
                // Only add if not excluded
                let is_excluded = excluded
                    .as_ref()
                    .is_some_and(|excl| excl.contains(&(128 + idx as u8)));
                if !is_excluded {
                    config.constants.push(sym);
                }
            }
        }
    }

    // Add user function symbols to the unary_ops pool
    // Map each user function to its corresponding symbol (UserFunction0, UserFunction1, etc.)
    for (idx, _uf) in user_functions.iter().enumerate() {
        if idx < 16 {
            if let Some(sym) = symbol::Symbol::from_byte(144 + idx as u8) {
                // Only add if not excluded
                let is_excluded = excluded
                    .as_ref()
                    .is_some_and(|excl| excl.contains(&(144 + idx as u8)));
                if !is_excluded {
                    config.unary_ops.push(sym);
                }
            }
        }
    }

    // Build full symbol sets including user symbols for RHS overrides.
    let mut all_constants = symbol::Symbol::constants().to_vec();
    let mut all_unary = symbol::Symbol::unary_ops().to_vec();
    let all_binary = symbol::Symbol::binary_ops().to_vec();
    for idx in 0..user_constants.len().min(16) {
        if let Some(sym) = symbol::Symbol::from_byte(128 + idx as u8) {
            all_constants.push(sym);
        }
    }
    for idx in 0..user_functions.len().min(16) {
        if let Some(sym) = symbol::Symbol::from_byte(144 + idx as u8) {
            all_unary.push(sym);
        }
    }

    if allowed_rhs.is_some() || excluded_rhs.is_some() || op_limits_rhs.is_some() {
        let constants_base = if allowed_rhs.is_some() {
            all_constants
        } else {
            config.constants.clone()
        };
        let unary_base = if allowed_rhs.is_some() {
            all_unary
        } else {
            config.unary_ops.clone()
        };
        let binary_base = if allowed_rhs.is_some() {
            all_binary
        } else {
            config.binary_ops.clone()
        };

        config.rhs_constants = Some(filter_symbols(
            &constants_base,
            allowed_rhs.as_ref(),
            excluded_rhs.as_ref(),
        ));
        config.rhs_unary_ops = Some(filter_symbols(
            &unary_base,
            allowed_rhs.as_ref(),
            excluded_rhs.as_ref(),
        ));
        config.rhs_binary_ops = Some(filter_symbols(
            &binary_base,
            allowed_rhs.as_ref(),
            excluded_rhs.as_ref(),
        ));
    }

    Ok(config)
}

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

/// Perform the search (helper function to avoid code duplication)
#[allow(clippy::too_many_arguments)]
fn perform_search(
    gen_config: &gen::GenConfig,
    search_config: &search::SearchConfig,
    streaming: bool,
    parallel: bool,
    one_sided: bool,
) -> (Vec<search::Match>, search::SearchStats) {
    if one_sided {
        search::search_one_sided_with_stats_and_config(gen_config, search_config)
    } else if streaming {
        search::search_streaming_with_config(gen_config, search_config)
    } else {
        #[cfg(feature = "parallel")]
        {
            if parallel {
                search::search_parallel_with_stats_and_config(gen_config, search_config)
            } else {
                search::search_with_stats_and_config(gen_config, search_config)
            }
        }
        #[cfg(not(feature = "parallel"))]
        {
            search::search_with_stats_and_config(gen_config, search_config)
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum DisplayFormat {
    Infix(expr::OutputFormat),
    PostfixCompact,
    PostfixVerbose,
    Condensed, // -F1 alias for PostfixCompact
}

#[derive(Debug, Default)]
struct DiagnosticOptions {
    // Existing
    show_work: bool,           // s, N
    show_stats: bool,          // y, M
    // NEW channels
    show_match_checks: bool,   // o
    show_pruned_arith: bool,   // A, a
    show_pruned_range: bool,   // B, b
    show_db_adds: bool,        // G, g
    show_newton: bool,         // n
    unsupported_channels: Vec<char>,
}

fn parse_diagnostics(
    diagnostics: Option<&str>,
    show_work_flag: bool,
    show_stats_flag: bool,
) -> DiagnosticOptions {
    let mut opts = DiagnosticOptions {
        show_work: show_work_flag,
        show_stats: show_stats_flag,
        show_match_checks: false,
        show_pruned_arith: false,
        show_pruned_range: false,
        show_db_adds: false,
        show_newton: false,
        unsupported_channels: Vec::new(),
    };

    if let Some(spec) = diagnostics {
        for ch in spec.chars() {
            match ch {
                's' | 'N' => opts.show_work = true,
                'y' | 'M' => opts.show_stats = true,
                'o' => opts.show_match_checks = true,
                'A' | 'a' => opts.show_pruned_arith = true,
                'B' | 'b' => opts.show_pruned_range = true,
                'G' | 'g' => opts.show_db_adds = true,
                'n' => opts.show_newton = true,
                _ => opts.unsupported_channels.push(ch),
            }
        }
    }

    opts
}

#[inline]
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

    // Handle -S without argument (print symbol table and exit)
    // When -S is used with num_args=0..=1, bare -S gives Some("") and -S with value gives Some(value)
    // Also check if target is None to distinguish from "-S symbols target"
    // Note: clap's num_args=0..=1 with a positional arg means -S alone could also give None
    // if the positional target is consumed instead
    let is_bare_s = (args.only_symbols.as_ref().is_some_and(|s| s.is_empty()) && args.target.is_none())
        || (args.only_symbols.is_none() && args.target.is_none() && std::env::args().any(|a| a == "-S"));
    if is_bare_s {
        print_symbol_table();
        return;
    }

    let _compat_noop = (
        args.wide,
        args.wide_output,
        args.relative_roots,
        args.any_exponents,
        args.any_subexpressions,
        args.any_trig_args,
        args.canon_reduction.as_deref(),
        args.canon_simplify,
        args.derivative_margin,
        args.match_all_digits,
        args.max_memory.as_deref(),
        args.memory_abort_threshold,
        args.max_trig_cycles,
        args.min_memory.as_deref(),
        args.no_canon_simplify,
        args.no_slow_messages,
        args.numeric_anagram,
        args.rational_exponents,
        args.rational_trig_args,
        args.significance_loss_margin,
        args.trig_argument_scale,
    );
    let diagnostics =
        parse_diagnostics(args.diagnostics.as_deref(), args.show_work, args.stats);

    if !diagnostics.unsupported_channels.is_empty() {
        let unsupported: String = diagnostics.unsupported_channels.iter().collect();
        eprintln!(
            "Warning: -D channels not implemented in ries-rs yet: {}",
            unsupported
        );
    }

    // Warn about unimplemented precision flag
    if args.precision.is_some() {
        eprintln!(
            "Warning: --precision flag specified but high-precision mode is not yet implemented."
        );
        eprintln!("         Using standard f64 precision (~15 digits).");
    }

    // Handle -p legacy semantics: if profile looks like a number and no target, treat as target
    // Original ries behavior: "ries -p 2.5" means "use default profile and search for 2.5"
    let (profile_arg, resolved_target) =
        if let Some(ref profile_path) = args.profile {
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
    let (enable_arg, resolved_target) =
        if let Some(ref enable_str) = args.enable {
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
    let (level_value, liouvillian_override, final_target) =
        if resolved_target.is_some() {
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

    // Apply symbol weight overrides globally (used by complexity calculations).
    let mut weight_overrides = profile.symbol_weights.clone();
    for (idx, user_constant) in profile.constants.iter().enumerate().take(16) {
        if let Some(sym) = symbol::Symbol::from_byte(128 + idx as u8) {
            weight_overrides.insert(sym, user_constant.weight);
        }
    }
    for (idx, user_function) in profile.functions.iter().enumerate().take(16) {
        if let Some(sym) = symbol::Symbol::from_byte(144 + idx as u8) {
            weight_overrides.insert(sym, user_function.weight as u32);
        }
    }
    symbol::set_weight_overrides(weight_overrides);

    // Apply symbol display-name overrides globally.
    let mut name_overrides = profile.symbol_names.clone();
    for (idx, user_constant) in profile.constants.iter().enumerate().take(16) {
        if let Some(sym) = symbol::Symbol::from_byte(128 + idx as u8) {
            name_overrides.insert(sym, user_constant.name.clone());
        }
    }
    for (idx, user_function) in profile.functions.iter().enumerate().take(16) {
        if let Some(sym) = symbol::Symbol::from_byte(144 + idx as u8) {
            name_overrides.insert(sym, user_function.name.clone());
        }
    }
    symbol::set_name_overrides(name_overrides);

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
            eprintln!("ries: Replacing -i with -r because target isn't an integer.");
            (false, true, false) // Fallback to rational mode
        } else {
            (true, false, false)
        }
    } else {
        (args.integer, args.rational, false)
    };

    // Determine numeric type restriction
    // Check liouvillian_override first (from -l legacy semantics)
    let min_type = if integer_mode {
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

    // Build generation config with CLI options
    let gen_config = match build_gen_config(
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
        match_all_digits: args.match_all_digits,
        derivative_margin: args.derivative_margin.unwrap_or(thresholds::DEGENERATE_DERIVATIVE),
    };

    // When --match-all-digits is enabled, set tolerance based on target's significant digits
    if args.match_all_digits && args.max_match_distance.is_none() {
        search_config.max_error = compute_significant_digits_tolerance(target);
    }

    if args.one_sided {
        // One-sided mode ranks direct x = RHS matches, so keep only display count.
        search_config.max_matches = effective_max_matches;
    }

    let start = Instant::now();

    // Build symbol filters for fast path
    let mut excluded_symbols: std::collections::HashSet<u8> =
        excluded_effective.unwrap_or_default();
    if let Some(rhs_excluded) = &search_config.rhs_excluded_symbols {
        excluded_symbols.extend(rhs_excluded.iter().copied());
    }

    let fast_allowed_storage: Option<std::collections::HashSet<u8>> =
        match (allowed_effective.as_ref(), search_config.rhs_allowed_symbols.as_ref()) {
            (Some(all_set), Some(rhs_set)) => {
                Some(all_set.intersection(rhs_set).copied().collect())
            }
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
            fast_match::find_fast_match(target, &profile.constants, &fast_config)
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
            perform_search(
                &gen_config,
                &search_config,
                args.streaming,
                args.parallel,
                args.one_sided,
            )
        }
    } else {
        // Not in quick mode, always do full search
        perform_search(
            &gen_config,
            &search_config,
            args.streaming,
            args.parallel,
            args.one_sided,
        )
    };

    let mut matches = matches;
    if args.min_equate_value.is_some() || args.max_equate_value.is_some() {
        matches.retain(|m| match_in_equate_bounds(m, args.min_equate_value, args.max_equate_value));
    }
    if let Some(min_match_distance) = args.min_match_distance {
        matches.retain(|m| m.error.abs() >= min_match_distance);
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
    if matches.is_empty() {
        println!("   No matches found.");
    } else if !use_report {
        // Classic mode: single list sorted by complexity
        let output_format = parse_display_format(&args.format);
        let shown: Vec<&search::Match> = matches.iter().take(effective_max_matches).collect();
        for m in shown.iter().copied() {
            let show_solve = args.solve && !args.no_solve;
            if args.absolute {
                print_match_absolute(m, show_solve, output_format, args.explicit_multiply);
            } else {
                print_match_relative(m, show_solve, output_format, args.explicit_multiply);
            }
        }

        if diagnostics.show_work {
            print_show_work_details(
                &shown,
                output_format,
                args.explicit_multiply,
                &profile.constants,
                &profile.functions,
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
            eprintln!(
                "Warning: --show-work/-Ds is currently only available with --report false."
            );
        }
        let mut report_config = ReportConfig::default()
            .with_top_k(args.top_k)
            .with_target(target);

        if args.no_stable {
            report_config = report_config.without_stable();
        }

        let report = Report::generate(matches, target, &report_config);
        report.print(args.absolute, args.solve && !args.no_solve);
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
}

fn format_value(v: f64) -> String {
    if v.abs() >= 1e6 || (v.abs() < 1e-4 && v != 0.0) {
        format!("{:.10e}", v)
    } else {
        format!("{:.10}", v)
    }
}

/// Compute tolerance for --match-all-digits based on significant digits of the target
///
/// When --match-all-digits is enabled, the match tolerance is set so that matches
/// must agree with the target value to all significant digits provided.
///
/// For example:
/// - Target "2.5" (1 sig fig after decimal) -> tolerance ~0.05 (half of last digit)
/// - Target "2.50" (2 sig figs after decimal) -> tolerance ~0.005
/// - Target "2.500" (3 sig figs after decimal) -> tolerance ~0.0005
fn compute_significant_digits_tolerance(target: f64) -> f64 {
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

fn print_header(target: f64, level: i32) {
    println!();
    println!("  Target: {}", target);
    println!("  Level: {}", level);
    println!();
}

fn print_footer(stats: &search::SearchStats, elapsed: std::time::Duration) {
    println!();
    println!("  === Summary ===");
    let total_tested = stats.lhs_tested.saturating_add(stats.candidates_tested);
    println!("  Total expressions tested: {}", total_tested);
    println!("  LHS expressions: {}", stats.lhs_count);
    println!("  RHS expressions: {}", stats.rhs_count);
    println!("  Search time: {:.3}s", elapsed.as_secs_f64());
}

/// Parse output format from string
fn parse_display_format(s: &str) -> DisplayFormat {
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

fn apply_explicit_multiply(infix: &str) -> String {
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

fn format_expression_for_display(
    expression: &expr::Expression,
    format: DisplayFormat,
    explicit_multiply: bool,
) -> String {
    match format {
        DisplayFormat::Infix(inner) => {
            let infix = expression.to_infix_with_format(inner);
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

fn print_match_relative(
    m: &search::Match,
    solve: bool,
    format: DisplayFormat,
    explicit_multiply: bool,
) {
    let lhs_str = format_expression_for_display(&m.lhs.expr, format, explicit_multiply);
    let rhs_str = format_expression_for_display(&m.rhs.expr, format, explicit_multiply);

    let error_str = if m.error.abs() < EXACT_MATCH_TOLERANCE {
        "('exact' match)".to_string()
    } else {
        let sign = if m.error >= 0.0 { "+" } else { "-" };
        format!("for x = T {} {:.6e}", sign, m.error.abs())
    };

    // Note: The -s flag is intended to transform equations to x = ... form, but proper
    // algebraic solving is complex. For now, show the equation form to avoid misleading
    // output (e.g., showing "x = RHS" when the actual equation is "tanpi(x) = RHS").
    let _ = solve; // Suppress unused warning until proper transformation is implemented
    println!(
        "{:>24} = {:<24} {} {{{}}}",
        lhs_str, rhs_str, error_str, m.complexity
    );
}

fn print_match_absolute(
    m: &search::Match,
    solve: bool,
    format: DisplayFormat,
    explicit_multiply: bool,
) {
    let lhs_str = format_expression_for_display(&m.lhs.expr, format, explicit_multiply);
    let rhs_str = format_expression_for_display(&m.rhs.expr, format, explicit_multiply);

    // Note: The -s flag is intended to transform equations to x = ... form, but proper
    // algebraic solving is complex. For now, show the equation form to avoid misleading
    // output (e.g., showing "x = RHS" when the actual equation is "tanpi(x) = RHS").
    let _ = solve; // Suppress unused warning until proper transformation is implemented
    println!(
        "{:>24} = {:<24} for x = {:.15} {{{}}}",
        lhs_str, rhs_str, m.x_value, m.complexity
    );
}

fn expression_from_symbols(symbols: &[symbol::Symbol]) -> expr::Expression {
    let mut expression = expr::Expression::new();
    for &sym in symbols {
        expression.push(sym);
    }
    expression
}

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

fn print_expression_steps(
    label: &str,
    expression: &expr::Expression,
    x: f64,
    format: DisplayFormat,
    explicit_multiply: bool,
    user_constants: &[profile::UserConstant],
    user_functions: &[udf::UserFunction],
) {
    println!("    {} steps:", label);
    for (idx, step_expr) in decompose_subexpressions(expression).iter().enumerate() {
        let rendered = format_expression_for_display(step_expr, format, explicit_multiply);
        match eval::evaluate_with_constants_and_functions(
            step_expr,
            x,
            user_constants,
            user_functions,
        ) {
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

fn print_show_work_details(
    shown_matches: &[&search::Match],
    format: DisplayFormat,
    explicit_multiply: bool,
    user_constants: &[profile::UserConstant],
    user_functions: &[udf::UserFunction],
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
            user_constants,
            user_functions,
        );
        print_expression_steps(
            "RHS",
            &m.rhs.expr,
            m.x_value,
            format,
            explicit_multiply,
            user_constants,
            user_functions,
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
}

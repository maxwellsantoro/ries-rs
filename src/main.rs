//! RIES-RS: Find algebraic equations given their solution
//!
//! A Rust implementation of Robert Munafo's RIES program.

// Allow field reassignment with default in test code - common pattern for config building
#![cfg_attr(test, allow(clippy::field_reassign_with_default))]

mod eval;
mod expr;
mod fast_match;
mod gen;
mod manifest;
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
use manifest::{MatchInfo, RunManifest, SearchConfigInfo};
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

    /// Force deterministic output (disables parallelism, uses stable sorting)
    /// Required for reproducible results in academic papers
    #[arg(long)]
    deterministic: bool,

    /// Use streaming search for lower memory usage at high complexity levels
    /// Streaming processes expressions on-the-fly instead of accumulating in memory
    #[arg(long)]
    streaming: bool,

    /// Use report mode with categorized output (default: true)
    /// Shows top matches in each category: exact, best, elegant, interesting, stable
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    report: bool,

    /// Classic/sniper mode: single list output (like original RIES)
    /// Implies --stop-at-exact and aggressive early exit for speed
    #[arg(long)]
    classic: bool,

    /// Use original-RIES-like signed weight ranking for match ordering
    /// (exactness -> error -> legacy parity score -> complexity)
    #[arg(long, conflicts_with = "complexity_ranking")]
    parity_ranking: bool,

    /// Force complexity-first ranking
    /// (exactness -> error -> complexity)
    #[arg(long, conflicts_with = "parity_ranking")]
    complexity_ranking: bool,

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

    /// Compatibility alias for wide output formatting (accepted for parity; no-op)
    #[arg(long)]
    wide: bool,

    /// Compatibility alias for wide output formatting (accepted for parity; no-op)
    #[arg(long = "wide-output")]
    wide_output: bool,

    /// Compatibility alias for relative roots display (accepted for parity; no-op)
    #[arg(long = "relative-roots")]
    relative_roots: bool,

    /// Disable rational-exponents filtering when combined with --rational-exponents
    #[arg(long = "any-exponents")]
    any_exponents: bool,

    /// Clear numeric-type subexpression restrictions (-a/-c/-r/-i/--liouvillian-subexpressions)
    #[arg(long = "any-subexpressions")]
    any_subexpressions: bool,

    /// Disable rational-trig-args filtering when combined with --rational-trig-args
    #[arg(long = "any-trig-args")]
    any_trig_args: bool,

    /// Canonical reduction mode used by compatibility dedupe pass
    #[arg(long = "canon-reduction")]
    canon_reduction: Option<String>,

    /// Enable canonical simplification pass for match deduplication
    #[arg(long = "canon-simplify")]
    canon_simplify: bool,

    /// Override Newton derivative threshold used to detect degenerate derivatives
    #[arg(long = "derivative-margin")]
    derivative_margin: Option<f64>,

    /// Force explicit '*' in infix display output
    #[arg(long = "explicit-multiply")]
    explicit_multiply: bool,

    /// Require matches to agree with target precision (uses target significant digits)
    #[arg(long = "match-all-digits")]
    match_all_digits: bool,

    /// Reject matches where either side exceeds this value
    #[arg(long = "max-equate-value")]
    max_equate_value: Option<f64>,

    /// Memory budget hint used by streaming fallback heuristics (e.g. 512M, 2G)
    #[arg(long = "max-memory")]
    max_memory: Option<String>,

    /// Threshold used with --max-memory to trigger streaming fallback
    #[arg(long = "memory-abort-threshold")]
    memory_abort_threshold: Option<f64>,

    /// Maximum number of trig operators allowed in accepted matches
    #[arg(long = "max-trig-cycles")]
    max_trig_cycles: Option<u32>,

    /// Reject matches where either side is below this value
    #[arg(long = "min-equate-value")]
    min_equate_value: Option<f64>,

    /// Lower memory bound hint used by streaming fallback heuristics
    #[arg(long = "min-memory")]
    min_memory: Option<String>,

    /// Disable canonical simplification even if requested elsewhere
    #[arg(long = "no-canon-simplify")]
    no_canon_simplify: bool,

    /// Suppress compatibility warnings and slow-path informational warnings
    #[arg(long = "no-slow-messages")]
    no_slow_messages: bool,

    /// Restrict matches to those sharing target digit-anagram signature
    #[arg(long = "numeric-anagram")]
    numeric_anagram: bool,

    /// Restrict accepted matches to rational exponent forms
    #[arg(long = "rational-exponents")]
    rational_exponents: bool,

    /// Restrict accepted matches to rational trig arguments
    #[arg(long = "rational-trig-args")]
    rational_trig_args: bool,

    /// Compatibility alias for diagnostics channel -Ds (show work)
    #[arg(long = "show-work")]
    show_work: bool,

    /// Legacy alias for derivative margin when --derivative-margin is not set
    #[arg(long = "significance-loss-margin")]
    significance_loss_margin: Option<f64>,

    /// Scale factor applied to trig arguments during evaluation
    #[arg(long = "trig-argument-scale")]
    trig_argument_scale: Option<f64>,

    /// Show verbose output with header and footer details
    #[arg(long)]
    verbose: bool,

    /// Emit a run manifest JSON file for reproducibility
    /// Contains full configuration and results for academic verification
    #[arg(long, value_name = "FILE")]
    emit_manifest: Option<PathBuf>,
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
            return Err(format!("Unknown symbol in --symbol-names: {}", symbol_char));
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
fn parse_symbol_count_limits(
    spec: &str,
) -> Result<std::collections::HashMap<symbol::Symbol, u32>, String> {
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
            return Err(format!(
                "Unknown symbol '{}' in -O/--op-limits",
                symbol_char
            ));
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
    let (allowed_rhs, excluded_rhs) = parse_symbol_sets(only_symbols_rhs, exclude_rhs, enable_rhs);

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
    show_work: bool,  // s, N
    show_stats: bool, // y, M
    // NEW channels
    show_match_checks: bool, // o
    show_pruned_arith: bool, // A, a
    show_pruned_range: bool, // B, b
    show_db_adds: bool,      // G, g
    show_newton: bool,       // n
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
        const COMPAT_NOOP_CHANNELS: &str = "CcDdEeFfHhIiJjKkLlPpQqRrTtUuVvWwXxZz";
        for ch in spec.chars() {
            match ch {
                's' | 'N' => opts.show_work = true,
                'y' | 'M' => opts.show_stats = true,
                'o' => opts.show_match_checks = true,
                'A' | 'a' => opts.show_pruned_arith = true,
                'B' | 'b' => opts.show_pruned_range = true,
                'G' | 'g' => opts.show_db_adds = true,
                'n' => opts.show_newton = true,
                _ if COMPAT_NOOP_CHANNELS.contains(ch) => {
                    // Recognized for compatibility; currently no-op.
                }
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

fn parse_memory_size_bytes(spec: &str) -> Option<u64> {
    let trimmed = spec.trim();
    if trimmed.is_empty() {
        return None;
    }

    let (num_part, suffix) = match trimmed.chars().last().filter(|c| c.is_ascii_alphabetic()) {
        Some(last) => (&trimmed[..trimmed.len() - last.len_utf8()], Some(last)),
        None => (trimmed, None),
    };

    let number: f64 = num_part.trim().parse().ok()?;
    if !number.is_finite() || number < 0.0 {
        return None;
    }

    let mult = match suffix.map(|c| c.to_ascii_uppercase()) {
        None => 1_f64,
        Some('K') => 1024_f64,
        Some('M') => 1024_f64 * 1024_f64,
        Some('G') => 1024_f64 * 1024_f64 * 1024_f64,
        Some('T') => 1024_f64 * 1024_f64 * 1024_f64 * 1024_f64,
        _ => return None,
    };

    Some((number * mult) as u64)
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

fn canon_reduction_enabled(spec: Option<&str>) -> bool {
    let Some(value) = spec else {
        return false;
    };
    let lowered = value.trim().to_ascii_lowercase();
    !matches!(lowered.as_str(), "" | "off" | "none" | "0" | "false")
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

    // Warn about unimplemented precision flag
    if !args.no_slow_messages && args.precision.is_some() {
        eprintln!(
            "Warning: --precision flag specified but high-precision mode is not yet implemented."
        );
        eprintln!("         Using standard f64 precision (~15 digits).");
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

    let mut use_streaming = args.streaming;
    let parsed_max_memory = args.max_memory.as_deref().and_then(parse_memory_size_bytes);
    let parsed_min_memory = args.min_memory.as_deref().and_then(parse_memory_size_bytes);
    if !use_streaming {
        if let Some(max_bytes) = parsed_max_memory {
            if max_bytes <= 512 * 1024 * 1024 {
                use_streaming = true;
            }
        }
    }
    if use_streaming {
        if let Some(min_bytes) = parsed_min_memory {
            if min_bytes >= 2 * 1024 * 1024 * 1024 {
                use_streaming = false;
            }
        }
    }
    if let (Some(max_bytes), Some(threshold)) = (parsed_max_memory, args.memory_abort_threshold) {
        if (0.0..=1.0).contains(&threshold) {
            let budget = (max_bytes as f64 * threshold) as u64;
            let estimate = (pool_size as u64).saturating_mul(4096).saturating_add(
                (max_lhs_complexity as u64 + max_rhs_complexity as u64).saturating_mul(1_000_000),
            );
            if estimate > budget {
                use_streaming = true;
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
            // Deterministic mode disables parallelism for reproducible results
            let use_parallel = !args.deterministic && args.parallel;
            perform_search(
                &gen_config,
                &search_config,
                use_streaming,
                use_parallel,
                args.one_sided,
            )
        }
    } else {
        // Not in quick mode, always do full search
        // Deterministic mode disables parallelism for reproducible results
        let use_parallel = !args.deterministic && args.parallel;
        perform_search(
            &gen_config,
            &search_config,
            use_streaming,
            use_parallel,
            args.one_sided,
        )
    };

    let mut matches = matches;

    // Deterministic mode: apply stable sorting to ensure reproducible order
    // This handles any remaining non-determinism from pool ordering
    if args.deterministic {
        matches.sort_by(|a, b| pool::compare_matches(a, b, ranking_mode));
    }

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
    let solved_rhs = if solve {
        solve_for_x_rhs_expression(&m.lhs.expr, &m.rhs.expr)
    } else {
        None
    };

    let lhs_expr = if solved_rhs.is_some() {
        let mut x_expr = expr::Expression::new();
        x_expr.push(symbol::Symbol::X);
        x_expr
    } else {
        m.lhs.expr.clone()
    };
    let rhs_expr = solved_rhs.as_ref().unwrap_or(&m.rhs.expr);

    let lhs_str = format_expression_for_display(&lhs_expr, format, explicit_multiply);
    let rhs_str = format_expression_for_display(rhs_expr, format, explicit_multiply);

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

fn print_match_absolute(
    m: &search::Match,
    solve: bool,
    format: DisplayFormat,
    explicit_multiply: bool,
) {
    let solved_rhs = if solve {
        solve_for_x_rhs_expression(&m.lhs.expr, &m.rhs.expr)
    } else {
        None
    };

    let lhs_expr = if solved_rhs.is_some() {
        let mut x_expr = expr::Expression::new();
        x_expr.push(symbol::Symbol::X);
        x_expr
    } else {
        m.lhs.expr.clone()
    };
    let rhs_expr = solved_rhs.as_ref().unwrap_or(&m.rhs.expr);

    let lhs_str = format_expression_for_display(&lhs_expr, format, explicit_multiply);
    let rhs_str = format_expression_for_display(rhs_expr, format, explicit_multiply);

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

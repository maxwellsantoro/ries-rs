//! Command-line argument parsing for RIES
//!
//! This module defines the CLI interface using clap, including all argument
//! definitions and parsing helpers.

use clap::{ArgAction, Parser};
use std::path::PathBuf;

use crate::profile;
use crate::symbol;
use crate::udf;

/// Find algebraic equations given their solution
#[derive(Parser, Debug)]
#[command(name = "ries-rs")]
#[command(author = "RIES Contributors")]
#[command(version = "0.1.0")]
#[command(about = "Find algebraic equations given their solution", long_about = None)]
pub struct Args {
    /// Target value to find equations for (optional if using --eval-expression)
    pub target: Option<f64>,

    /// Search level (each increment ? 10x more equations)
    /// Level 0 ? 89M equations, Level 2 ? 11B, Level 5 ? 15T
    /// Legacy: "-l 2.5" (with no explicit target) means Liouvillian mode + target 2.5
    #[arg(short = 'l', long, default_value = "2")]
    pub level: String,

    /// Maximum number of matches to display
    #[arg(short = 'n', long = "max-matches", default_value = "16")]
    pub max_matches: usize,

    /// Show absolute x values instead of T ? error
    #[arg(short = 'x', long, alias = "absolute-roots")]
    pub absolute: bool,

    /// Try to solve for x (show x = ... form)
    #[arg(short = 's', long, alias = "try-solve-for-x")]
    pub solve: bool,

    /// Disable solve-for-x presentation even if requested elsewhere
    #[arg(long = "no-solve-for-x")]
    pub no_solve: bool,

    /// Symbols to never use (e.g., "+-" to exclude add/subtract)
    #[arg(short = 'N', long)]
    pub exclude: Option<String>,

    /// Re-enable symbols disabled by exclude/only options, or enable all if no argument
    #[arg(short = 'E', long = "enable", num_args = 0..=1, default_missing_value = "all")]
    pub enable: Option<String>,

    /// Only use these symbols, or print symbol table if no argument
    /// Using -S alone prints the full symbol table and exits
    #[arg(short = 'S', long, num_args = 0..=1)]
    pub only_symbols: Option<String>,

    /// Operator/symbol count limits (C RIES -O semantics).
    /// Example: "-Ox" means at most one x per side; "-O2+" means at most two '+'.
    #[arg(short = 'O', long)]
    pub op_limits: Option<String>,

    /// Only use these symbols on RHS (right-hand side)
    #[arg(long = "S-RHS")]
    pub only_symbols_rhs: Option<String>,

    /// Exclude these symbols on RHS
    #[arg(long = "N-RHS")]
    pub exclude_rhs: Option<String>,

    /// Re-enable symbols on RHS
    #[arg(long = "E-RHS")]
    pub enable_rhs: Option<String>,

    /// RHS-only symbol count limits (like -O but RHS only)
    #[arg(long = "O-RHS")]
    pub op_limits_rhs: Option<String>,

    /// Custom symbol weights (e.g., ":W:20" sets Lambert W weight to 20)
    #[arg(long)]
    pub symbol_weights: Option<String>,

    /// Custom symbol names (e.g., ":p:PI" renames pi to PI in output)
    #[arg(long)]
    pub symbol_names: Option<String>,

    /// Restrict to algebraic solutions
    #[arg(short = 'a', long, alias = "algebraic-subexpressions")]
    pub algebraic: bool,

    /// Restrict to constructible solutions
    #[arg(short = 'c', long, alias = "constructible-subexpressions")]
    pub constructible: bool,

    /// Restrict to rational solutions
    #[arg(short = 'r', long, alias = "rational-subexpressions")]
    pub rational: bool,

    /// Restrict to integer solutions
    #[arg(short = 'i', long, alias = "integer-subexpressions")]
    pub integer: bool,

    /// Integer exact mode (equivalent to -i --stop-at-exact)
    /// Stops at first integer match
    #[arg(long = "ie")]
    pub integer_exact: bool,

    /// Rational exact mode (equivalent to -r --stop-at-exact)
    /// Stops at first rational match
    #[arg(long = "re")]
    pub rational_exact: bool,

    /// Restrict to Liouvillian subexpressions
    #[arg(long = "liouvillian-subexpressions")]
    pub liouvillian: bool,

    /// Use parallel search (default: true)
    #[arg(long, default_value = "true")]
    pub parallel: bool,

    /// Force deterministic output (disables parallelism, uses stable sorting)
    /// Required for reproducible results in academic papers
    #[arg(long)]
    pub deterministic: bool,

    /// Use streaming search for lower memory usage at high complexity levels
    /// Streaming processes expressions on-the-fly instead of accumulating in memory
    #[arg(long)]
    pub streaming: bool,

    /// Use adaptive search (like original RIES) for better precision
    /// Iteratively expands LHS/RHS complexity to match original RIES expression counts
    /// Generates ~500K expressions at level 2 (vs ~3K with default mode)
    #[arg(long)]
    pub adaptive: bool,

    /// Use report mode with categorized output (default: true)
    /// Shows top matches in each category: exact, best, elegant, interesting, stable
    #[arg(long, default_value_t = true, action = ArgAction::Set)]
    pub report: bool,

    /// Classic/sniper mode: single list output (like original RIES)
    /// Implies --stop-at-exact and aggressive early exit for speed
    #[arg(long)]
    pub classic: bool,

    /// Use original-RIES-like signed weight ranking for match ordering
    /// (exactness -> error -> legacy parity score -> complexity)
    #[arg(long, conflicts_with = "complexity_ranking")]
    pub parity_ranking: bool,

    /// Force complexity-first ranking
    /// (exactness -> error -> complexity)
    #[arg(long, conflicts_with = "parity_ranking")]
    pub complexity_ranking: bool,

    /// Output format for expressions
    /// Options: default, pretty (Unicode), mathematica, sympy
    #[arg(short = 'F', long, num_args = 0..=1, default_missing_value = "3", default_value = "2")]
    pub format: String,

    /// Compatibility diagnostics flag (-D, -Dy, etc.)
    #[arg(short = 'D', num_args = 0..=1, default_missing_value = "")]
    pub diagnostics: Option<String>,

    /// Number of matches per category in report mode
    #[arg(short = 'k', long = "top-k", default_value = "8")]
    pub top_k: usize,

    /// Exclude stability category from the report
    #[arg(long)]
    pub no_stable: bool,

    /// Show detailed search statistics
    #[arg(long)]
    pub stats: bool,

    /// Emit machine-readable JSON results to stdout (suppresses text report output)
    #[arg(long)]
    pub json: bool,

    /// Print list of supported options and exit
    #[arg(long = "list-options")]
    pub list_options: bool,

    /// Use a domain-specific preset for symbol weights and constants
    /// Available: analytic-nt, elliptic, combinatorics, physics, number-theory, calculus
    /// Use --list-presets to see descriptions
    #[arg(long)]
    pub preset: Option<String>,

    /// List available domain presets and exit
    #[arg(long)]
    pub list_presets: bool,

    /// Run stability analysis (impostor detection)
    /// Runs search at multiple tolerance levels and classifies candidates
    /// Use --stability-thorough for more precision levels
    #[arg(long)]
    pub stability_check: bool,

    /// Use thorough stability analysis (more precision levels, slower)
    #[arg(long)]
    pub stability_thorough: bool,

    /// Stop search when an exact match is found
    #[arg(long)]
    pub stop_at_exact: bool,

    /// Stop search when error goes below this threshold
    #[arg(long)]
    pub stop_below: Option<f64>,

    /// Load profile file for custom constants and symbol settings
    #[arg(short = 'p', long)]
    pub profile: Option<PathBuf>,

    /// Include additional profile file (can be used multiple times)
    #[arg(long)]
    pub include: Vec<PathBuf>,

    /// Add a user-defined constant
    /// Format: "weight:name:description:value"
    /// Example: -X "4:gamma:Euler's constant:0.5772156649"
    #[arg(short = 'X', long = "user-constant", alias = "constant")]
    pub user_constant: Vec<String>,

    /// Define a custom operation/function
    /// Format: "weight:name:description:formula"
    /// Formula uses postfix notation with | for dup and @ for swap
    /// Example: --define "4:sinh:hyperbolic sine:E|r-2/"
    #[arg(long)]
    pub define: Vec<String>,

    /// Maximum acceptable error for matches (default: 1% of |target|)
    #[arg(long, alias = "mad")]
    pub max_match_distance: Option<f64>,

    /// Minimum error threshold (exclude matches closer than this)
    #[arg(long)]
    pub min_match_distance: Option<f64>,

    /// Use one-sided mode: only generate RHS expressions, compare directly to target
    #[arg(long)]
    pub one_sided: bool,

    /// Skip Newton-Raphson refinement of matches
    #[arg(long)]
    pub no_refinement: bool,

    /// Evaluate an expression at a given x value and exit
    /// Example: --eval-expression "xq" --at 2.5
    #[arg(long)]
    pub eval_expression: Option<String>,

    /// Evaluate and display a specific expression (compatibility option)
    #[arg(long)]
    pub find_expression: Option<String>,

    /// X value for --eval-expression
    #[arg(long)]
    pub at: Option<f64>,

    /// Maximum Newton-Raphson iterations for root refinement (default: 15)
    #[arg(long, default_value = "15")]
    pub newton_iterations: usize,

    /// Precision in bits for high-precision verification (e.g., 256 for ~77 digits)
    /// Re-evaluates top matches at higher precision to verify/impostor detection
    /// Requires --features highprec at compile time
    #[arg(long)]
    pub precision: Option<u32>,

    /// Threshold for pruning LHS expressions with near-zero values (default: 1e-4)
    #[arg(long)]
    pub zero_threshold: Option<f64>,

    /// Compatibility alias for wide output formatting (accepted for parity; no-op)
    #[arg(long)]
    pub wide: bool,

    /// Compatibility alias for wide output formatting (accepted for parity; no-op)
    #[arg(long = "wide-output")]
    pub wide_output: bool,

    /// Compatibility alias for relative roots display (accepted for parity; no-op)
    #[arg(long = "relative-roots")]
    pub relative_roots: bool,

    /// Disable rational-exponents filtering when combined with --rational-exponents
    #[arg(long = "any-exponents")]
    pub any_exponents: bool,

    /// Clear numeric-type subexpression restrictions (-a/-c/-r/-i/--liouvillian-subexpressions)
    #[arg(long = "any-subexpressions")]
    pub any_subexpressions: bool,

    /// Disable rational-trig-args filtering when combined with --rational-trig-args
    #[arg(long = "any-trig-args")]
    pub any_trig_args: bool,

    /// Canonical reduction mode used by compatibility dedupe pass
    #[arg(long = "canon-reduction")]
    pub canon_reduction: Option<String>,

    /// Enable canonical simplification pass for match deduplication
    #[arg(long = "canon-simplify")]
    pub canon_simplify: bool,

    /// Override Newton derivative threshold used to detect degenerate derivatives
    #[arg(long = "derivative-margin")]
    pub derivative_margin: Option<f64>,

    /// Force explicit '*' in infix display output
    #[arg(long = "explicit-multiply")]
    pub explicit_multiply: bool,

    /// Require matches to agree with target precision (uses target significant digits)
    #[arg(long = "match-all-digits")]
    pub match_all_digits: bool,

    /// Reject matches where either side exceeds this value
    #[arg(long = "max-equate-value")]
    pub max_equate_value: Option<f64>,

    /// Memory budget hint used by streaming fallback heuristics (e.g. 512M, 2G)
    #[arg(long = "max-memory")]
    pub max_memory: Option<String>,

    /// Threshold used with --max-memory to trigger streaming fallback
    #[arg(long = "memory-abort-threshold")]
    pub memory_abort_threshold: Option<f64>,

    /// Maximum number of trig operators allowed in accepted matches
    #[arg(long = "max-trig-cycles")]
    pub max_trig_cycles: Option<u32>,

    /// Reject matches where either side is below this value
    #[arg(long = "min-equate-value")]
    pub min_equate_value: Option<f64>,

    /// Lower memory bound hint used by streaming fallback heuristics
    #[arg(long = "min-memory")]
    pub min_memory: Option<String>,

    /// Disable canonical simplification even if requested elsewhere
    #[arg(long = "no-canon-simplify")]
    pub no_canon_simplify: bool,

    /// Suppress compatibility warnings and slow-path informational warnings
    #[arg(long = "no-slow-messages")]
    pub no_slow_messages: bool,

    /// Restrict matches to those sharing target digit-anagram signature
    #[arg(long = "numeric-anagram")]
    pub numeric_anagram: bool,

    /// Restrict accepted matches to rational exponent forms
    #[arg(long = "rational-exponents")]
    pub rational_exponents: bool,

    /// Restrict accepted matches to rational trig arguments
    #[arg(long = "rational-trig-args")]
    pub rational_trig_args: bool,

    /// Compatibility alias for diagnostics channel -Ds (show work)
    #[arg(long = "show-work")]
    pub show_work: bool,

    /// Legacy alias for derivative margin when --derivative-margin is not set
    #[arg(long = "significance-loss-margin")]
    pub significance_loss_margin: Option<f64>,

    /// Scale factor applied to trig arguments during evaluation
    #[arg(long = "trig-argument-scale")]
    pub trig_argument_scale: Option<f64>,

    /// Show verbose output with header and footer details
    #[arg(long)]
    pub verbose: bool,

    /// Run PSLQ integer relation detection on the target
    /// Searches for integer coefficients relating target to known constants
    /// Example: ries-rs 3.14159 --pslq might find x - ? 0
    #[arg(long)]
    pub pslq: bool,

    /// Use extended constant set for PSLQ (includes ?3, ?5, ln(3), etc.)
    #[arg(long)]
    pub pslq_extended: bool,

    /// Maximum coefficient magnitude for PSLQ search (default: 1000)
    #[arg(long, default_value = "1000")]
    pub pslq_max_coeff: i64,

    /// Emit a run manifest JSON file for reproducibility
    /// Contains full configuration and results for academic verification
    #[arg(long, value_name = "FILE")]
    pub emit_manifest: Option<PathBuf>,
}

/// Parse a user constant from CLI argument.
/// Format: "weight:name:description:value"
pub fn parse_user_constant_from_cli(
    profile: &mut profile::Profile,
    spec: &str,
) -> Result<(), String> {
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

/// Check if a value is likely rational (simple fraction).
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

/// Parse a user-defined function from CLI argument.
/// Format: "weight:name:description:formula"
pub fn parse_user_function_from_cli(
    profile: &mut profile::Profile,
    spec: &str,
) -> Result<(), String> {
    let udf = udf::UserFunction::parse(spec)?;
    profile.functions.push(udf);
    Ok(())
}

/// Parse symbol names from CLI argument.
/// Format: ":p:PI :e:EULER"
pub fn parse_symbol_names_from_cli(
    profile: &mut profile::Profile,
    spec: &str,
) -> Result<(), String> {
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

/// Parse symbol weights from CLI argument.
/// Format: ":W:20 :p:25"
pub fn parse_symbol_weights_from_cli(
    profile: &mut profile::Profile,
    spec: &str,
) -> Result<(), String> {
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
pub fn parse_symbol_count_limits(
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

/// Parse effective allowed/excluded symbol sets with optional re-enable set.
pub fn parse_symbol_sets(
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

/// Parse a memory size string into bytes.
///
/// Supports suffixes: K, M, G, T (case-insensitive).
/// Examples: "512M" -> 536870912, "2G" -> 2147483648
pub fn parse_memory_size_bytes(spec: &str) -> Option<u64> {
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

/// Check if canonical reduction is enabled from the option value.
pub fn canon_reduction_enabled(spec: Option<&str>) -> bool {
    let Some(value) = spec else {
        return false;
    };
    let lowered = value.trim().to_ascii_lowercase();
    !matches!(lowered.as_str(), "" | "off" | "none" | "0" | "false")
}

/// Print the list of supported options (for --list-options).
pub fn print_option_list() {
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
}

/// Get a description for a symbol (for -S symbol table output).
pub fn sym_description(sym: symbol::Symbol) -> &'static str {
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

/// Print the symbol table (for -S without argument).
pub fn print_symbol_table() {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_memory_size() {
        assert_eq!(parse_memory_size_bytes("512M"), Some(512 * 1024 * 1024));
        assert_eq!(parse_memory_size_bytes("2G"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_memory_size_bytes("1024"), Some(1024));
        assert_eq!(parse_memory_size_bytes("1k"), Some(1024));
    }

    #[test]
    fn test_parse_symbol_sets() {
        let (allowed, excluded) = parse_symbol_sets(Some("abc"), Some("d"), None);
        assert_eq!(allowed, Some(vec![b'a', b'b', b'c'].into_iter().collect()));
        assert_eq!(excluded, Some(vec![b'd'].into_iter().collect()));

        // Test enable all
        let (_, excluded) = parse_symbol_sets(None, Some("abc"), Some("all"));
        assert!(excluded.is_none());
    }
}

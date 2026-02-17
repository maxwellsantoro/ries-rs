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
    #[arg(short = 'l', long, default_value = "2")]
    level: f32,

    /// Maximum number of matches to display
    #[arg(short = 'n', long = "max-matches", default_value = "16")]
    max_matches: usize,

    /// Show absolute x values instead of T ± error
    #[arg(short = 'x', long)]
    absolute: bool,

    /// Try to solve for x (show x = ... form)
    #[arg(short = 's', long)]
    solve: bool,

    /// Symbols to never use (e.g., "+-" to exclude add/subtract)
    #[arg(short = 'N', long)]
    exclude: Option<String>,

    /// Only use these symbols
    #[arg(short = 'S', long)]
    only_symbols: Option<String>,

    /// Operator count limits (e.g., "-O+-" = one each of + and -)
    /// Format: each character is a symbol, prefix with number for count (e.g., "-O2+-" = two + and one -)
    #[arg(short = 'O', long)]
    op_limits: Option<String>,

    /// Only use these symbols on RHS (right-hand side)
    #[arg(long = "S-RHS")]
    only_symbols_rhs: Option<String>,

    /// Exclude these symbols on RHS
    #[arg(long = "N-RHS")]
    exclude_rhs: Option<String>,

    /// Custom symbol weights (e.g., ":W:20" sets Lambert W weight to 20)
    #[arg(long)]
    symbol_weights: Option<String>,

    /// Restrict to algebraic solutions
    #[arg(short = 'a', long)]
    algebraic: bool,

    /// Restrict to constructible solutions
    #[arg(short = 'c', long)]
    constructible: bool,

    /// Restrict to rational solutions
    #[arg(short = 'r', long)]
    rational: bool,

    /// Restrict to integer solutions
    #[arg(short = 'i', long)]
    integer: bool,

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
    #[arg(short = 'F', long, default_value = "default")]
    format: String,

    /// Number of matches per category in report mode
    #[arg(short = 'k', long = "top-k", default_value = "8")]
    top_k: usize,

    /// Exclude stability category from the report
    #[arg(long)]
    no_stable: bool,

    /// Show detailed search statistics
    #[arg(long)]
    stats: bool,

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
    #[arg(short = 'X', long = "user-constant")]
    user_constant: Vec<String>,

    /// Define a custom operation/function
    /// Format: "weight:name:description:formula"
    /// Formula uses postfix notation with | for dup and @ for swap
    /// Example: --define "4:sinh:hyperbolic sine:E|r-2/"
    #[arg(long)]
    define: Vec<String>,

    /// Maximum acceptable error for matches (default: 1.0)
    #[arg(long)]
    max_match_distance: Option<f64>,

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

    // Determine numeric type based on value characteristics
    let num_type = if value.fract() == 0.0 && value.abs() < 1e10 {
        NumType::Integer
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

/// Parse a user-defined function from CLI argument
/// Format: "weight:name:description:formula"
fn parse_user_function_from_cli(profile: &mut Profile, spec: &str) -> Result<(), String> {
    let udf = udf::UserFunction::parse(spec)?;
    profile.functions.push(udf);
    Ok(())
}

/// Build a GenConfig from CLI options
#[allow(clippy::too_many_arguments)]
#[allow(clippy::field_reassign_with_default)]
fn build_gen_config(
    max_lhs_complexity: u32,
    max_rhs_complexity: u32,
    min_type: symbol::NumType,
    exclude: Option<&str>,
    only_symbols: Option<&str>,
    exclude_rhs: Option<&str>,
    only_symbols_rhs: Option<&str>,
    op_limits: Option<&str>,
    user_constants: Vec<profile::UserConstant>,
    user_functions: Vec<udf::UserFunction>,
) -> gen::GenConfig {
    let mut config = gen::GenConfig::default();
    config.max_lhs_complexity = max_lhs_complexity;
    config.max_rhs_complexity = max_rhs_complexity;
    config.min_num_type = min_type;
    config.user_constants = user_constants.clone();
    config.user_functions = user_functions.clone();

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

    // Parse symbol sets
    let allowed: Option<std::collections::HashSet<u8>> = only_symbols.map(|s| s.bytes().collect());
    let excluded: Option<std::collections::HashSet<u8>> = exclude.map(|s| s.bytes().collect());
    // RHS-specific filtering (for future use when GenConfig supports separate RHS symbols)
    let _allowed_rhs: Option<std::collections::HashSet<u8>> =
        only_symbols_rhs.map(|s| s.bytes().collect());
    let _excluded_rhs: Option<std::collections::HashSet<u8>> =
        exclude_rhs.map(|s| s.bytes().collect());

    // Parse operator limits (simplified - just filter to allowed operators)
    let op_limit_allowed: Option<std::collections::HashSet<u8>> = op_limits.map(|s| {
        let mut set = std::collections::HashSet::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            // Check for numeric prefix (e.g., "2+" means two + signs)
            // For now we just track which operators are allowed, not the count
            if chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < chars.len() {
                set.insert(chars[i] as u8);
                i += 1;
            }
        }
        set
    });

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

    // Apply operator limits if specified
    if let Some(ref op_allowed) = op_limit_allowed {
        config
            .unary_ops
            .retain(|s| op_allowed.contains(&(*s as u8)));
        config
            .binary_ops
            .retain(|s| op_allowed.contains(&(*s as u8)));
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

    // Note: RHS-specific filtering would require extending GenConfig
    // For now, RHS uses the same symbols as LHS

    config
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
    target: f64,
    gen_config: &gen::GenConfig,
    pool_size: usize,
    stop_at_exact: bool,
    stop_below: Option<f64>,
    streaming: bool,
    parallel: bool,
) -> (Vec<search::Match>, search::SearchStats) {
    if streaming {
        search::search_streaming(target, gen_config, pool_size, stop_at_exact, stop_below)
    } else {
        #[cfg(feature = "parallel")]
        {
            if parallel {
                search::search_parallel_with_stats_and_options(
                    target, gen_config, pool_size, stop_at_exact, stop_below,
                )
            } else {
                search::search_with_stats_and_options(
                    target, gen_config, pool_size, stop_at_exact, stop_below,
                )
            }
        }
        #[cfg(not(feature = "parallel"))]
        {
            search::search_with_stats_and_options(
                target, gen_config, pool_size, stop_at_exact, stop_below,
            )
        }
    }
}

fn main() {
    let args = Args::parse();

    // Warn about unimplemented precision flag
    if args.precision.is_some() {
        eprintln!(
            "Warning: --precision flag specified but high-precision mode is not yet implemented."
        );
        eprintln!("         Using standard f64 precision (~15 digits).");
    }

    // Load profile early (needed for both --eval-expression and search modes)
    let mut profile = Profile::load_from(args.profile.as_deref());

    // Include additional profiles
    for include_path in &args.include {
        if let Ok(included) = Profile::from_file(include_path) {
            profile = profile.merge(included);
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
    let target = match args.target {
        Some(t) => t,
        None => {
            eprintln!("Error: TARGET is required unless using --eval-expression");
            std::process::exit(1);
        }
    };

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
    let level_factor = 4.0 * args.level;
    let max_lhs_complexity = (base_lhs + level_factor) as u32;
    let max_rhs_complexity = (base_rhs + level_factor) as u32;

    // Determine numeric type restriction
    let min_type = if args.integer {
        symbol::NumType::Integer
    } else if args.rational {
        symbol::NumType::Rational
    } else if args.constructible {
        symbol::NumType::Constructible
    } else if args.algebraic {
        symbol::NumType::Algebraic
    } else {
        symbol::NumType::Transcendental
    };

    // Build generation config with CLI options
    let gen_config = build_gen_config(
        max_lhs_complexity,
        max_rhs_complexity,
        min_type,
        args.exclude.as_deref(),
        args.only_symbols.as_deref(),
        args.exclude_rhs.as_deref(),
        args.only_symbols_rhs.as_deref(),
        args.op_limits.as_deref(),
        profile.constants.clone(),
        profile.functions.clone(),
    );

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
    let stop_at_exact = if args.classic && !args.stop_at_exact {
        true
    } else {
        args.stop_at_exact
    };

    let stop_below = if args.classic && args.stop_below.is_none() {
        Some(1e-10_f64.max(target.abs() * 1e-12))
    } else {
        args.stop_below
    };

    let start = Instant::now();

    // Build excluded symbols set for fast path
    let excluded_symbols: std::collections::HashSet<u8> = args.exclude
        .as_ref()
        .map(|s| s.bytes().collect())
        .unwrap_or_default();

    // Build fast match config
    let fast_config = fast_match::FastMatchConfig {
        excluded_symbols: &excluded_symbols,
        min_num_type: min_type,
    };

    // Fast path: check for simple exact matches before expensive generation
    // This handles cases like pi, e, sqrt(2), phi, integers, etc. instantly
    let (matches, stats) = if stop_at_exact || args.classic {
        // Only use fast path when we're looking for quick results
        if let Some(fast_match) = fast_match::find_fast_match(target, &profile.constants, &fast_config) {
            use search::SearchStats;
            let mut fast_stats = SearchStats::default();
            fast_stats.lhs_count = 1;
            fast_stats.rhs_count = 1;
            fast_stats.search_time = std::time::Duration::from_micros(1);
            (vec![fast_match], fast_stats)
        } else {
            // No fast match found, do full search
            perform_search(
                target,
                &gen_config,
                pool_size,
                stop_at_exact,
                stop_below,
                args.streaming,
                args.parallel,
            )
        }
    } else {
        // Not in quick mode, always do full search
        perform_search(
            target,
            &gen_config,
            pool_size,
            stop_at_exact,
            stop_below,
            args.streaming,
            args.parallel,
        )
    };

    let elapsed = start.elapsed();

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
        let output_format = parse_format(&args.format);
        for m in matches.iter().take(effective_max_matches) {
            if args.absolute {
                print_match_absolute(m, args.solve, output_format);
            } else {
                print_match_relative(m, args.solve, output_format);
            }
        }

        // Print footer
        println!();
        if matches.len() >= effective_max_matches {
            let next_level = (args.level + 1.0) as i32;
            println!(
                "                  (for more results, use the option '-l{}')",
                next_level
            );
        }
    } else {
        // Report mode: categorized output
        let mut report_config = ReportConfig::default()
            .with_top_k(args.top_k)
            .with_target(target);

        if args.no_stable {
            report_config = report_config.without_stable();
        }

        let report = Report::generate(matches, target, &report_config);
        report.print(args.absolute, args.solve);
    }

    println!();
    println!("  Search completed in {:.3}s", elapsed.as_secs_f64());

    // Print detailed stats if requested
    if args.stats {
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

/// Parse output format from string
fn parse_format(s: &str) -> expr::OutputFormat {
    match s.to_lowercase().as_str() {
        "pretty" | "unicode" => expr::OutputFormat::Pretty,
        "mathematica" | "math" | "mma" => expr::OutputFormat::Mathematica,
        "sympy" | "python" => expr::OutputFormat::SymPy,
        _ => expr::OutputFormat::Default,
    }
}

fn print_match_relative(m: &search::Match, solve: bool, format: expr::OutputFormat) {
    let lhs_str = m.lhs.expr.to_infix_with_format(format);
    let rhs_str = m.rhs.expr.to_infix_with_format(format);

    let error_str = if m.error.abs() < 1e-14 {
        "('exact' match)".to_string()
    } else {
        let sign = if m.error >= 0.0 { "+" } else { "-" };
        format!("for x = T {} {:.6e}", sign, m.error.abs())
    };

    if solve {
        println!("     x = {:40} {} {{{}}}", rhs_str, error_str, m.complexity);
    } else {
        println!(
            "{:>24} = {:<24} {} {{{}}}",
            lhs_str, rhs_str, error_str, m.complexity
        );
    }
}

fn print_match_absolute(m: &search::Match, solve: bool, format: expr::OutputFormat) {
    let lhs_str = m.lhs.expr.to_infix_with_format(format);
    let rhs_str = m.rhs.expr.to_infix_with_format(format);

    if solve {
        println!(
            "     x = {:40} for x = {:.15} {{{}}}",
            rhs_str, m.x_value, m.complexity
        );
    } else {
        println!(
            "{:>24} = {:<24} for x = {:.15} {{{}}}",
            lhs_str, rhs_str, m.x_value, m.complexity
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

//! RIES-RS: Find algebraic equations given their solution
//!
//! A Rust implementation of Robert Munafo's RIES program.

// Allow field reassignment with default in test code - common pattern for config building
#![cfg_attr(test, allow(clippy::field_reassign_with_default))]
use ries_rs::{
    eval, expr, format_stability_report_with, format_verification_report_with, gen, pool,
    print_presets, profile, search, symbol, udf, verify_matches_highprec_with_trig_scale,
    FastMatchConfig, Report, ReportConfig, DEGENERATE_DERIVATIVE,
};

mod cli;

use clap::{CommandFactory, FromArgMatches};
use cli::{
    build_gen_config, build_json_output, build_manifest, canon_reduction_enabled,
    cli_level_to_complexity, expression_constraints_from_args, format_bytes_binary, format_value,
    handle_special_modes, load_runtime_profile, normalize_legacy_args, parse_diagnostics,
    parse_display_format, parse_memory_size_bytes, parse_symbol_sets, peak_memory_bytes,
    print_footer, print_header, print_match_absolute, print_match_relative,
    print_show_work_details, print_symbol_table, run_search, run_stability_check, Args, CliExit,
    DisplayFormat, ExpressionConstraintArgs, NormalizedArgs, StabilityRunConfig,
};
#[cfg(test)]
use ries_rs::solve_for_x_rhs_expression;
use ries_rs::{canonical_expression_key, solve_for_x_at_with_context};
use std::time::Duration;

// Args struct is now imported from cli::Args

fn exit_with(err: CliExit) -> ! {
    eprintln!("{}", err.message);
    std::process::exit(err.code);
}

fn main() {
    #[cfg(unix)]
    // SAFETY: SIG_DFL restores the OS action for a closed output pipe.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let arg_matches = Args::command().get_matches();
    let explicit_target_literal = arg_matches
        .get_raw("target")
        .and_then(|mut values| values.next())
        .and_then(|value| value.to_str());
    let args = Args::from_arg_matches(&arg_matches).unwrap_or_else(|err| err.exit());

    if args.list_options {
        cli::print_option_list();
        return;
    }

    // Handle --list-presets (print available presets and exit)
    if args.list_presets {
        print_presets();
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
    }

    let mut trig_argument_scale = eval::DEFAULT_TRIG_ARGUMENT_SCALE;
    if let Some(scale) = args.trig_argument_scale {
        if scale.is_finite() && scale != 0.0 {
            trig_argument_scale = scale;
        } else if !args.no_slow_messages {
            eprintln!(
                "Warning: --trig-argument-scale must be finite and non-zero (got {}).",
                scale
            );
        }
    }

    // Handle legacy argument semantics using the normalize_legacy_args function
    let NormalizedArgs {
        target: resolved_target,
        target_literal,
        profile: profile_arg,
        enable: enable_arg,
        level: level_value,
        liouvillian: liouvillian_override,
    } = normalize_legacy_args(
        args.profile
            .as_deref()
            .map(|p| p.to_string_lossy())
            .as_deref(),
        args.enable.as_deref(),
        &args.level,
        args.target,
    )
    .unwrap_or_else(|err| exit_with(CliExit::config(err)));

    let profile = match load_runtime_profile(&args, profile_arg.as_deref()) {
        Ok(profile) => profile,
        Err(err) => exit_with(err),
    };

    match handle_special_modes(&args, resolved_target, &profile, trig_argument_scale) {
        Ok(true) => return,
        Ok(false) => {}
        Err(err) => exit_with(err),
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
    if !args.json {
        println!();
        println!(
            "   Your target value: T = {:<20}  ries-rs v{}",
            format_value(target),
            env!("CARGO_PKG_VERSION")
        );
        println!();
    }

    // Convert level to CLI complexity limits.
    let (max_lhs_complexity, max_rhs_complexity) = cli_level_to_complexity(level_value);

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
    } else if args.liouvillian || liouvillian_override {
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
        &profile,
        diagnostics.show_pruned_arith,
    ) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(2);
        }
    };

    // Report mode retains extra candidates for its categories.
    const MAX_POOL_SIZE: usize = 1_000_000;
    const MIN_REPORT_POOL_SIZE: usize = 5_000;
    let use_report = args.report && !args.classic;
    let Some((effective_max_matches, pool_size)) = (|| {
        let displayed = if use_report {
            args.max_matches.max(args.top_k.checked_mul(10)?)
        } else {
            args.max_matches
        };
        let capacity = if use_report {
            displayed.checked_mul(10)?.max(MIN_REPORT_POOL_SIZE)
        } else {
            displayed
        };
        (capacity <= MAX_POOL_SIZE).then_some((displayed, capacity))
    })() else {
        eprintln!(
            "Error: --max-matches and --top-k must request a search pool of at most {MAX_POOL_SIZE} entries."
        );
        std::process::exit(2);
    };

    if args
        .max_match_distance
        .is_some_and(|distance| !distance.is_finite() || distance < 0.0)
    {
        eprintln!("Error: --max-match-distance must be finite and nonnegative.");
        std::process::exit(2);
    }

    // Classic mode = "sniper mode": stop early like original RIES
    // Also stop at exact for --ie/--re exact modes
    let stop_at_exact = args.classic || exact_mode || args.stop_at_exact;

    let stop_below = if args.classic && args.stop_below.is_none() {
        Some(1e-10_f64.max(target.abs() * 1e-12))
    } else {
        args.stop_below
    };

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

    let (expression_constraints, expression_constraints_active) = expression_constraints_from_args(
        &profile,
        ExpressionConstraintArgs {
            rational_exponents: args.rational_exponents,
            any_exponents: args.any_exponents,
            rational_trig_args: args.rational_trig_args,
            any_trig_args: args.any_trig_args,
            max_trig_cycles: args.max_trig_cycles,
        },
    );
    let canon_enabled = (args.canon_simplify
        || canon_reduction_enabled(args.canon_reduction.as_deref()))
        && !args.no_canon_simplify;
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
        trig_argument_scale,
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
            .unwrap_or(DEGENERATE_DERIVATIVE),
        ranking_mode,
        lhs_value_dedup: args.lhs_value_dedup,
        filters: search::MatchFilters {
            min_equate_value: args.min_equate_value,
            max_equate_value: args.max_equate_value,
            min_match_distance: args.min_match_distance,
            expression_constraints: expression_constraints_active.then_some(expression_constraints),
            numeric_anagram: args.numeric_anagram,
            canonical: canon_enabled,
        },
    };

    // When --match-all-digits is enabled, set tolerance based on target's significant digits
    if args.match_all_digits && args.max_match_distance.is_none() {
        search_config.max_error = search::significant_digits_tolerance(
            target,
            explicit_target_literal.or(target_literal.as_deref()),
        );
    }

    if args.one_sided {
        // One-sided mode ranks direct x = RHS matches, so keep only display count.
        search_config.max_matches = effective_max_matches;
    }

    let eval_context = search_config.context().eval;

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

    let use_parallel = !args.deterministic && args.parallel;
    let use_turbo = !args.deterministic && args.turbo;

    // A quick result must satisfy the same limits as an ordinary match.
    let fast_match = if stop_at_exact && !use_streaming && search_config.max_matches > 0 {
        let excluded = std::collections::HashSet::new();
        let fast_config = FastMatchConfig {
            excluded_symbols: search_config
                .rhs_excluded_symbols
                .as_ref()
                .unwrap_or(&excluded),
            allowed_symbols: search_config.rhs_allowed_symbols.as_ref(),
            min_num_type: min_type,
        };
        ries_rs::find_fast_match_with_context(
            target,
            &eval_context,
            &fast_config,
            &gen_config.symbol_table,
        )
        .filter(|m| {
            m.error.abs() <= search_config.max_error
                && gen_config.allows_evaluated_side(&m.lhs)
                && gen_config.allows_evaluated_side(&m.rhs)
                && search_config.filters.allows_match(m)
        })
    } else {
        None
    };

    let (mut matches, stats, elapsed) = if let Some(m) = fast_match {
        let elapsed = Duration::from_micros(1);
        let stats = search::SearchStats {
            matcher: Some(search::MatcherPath::FastMatch),
            lhs_count: 1,
            rhs_count: 1,
            pool_final_size: 1,
            pool_best_error: m.error.abs(),
            search_time: elapsed,
            ..Default::default()
        };
        (vec![m], stats, elapsed)
    } else {
        let result = run_search(
            &gen_config,
            &search_config,
            use_streaming,
            use_parallel,
            args.one_sided,
            args.adaptive,
            use_turbo,
            level_value as u32,
        );
        (result.matches, result.stats, result.elapsed)
    };

    // Streaming already orders its results and may put a stop match first.
    if (args.deterministic || args.turbo) && stats.matcher != Some(search::MatcherPath::Streaming) {
        matches.sort_by(|a, b| pool::compare_matches(a, b, ranking_mode));
    }

    let stability_results = if args.stability_check {
        let base_matches = (!search_config.stop_at_exact && search_config.stop_below.is_none())
            .then(|| matches.clone());
        Some(run_stability_check(
            base_matches,
            StabilityRunConfig {
                gen_config: &gen_config,
                search_config: &search_config,
                thorough: args.stability_thorough,
                use_streaming,
                use_parallel,
                one_sided: args.one_sided,
                adaptive: args.adaptive,
                turbo: use_turbo,
                level: level_value as u32,
            },
        ))
    } else {
        None
    };

    // Parse the output format once for both text and JSON modes
    let output_format = parse_display_format(&args.format);

    if let Some(path) = &args.emit_manifest {
        let manifest = build_manifest(
            &args,
            &gen_config,
            &search_config,
            level_value,
            effective_max_matches,
            &stats,
            &matches,
        );
        let json = manifest.to_json().unwrap_or_else(|err| {
            eprintln!("Error serializing manifest: {err}");
            std::process::exit(1);
        });
        if let Err(err) = std::fs::write(path, json) {
            eprintln!("Error writing manifest: {err}");
            std::process::exit(1);
        }
        if !args.no_slow_messages {
            eprintln!("Manifest written to {}", path.display());
        }
    }

    let verification = args
        .precision
        .filter(|_| cfg!(feature = "highprec"))
        .map(|bits| {
            let results = verify_matches_highprec_with_trig_scale(
                matches
                    .iter()
                    .take(effective_max_matches)
                    .cloned()
                    .collect(),
                target,
                bits,
                &profile.constants,
                trig_argument_scale,
            );
            (bits, results)
        });

    if args.json {
        let shown_count = matches.len().min(effective_max_matches);
        let mut json_output = build_json_output(
            target,
            level_value,
            max_lhs_complexity,
            max_rhs_complexity,
            effective_max_matches,
            ranking_mode,
            search_config.lhs_value_dedup,
            args.deterministic,
            cli::json_types::uses_parallelism(stats.matcher, use_parallel, &gen_config),
            args.adaptive,
            args.one_sided,
            use_report,
            output_format,
            args.explicit_multiply,
            &gen_config.symbol_table,
            &eval_context,
            &matches[..shown_count],
            &stats,
            elapsed,
            args.solve && !args.no_solve,
        );

        if let Some((bits, results)) = &verification {
            json_output.high_precision_verification =
                Some(cli::json_types::JsonVerification::new(*bits, results));
        }

        match serde_json::to_string_pretty(&json_output) {
            Ok(json) => println!("{json}"),
            Err(e) => {
                eprintln!("Error serializing JSON output: {}", e);
                std::process::exit(1);
            }
        }

        return;
    }

    if trig_argument_scale != eval::DEFAULT_TRIG_ARGUMENT_SCALE
        && !matches!(output_format, DisplayFormat::Infix(_))
    {
        println!("Trig argument scale: {trig_argument_scale}");
    }

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
        let shown: Vec<&search::Match> = matches.iter().take(effective_max_matches).collect();
        for m in shown.iter().copied() {
            let show_solve = args.solve && !args.no_solve;
            let solved = show_solve
                .then(|| {
                    solve_for_x_at_with_context(&m.lhs.expr, &m.rhs.expr, m.x_value, &eval_context)
                })
                .flatten();
            if args.absolute {
                print_match_absolute(
                    m,
                    show_solve,
                    output_format,
                    args.explicit_multiply,
                    solved.as_ref(),
                    Some(&gen_config.symbol_table),
                    trig_argument_scale,
                );
            } else {
                print_match_relative(
                    m,
                    show_solve,
                    output_format,
                    args.explicit_multiply,
                    solved.as_ref(),
                    Some(&gen_config.symbol_table),
                    trig_argument_scale,
                );
            }
        }

        if diagnostics.show_work {
            print_show_work_details(
                &shown,
                output_format,
                args.explicit_multiply,
                &eval_context,
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

        let report = Report::generate(matches, target, &report_config);
        report.print_with_context(
            args.absolute,
            args.solve && !args.no_solve,
            output_format,
            args.explicit_multiply,
            Some(&gen_config.symbol_table),
            &eval_context,
        );
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
        if let Some(peak_rss) = peak_memory_bytes() {
            println!(
                "    Peak RSS:       {:>10} ({})",
                peak_rss,
                format_bytes_binary(peak_rss)
            );
        }
    }

    let render = |expression: &expr::Expression| {
        output_format.format_expression(
            expression,
            args.explicit_multiply,
            Some(&gen_config.symbol_table),
            trig_argument_scale,
        )
    };

    // Print stability analysis if requested
    if let Some(ref results) = stability_results {
        println!();
        println!("  === Stability Analysis ===");
        print!(
            "{}",
            format_stability_report_with(results, effective_max_matches, render)
        );
    }

    if let Some((bits, results)) = verification {
        println!("\n  === High-Precision Verification ({bits} bits) ===");
        print!(
            "{}",
            format_verification_report_with(&results, effective_max_matches, render)
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

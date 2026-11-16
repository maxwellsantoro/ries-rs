//! Additive, literal-preserving discovery mode. Ordinary CLI search is unchanged.

use clap::{parser::ValueSource, ArgMatches, CommandFactory};

use super::{Args, CliExit};
use ries_rs::{discover, DiscoveryOptions};

/// Dispatch a self-contained discovery run rather than silently ignoring raw CLI
/// configuration. All accepted settings appear in the discovery result envelope.
pub fn run(args: &Args, matches: &ArgMatches, literal: Option<&str>) -> Result<(), CliExit> {
    const ALLOWED: &[&str] = &[
        "discover",
        "target",
        "level",
        "max_matches",
        "preset",
        "json",
        "parallel",
        "deterministic",
        "precision",
        "allow_unverified",
    ];
    // Derive also creates an `Args` argument group. It is not a supplied flag.
    let command = Args::command();
    for id in matches.ids() {
        if command
            .get_arguments()
            .any(|argument| argument.get_id() == id)
            && matches.value_source(id.as_str()) == Some(ValueSource::CommandLine)
            && !ALLOWED.contains(&id.as_str())
        {
            return Err(CliExit::config(format!(
                "Error: --discover does not accept '{}'; use ordinary search for advanced CLI configuration",
                id.as_str()
            )));
        }
    }
    let literal = literal.ok_or_else(|| CliExit::config("Error: --discover requires TARGET"))?;
    let level = args.level.parse::<u32>().map_err(|_| {
        CliExit::config("Error: discovery level must be an integer from 0 to 5 (API budget scale)")
    })?;
    if args.precision.is_some()
        && (!cfg!(feature = "highprec") || args.precision.is_some_and(|bits| bits <= 53))
        && !args.allow_unverified
    {
        return Err(CliExit::config(
            "Error: requested verification is unavailable; use a highprec build with --precision > 53 or explicitly pass --allow-unverified",
        ));
    }
    let run = discover(
        literal,
        &DiscoveryOptions {
            level,
            max_matches: args.max_matches,
            preset: args.preset.clone(),
            parallel: args.parallel && !args.deterministic,
            verification_bits: args.precision,
        },
    )
    .map_err(CliExit::config)?;
    if args.precision.is_some()
        && !args.allow_unverified
        && run
            .hypotheses
            .iter()
            .any(|hypothesis| hypothesis.validation.high_precision.status == "unavailable")
    {
        return Err(CliExit::config(
            "Error: high-precision verification unavailable for one or more hypotheses; use --allow-unverified to accept an explicit unavailable status",
        ));
    }
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&run).map_err(|error| CliExit::config(format!(
                "Error: serializing discovery: {error}"
            )))?
        );
    } else {
        println!("Numerical hypotheses for {}", run.target_literal);
        println!(
            "Input tolerance: {:.6e}; discovery API level: {}; {} distinct hypotheses",
            run.config.input_tolerance,
            run.config.level,
            run.hypotheses.len()
        );
        for hypothesis in &run.hypotheses {
            let summary = &hypothesis.representative;
            let equation = summary
                .solve_for_x
                .clone()
                .unwrap_or_else(|| format!("{} = {}", summary.lhs, summary.rhs));
            println!(
                "  {}  error={:.6e} complexity={} [{}]",
                equation,
                summary.error,
                summary.complexity,
                if hypothesis.within_input_precision {
                    "within input precision"
                } else {
                    "approximation"
                }
            );
            println!(
                "    High-precision: {} ({})",
                hypothesis.validation.high_precision.status,
                hypothesis.validation.high_precision.reason
            );
        }
        println!("Numerical agreement is not a symbolic proof. Use --json to retain the complete run and validation availability.");
    }
    Ok(())
}

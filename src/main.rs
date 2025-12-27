//! RIES-RS: Find algebraic equations given their solution
//!
//! A Rust implementation of Robert Munafo's RIES program.

mod eval;
mod expr;
mod gen;
mod search;
mod symbol;

use clap::Parser;
use std::time::Instant;

/// Find algebraic equations given their solution
#[derive(Parser, Debug)]
#[command(name = "ries-rs")]
#[command(author = "RIES Contributors")]
#[command(version = "0.1.0")]
#[command(about = "Find algebraic equations given their solution", long_about = None)]
struct Args {
    /// Target value to find equations for
    target: f64,

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
}

fn main() {
    let args = Args::parse();

    // Print header
    println!();
    println!(
        "   Your target value: T = {:<20}  ries-rs v0.1.0",
        format_value(args.target)
    );
    println!();

    // Convert level to complexity limit
    // Level 0 ≈ 50, Level 2 ≈ 75 (default), Level 5 ≈ 130
    let base_complexity: f32 = 35.0;
    let max_complexity = (base_complexity * (1.2_f32).powf(args.level + 2.0)) as u16;

    // Determine numeric type restriction
    let _min_type = if args.integer {
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

    let start = Instant::now();

    // Perform search
    #[cfg(feature = "parallel")]
    let matches = if args.parallel {
        search::search_parallel(args.target, max_complexity, args.max_matches)
    } else {
        search::search(args.target, max_complexity, args.max_matches)
    };

    #[cfg(not(feature = "parallel"))]
    let matches = search::search(args.target, max_complexity, args.max_matches);

    let elapsed = start.elapsed();

    // Display matches
    if matches.is_empty() {
        println!("   No matches found.");
    } else {
        for m in &matches {
            if args.absolute {
                print_match_absolute(m, args.solve);
            } else {
                print_match_relative(m, args.solve);
            }
        }
    }

    // Print footer
    println!();
    if matches.len() >= args.max_matches {
        let next_level = (args.level + 1.0) as i32;
        println!("                  (for more results, use the option '-l{}')", next_level);
    }

    println!();
    println!(
        "  Search completed in {:.3}s",
        elapsed.as_secs_f64()
    );
}

fn format_value(v: f64) -> String {
    if v.abs() >= 1e6 || (v.abs() < 1e-4 && v != 0.0) {
        format!("{:.10e}", v)
    } else {
        format!("{:.10}", v)
    }
}

fn print_match_relative(m: &search::Match, solve: bool) {
    let lhs_str = m.lhs.expr.to_infix();
    let rhs_str = m.rhs.expr.to_infix();

    let error_str = if m.error.abs() < 1e-14 {
        "('exact' match)".to_string()
    } else {
        let sign = if m.error >= 0.0 { "+" } else { "-" };
        format!("for x = T {} {:.6e}", sign, m.error.abs())
    };

    if solve {
        // Try to display as x = ...
        println!(
            "     x = {:40} {} {{{}}}",
            rhs_str, error_str, m.complexity
        );
    } else {
        println!(
            "{:>24} = {:<24} {} {{{}}}",
            lhs_str, rhs_str, error_str, m.complexity
        );
    }
}

fn print_match_absolute(m: &search::Match, solve: bool) {
    let lhs_str = m.lhs.expr.to_infix();
    let rhs_str = m.rhs.expr.to_infix();

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
    fn test_format_value() {
        assert_eq!(format_value(3.14159), "3.1415900000");
        assert_eq!(format_value(1e10), "1.0000000000e10");
    }
}

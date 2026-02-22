//! Custom configuration example
//!
//! Demonstrates how to customize search parameters.
//! Run with: cargo run --example custom_config

use ries_rs::{search, GenConfig, OutputFormat};

fn main() {
    // Example 1: Quick search with lower complexity limits
    println!("Example 1: Quick search with limited complexity");
    println!("{:-<60}", "");

    let target = std::f64::consts::SQRT_2; // sqrt(2)

    let quick_config = GenConfig {
        max_lhs_complexity: 15,
        max_rhs_complexity: 10,
        ..GenConfig::default()
    };

    let matches = search(target, &quick_config, 5);

    for (i, m) in matches.iter().enumerate() {
        println!(
            "  {}. {} = {}",
            i + 1,
            m.lhs.expr.to_infix_with_format(OutputFormat::Pretty),
            m.rhs.expr.to_infix_with_format(OutputFormat::Pretty)
        );
    }

    println!();

    // Example 2: Search with higher complexity for more results
    println!("Example 2: Search for golden ratio approximations");
    println!("{:-<60}", "");

    let phi = 1.61803398874989;

    let detailed_config = GenConfig {
        max_lhs_complexity: 25,
        max_rhs_complexity: 20,
        ..GenConfig::default()
    };

    let matches = search(phi, &detailed_config, 8);

    for (i, m) in matches.iter().enumerate() {
        println!(
            "  {}. {} = {}  (complexity: {})",
            i + 1,
            m.lhs.expr.to_infix_with_format(OutputFormat::Pretty),
            m.rhs.expr.to_infix_with_format(OutputFormat::Pretty),
            m.complexity
        );
    }

    println!("{:-<60}", "");
    println!("Done!");
}

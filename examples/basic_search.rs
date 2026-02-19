//! Basic search example
//!
//! Run with: cargo run --example basic_search
//! Or with a custom target: cargo run --example basic_search -- 3.14159

use ries_rs::{search, GenConfig, OutputFormat};

fn main() {
    let target = std::env::args().nth(1).unwrap_or_else(|| "2.5".to_string());
    let target: f64 = target.parse().expect("Please provide a valid number");

    println!("Searching for equations where x = {}", target);
    println!("{:-<60}", "");

    // Use lower complexity limits for a quick demo search
    let config = GenConfig {
        max_lhs_complexity: 20,
        max_rhs_complexity: 15,
        ..GenConfig::default()
    };
    let matches = search(target, &config, 20);

    for (i, m) in matches.iter().enumerate() {
        println!(
            "{:3}: {} = {}  (error: {:.2e}, complexity: {})",
            i + 1,
            m.lhs.expr.to_infix_with_format(OutputFormat::Pretty),
            m.rhs.expr.to_infix_with_format(OutputFormat::Pretty),
            m.error,
            m.complexity
        );
    }

    println!("{:-<60}", "");
    println!("Found {} matches", matches.len());
}

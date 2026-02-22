//! Streaming search example
//!
//! Run with: cargo run --example streaming -- 3.14159
//!
//! Streaming search reduces memory overhead by processing expressions as they're
//! generated, rather than accumulating all candidates in memory before matching.
//! This is essential for deep searches (level 4-5) where billions of expressions
//! might be generated.

use ries_rs::{gen::GenConfig, search::{search_streaming_with_config, SearchConfig}, OutputFormat};
use std::time::Instant;

fn main() {
    let target = std::env::args().nth(1).unwrap_or_else(|| "3.1415926535".to_string());
    let target: f64 = target.parse().expect("Please provide a valid number");

    println!("Performing deep streaming search for target: {}", target);
    println!("{:-<60}", "");

    // Very low complexity for instant feedback
    let gen_config = GenConfig {
        max_lhs_complexity: 25, 
        max_rhs_complexity: 20,
        ..GenConfig::default()
    };

    let search_config = SearchConfig {
        target,
        max_matches: 10,
        stop_at_exact: true,
        ..SearchConfig::default()
    };

    println!("Starting search...");
    let start = Instant::now();
    
    // Call the streaming search API
    // This mode reduces memory by building the RHS database incrementally
    // via streaming, rather than generating both sides into memory first.
    // While the LHS still requires buffering for the final match pass,
    // the peak memory is significantly lower than a full batch search.
    let (matches, stats) = search_streaming_with_config(&gen_config, &search_config);
    
    let elapsed = start.elapsed();

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
    println!("Search Statistics:");
    println!("  LHS expressions: {:>10}", stats.lhs_count);
    println!("  RHS expressions: {:>10}", stats.rhs_count);
    println!("  Total time:      {:>10.2?}", elapsed);
    println!("  Memory profile:  Streaming (O(depth) rather than O(N))");
}

//! Expensive debug tests for expression generation
//!
//! These tests use high complexity limits and all operators.
//! Run with `cargo test -- --ignored` to include them.

#![cfg(not(target_arch = "wasm32"))]

use ries_rs::gen::{generate_all, GenConfig};

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_x_to_x_generated() {
    let config = GenConfig {
        max_lhs_complexity: 50,
        max_rhs_complexity: 50,
        ..Default::default()
    };

    let result = generate_all(&config, 2.5);

    // Check if xx^ (x^x) is generated
    let has_xx_pow = result.lhs.iter().any(|e| e.expr.to_postfix() == "xx^");

    println!("LHS contains xx^ (x^x): {}", has_xx_pow);

    // Find expressions with value near 9.88 (x^x at 2.5)
    let near_xx: Vec<_> = result
        .lhs
        .iter()
        .filter(|e| (e.value - 9.88).abs() < 0.5)
        .take(5)
        .collect();

    println!("\nLHS expressions with value ≈ 9.88:");
    for e in &near_xx {
        println!(
            "  {} = {} (value={:.4}, deriv={:.4})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value,
            e.derivative
        );
    }

    assert!(has_xx_pow, "xx^ should be generated");
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_pi_squared_in_rhs() {
    let config = GenConfig {
        max_lhs_complexity: 50,
        max_rhs_complexity: 50,
        ..Default::default()
    };

    let result = generate_all(&config, 2.5);

    // Check for pi^2 (postfix: ps)
    let has_pi_sq = result.rhs.iter().any(|e| e.expr.to_postfix() == "ps");
    println!("RHS contains ps (pi^2): {}", has_pi_sq);

    // Find RHS near 9.87 (pi^2)
    let near_pi2: Vec<_> = result
        .rhs
        .iter()
        .filter(|e| (e.value - 9.87).abs() < 0.5)
        .take(5)
        .collect();

    println!("\nRHS expressions with value ≈ 9.87 (pi^2):");
    for e in &near_pi2 {
        println!(
            "  {} = {} (value={:.6})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value
        );
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_pi_squared_value() {
    let config = GenConfig {
        max_lhs_complexity: 60,
        max_rhs_complexity: 60,
        ..Default::default()
    };

    let result = generate_all(&config, 2.5);

    // Find RHS with value exactly near pi^2 = 9.8696
    let pi_sq = std::f64::consts::PI * std::f64::consts::PI;
    println!("pi^2 = {}", pi_sq);

    let near_pi2: Vec<_> = result
        .rhs
        .iter()
        .filter(|e| (e.value - pi_sq).abs() < 0.01)
        .collect();

    println!("\nRHS expressions with value within 0.01 of pi^2:");
    for e in &near_pi2 {
        println!(
            "  {} = {} (value={:.10})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value
        );
    }

    // Also check what's at value 9.882 (x^x at 2.5)
    let xx_val = 2.5_f64.powf(2.5);
    println!("\nx^x at 2.5 = {}", xx_val);

    let near_xx: Vec<_> = result
        .rhs
        .iter()
        .filter(|e| (e.value - xx_val).abs() < 0.02)
        .collect();

    println!("\nRHS expressions with value within 0.02 of x^x:");
    for e in &near_xx {
        println!(
            "  {} = {} (value={:.10})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value
        );
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_find_ps_specifically() {
    let config = GenConfig {
        max_lhs_complexity: 60,
        max_rhs_complexity: 60,
        ..Default::default()
    };

    let result = generate_all(&config, 2.5);

    // Find ps specifically
    let ps_expr = result.rhs.iter().find(|e| e.expr.to_postfix() == "ps");

    if let Some(e) = ps_expr {
        println!(
            "Found ps: {} = {} (value={:.10})",
            e.expr.to_postfix(),
            e.expr.to_infix(),
            e.value
        );
    } else {
        println!("ps not found in deduplicated RHS!");

        // Check what expression has the same quantized value
        let pi_sq = std::f64::consts::PI * std::f64::consts::PI;
        let key = (pi_sq * 1e8).round() as i64;
        println!("Key for pi^2 = {}", key);

        // Find all expressions with same key
        let same_key: Vec<_> = result
            .rhs
            .iter()
            .filter(|e| (e.value * 1e8).round() as i64 == key)
            .collect();

        println!("\nExpressions with same key:");
        for e in &same_key {
            println!(
                "  {} = {} (value={:.10}, complexity={})",
                e.expr.to_postfix(),
                e.expr.to_infix(),
                e.value,
                e.expr.complexity()
            );
        }
    }
}

#[test]
#[ignore = "expensive debug test - run with --ignored flag"]
fn test_xx_in_final_lhs() {
    let config = GenConfig {
        max_lhs_complexity: 50,
        max_rhs_complexity: 50,
        ..Default::default()
    };

    let result = generate_all(&config, 2.5);

    // Check if xx^ is in final deduplicated LHS
    let xx_expr = result.lhs.iter().find(|e| e.expr.to_postfix() == "xx^");

    if let Some(e) = xx_expr {
        println!(
            "xx^ in final LHS: {} (value={:.4}, deriv={:.4}, complexity={})",
            e.expr.to_infix(),
            e.value,
            e.derivative,
            e.expr.complexity()
        );
    } else {
        println!("xx^ NOT in final LHS - was deduplicated");

        // Find what has the same key
        let xx_val = 2.5_f64.powf(2.5);
        let xx_deriv = xx_val * (1.0 + 2.5_f64.ln());
        println!("Expected: value={:.4}, deriv={:.4}", xx_val, xx_deriv);

        let key_val = (xx_val * 1e8).round() as i64;
        let key_deriv = (xx_deriv * 1e8).round() as i64;
        println!("Key: ({}, {})", key_val, key_deriv);

        // Find expressions with same key
        let same: Vec<_> = result
            .lhs
            .iter()
            .filter(|e| {
                let kv = (e.value * 1e8).round() as i64;
                let kd = (e.derivative * 1e8).round() as i64;
                kv == key_val && kd == key_deriv
            })
            .collect();

        println!("\nExpressions with same key:");
        for e in &same {
            println!(
                "  {} (value={:.4}, deriv={:.4}, complexity={})",
                e.expr.to_postfix(),
                e.value,
                e.derivative,
                e.expr.complexity()
            );
        }
    }
}

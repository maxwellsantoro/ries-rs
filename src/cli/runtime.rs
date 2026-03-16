//! Runtime helpers for CLI startup and special modes.

use crate::eval;
use crate::expr;
use crate::presets;
use crate::profile::{Profile, UserConstant, UserFunction};
use crate::pslq;

use super::{
    parse_symbol_names_from_cli, parse_symbol_weights_from_cli, parse_user_constant_from_cli,
    parse_user_function_from_cli, Args,
};

#[derive(Debug)]
pub struct CliExit {
    pub message: String,
    pub code: i32,
}

impl CliExit {
    fn usage(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 1,
        }
    }

    fn config(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code: 2,
        }
    }
}

fn evaluate_and_print(
    expr_str: &str,
    x: f64,
    constants: &[UserConstant],
    functions: &[UserFunction],
) -> Result<(), CliExit> {
    let expr = expr::Expression::parse(expr_str)
        .ok_or_else(|| CliExit::usage(format!("Error: Invalid expression '{}'", expr_str)))?;

    match eval::evaluate_with_constants_and_functions(&expr, x, constants, functions) {
        Ok(result) => {
            println!("Expression: {}", expr_str);
            println!("At x = {}", x);
            println!("Value = {:.15}", result.value);
            println!("Derivative = {:.15}", result.derivative);
            Ok(())
        }
        Err(e) => Err(CliExit::usage(format!(
            "Error: Error evaluating expression: {:?}",
            e
        ))),
    }
}

pub fn load_runtime_profile(args: &Args, profile_arg: Option<&str>) -> Result<Profile, CliExit> {
    let mut profile = if let Some(profile_path) = profile_arg {
        Profile::from_file(profile_path).map_err(|e| CliExit::config(e.to_string()))?
    } else {
        Profile::load_default()
    };

    if let Some(preset_name) = &args.preset {
        let preset = presets::Preset::from_str(preset_name).ok_or_else(|| {
            CliExit::usage(format!(
                "Error: Unknown preset '{}'. Use --list-presets to see available presets.",
                preset_name
            ))
        })?;
        profile = profile.merge(preset.to_profile());
    }

    for include_path in &args.include {
        let included =
            Profile::from_file(include_path).map_err(|e| CliExit::config(e.to_string()))?;
        profile = profile.merge(included);
    }

    for constant_spec in &args.user_constant {
        if let Err(e) = parse_user_constant_from_cli(&mut profile, constant_spec) {
            eprintln!(
                "Warning: Failed to parse user constant '{}': {}",
                constant_spec, e
            );
        }
    }

    for func_spec in &args.define {
        if let Err(e) = parse_user_function_from_cli(&mut profile, func_spec) {
            eprintln!(
                "Warning: Failed to parse user function '{}': {}",
                func_spec, e
            );
        }
    }

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

    Ok(profile)
}

pub fn handle_special_modes(
    args: &Args,
    resolved_target: Option<f64>,
    profile: &Profile,
) -> Result<bool, CliExit> {
    if let Some(expr_str) = &args.find_expression {
        let x = args.at.or(resolved_target).unwrap_or(1.0);
        evaluate_and_print(expr_str, x, &profile.constants, &profile.functions)?;
        return Ok(true);
    }

    if let Some(expr_str) = &args.eval_expression {
        let x = args.at.unwrap_or(1.0);
        evaluate_and_print(expr_str, x, &profile.constants, &profile.functions)?;
        return Ok(true);
    }

    if !args.pslq {
        return Ok(false);
    }

    let target =
        resolved_target.ok_or_else(|| CliExit::usage("Error: TARGET is required for PSLQ"))?;

    let config = pslq::PslqConfig {
        max_coefficient: args.pslq_max_coeff,
        max_iterations: 10000,
        tolerance: 1e-10,
        extended_constants: args.pslq_extended,
    };

    println!();
    println!("   PSLQ Integer Relation Detection");
    println!("   Target: {:.15}", target);
    println!("   Max coefficient: {}", config.max_coefficient);
    if config.extended_constants {
        println!("   Using extended constant set");
    }
    println!();

    if let Some((num, den)) = pslq::find_rational_approximation(target, config.max_coefficient) {
        let approx = num as f64 / den as f64;
        let error = (approx - target).abs();
        println!("   Rational approximation:");
        println!(
            "   {} / {} = {:.15}  (error: {:.2e})",
            num, den, approx, error
        );
        println!();
    }

    match pslq::find_integer_relation(target, &config) {
        Some(relation) => {
            println!("   Integer relation found:");
            println!("   {}", relation.format());
            println!("   Residual: {:.2e}", relation.residual);
            if relation.is_exact {
                println!("   (exact match)");
            }
        }
        None => {
            println!("   No integer relation found within coefficient bounds.");
            println!("   Try increasing --pslq-max-coeff or using --pslq-extended.");
        }
    }
    println!();

    Ok(true)
}

/// Map the CLI `-l/--level` value to generation complexity budgets.
///
/// This intentionally differs from the lighter-weight library helper in
/// `search::level_to_complexity`. The CLI favors richer match sets and keeps
/// the historical command-line tuning separate from programmatic APIs.
pub fn cli_level_to_complexity(level_value: f32) -> (u32, u32) {
    let base_lhs: f32 = 35.0;
    let base_rhs: f32 = 35.0;
    let level_factor = 10.0 * level_value;
    (
        (base_lhs + level_factor) as u32,
        (base_rhs + level_factor) as u32,
    )
}

//! User-defined functions for RIES
//!
//! Parse and evaluate user-defined functions specified via --define option.
//! Functions are defined as postfix expressions using existing symbols
//! plus stack operations: | (dup) and @ (swap).

use crate::symbol::{NumType, Symbol};

/// A user-defined function
#[derive(Clone, Debug)]
pub struct UserFunction {
    /// Weight (complexity) of this function
    pub weight: u16,
    /// Short name (single or few characters)
    pub name: String,
    /// Description (for display)
    pub description: String,
    /// The body of the function as a postfix expression
    /// Uses standard symbols plus special stack operations
    pub body: Vec<UdfOp>,
    /// Numeric type of result
    pub num_type: NumType,
}

/// Operations that can appear in a user-defined function
#[derive(Clone, Debug, PartialEq)]
pub enum UdfOp {
    /// A standard RIES symbol (constant or operator)
    Symbol(Symbol),
    /// Duplicate top of stack (|)
    Dup,
    /// Swap top two stack elements (@)
    Swap,
}

impl UserFunction {
    /// Parse a user-defined function from a definition string
    /// Format: "weight:name:description:formula"
    /// Example: "4:sinh:hyperbolic sine:E|r-2/"
    pub fn parse(spec: &str) -> Result<Self, String> {
        let parts: Vec<&str> = spec.split(':').collect();
        if parts.len() != 4 {
            return Err(format!(
                "Invalid --define format: expected 4 colon-separated parts, got {}",
                parts.len()
            ));
        }

        let weight: u16 = parts[0]
            .parse()
            .map_err(|_| format!("Invalid weight: {}", parts[0]))?;

        let name = parts[1].to_string();
        if name.is_empty() {
            return Err("Function name cannot be empty".to_string());
        }

        let description = parts[2].to_string();

        // Parse the formula (postfix expression)
        let body = parse_udf_formula(parts[3])?;

        // Determine the numeric type based on the operations used
        let num_type = infer_num_type(&body);

        Ok(UserFunction {
            weight,
            name,
            description,
            body,
            num_type,
        })
    }

    /// Get the stack effect of this function (pushed - popped)
    /// For a unary function, this should be 0 (pop 1, push 1)
    pub fn stack_effect(&self) -> i32 {
        calculate_stack_effect(&self.body)
    }
}

/// Parse a UDF formula string into a vector of operations
fn parse_udf_formula(formula: &str) -> Result<Vec<UdfOp>, String> {
    let mut ops = Vec::new();

    for ch in formula.chars() {
        match ch {
            '|' => ops.push(UdfOp::Dup),
            '@' => ops.push(UdfOp::Swap),
            _ => {
                // Try to parse as a standard symbol
                if let Some(sym) = Symbol::from_byte(ch as u8) {
                    ops.push(UdfOp::Symbol(sym));
                } else {
                    return Err(format!("Unknown symbol '{}' in function definition", ch));
                }
            }
        }
    }

    // Validate the stack effect
    let effect = calculate_stack_effect(&ops);
    if effect != 0 {
        return Err(format!(
            "Invalid function: stack effect is {} (should be 0 for a unary function)",
            effect
        ));
    }

    Ok(ops)
}

/// Calculate the net stack effect of a sequence of operations
fn calculate_stack_effect(ops: &[UdfOp]) -> i32 {
    let mut effect = 0;

    for op in ops {
        match op {
            UdfOp::Symbol(sym) => {
                // Use the symbol's Seft to determine stack effect
                let seft = sym.seft();
                match seft {
                    crate::symbol::Seft::A => {
                        // Constant: pushes 1, pops 0 → effect +1
                        effect += 1;
                    }
                    crate::symbol::Seft::B => {
                        // Unary: pushes 1, pops 1 → effect 0
                        // But net effect is 0 since we pop first
                        effect -= 1; // pop
                        effect += 1; // push
                    }
                    crate::symbol::Seft::C => {
                        // Binary: pushes 1, pops 2 → effect -1
                        effect -= 2; // pop 2
                        effect += 1; // push 1
                    }
                }
            }
            UdfOp::Dup => {
                // Dup: pops 1, pushes 2 → effect +1
                effect -= 1;
                effect += 2;
            }
            UdfOp::Swap => {
                // Swap: pops 2, pushes 2 → effect 0
                // No net change
            }
        }
    }

    effect
}

/// Infer the numeric type of a function based on its operations
fn infer_num_type(ops: &[UdfOp]) -> NumType {
    for op in ops {
        if let UdfOp::Symbol(sym) = op {
            // If any operation produces transcendental results, the function is transcendental
            // Use result_type with an empty arg_types to check
            let result = sym.result_type(&[]);
            if matches!(result, NumType::Transcendental) {
                return NumType::Transcendental;
            }
        }
    }

    // Default to transcendental for safety
    NumType::Transcendental
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sinh() {
        // sinh(x) = (e^x - e^-x) / 2
        // In postfix: E|r-2/ (exp, dup, recip, subtract, 2, divide)
        let udf = UserFunction::parse("4:sinh:hyperbolic sine:E|r-2/").unwrap();

        assert_eq!(udf.weight, 4);
        assert_eq!(udf.name, "sinh");
        assert_eq!(udf.description, "hyperbolic sine");
        assert_eq!(udf.stack_effect(), 0);
    }

    #[test]
    fn test_parse_xex() {
        // XeX(x) = x * e^x
        // In postfix: |E* (dup, exp, multiply)
        let udf = UserFunction::parse("4:XeX:x*exp(x):|E*").unwrap();

        assert_eq!(udf.weight, 4);
        assert_eq!(udf.name, "XeX");
        assert_eq!(udf.stack_effect(), 0);

        // Verify the body
        assert_eq!(udf.body.len(), 3);
        assert_eq!(udf.body[0], UdfOp::Dup);
        assert_eq!(udf.body[1], UdfOp::Symbol(Symbol::Exp));
        assert_eq!(udf.body[2], UdfOp::Symbol(Symbol::Mul));
    }

    #[test]
    fn test_parse_cosh() {
        // cosh(x) = (e^x + e^-x) / 2
        // In postfix: E|r+2/
        let udf = UserFunction::parse("4:cosh:hyperbolic cosine:E|r+2/").unwrap();

        assert_eq!(udf.stack_effect(), 0);
    }

    #[test]
    fn test_invalid_stack_effect() {
        // This should fail because it doesn't produce a valid unary function
        let result = UserFunction::parse("4:bad:bad function:12+");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("stack effect"));
    }

    #[test]
    fn test_unknown_symbol() {
        let result = UserFunction::parse("4:bad:bad function:xyz");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown symbol"));
    }
}

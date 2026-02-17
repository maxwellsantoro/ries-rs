//! Profile file support for RIES configuration
//!
//! Parse and load `.ries` profile files for custom configuration including
//! user-defined constants, user-defined functions, symbol names, and symbol weights.

use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

use crate::symbol::{NumType, Symbol};

// Re-export UserFunction for convenience
pub use crate::udf::UserFunction;

/// A user-defined constant
#[derive(Clone, Debug)]
pub struct UserConstant {
    /// Weight (complexity) of this constant
    pub weight: u16,
    /// Short name (single character)
    pub name: String,
    /// Description (for display)
    pub description: String,
    /// Numeric value
    pub value: f64,
    /// Numeric type classification
    pub num_type: NumType,
}

/// Parsed profile configuration
#[derive(Clone, Debug, Default)]
pub struct Profile {
    /// User-defined constants
    pub constants: Vec<UserConstant>,
    /// User-defined functions
    pub functions: Vec<UserFunction>,
    /// Custom symbol names (e.g., :p:π)
    pub symbol_names: HashMap<Symbol, String>,
    /// Custom symbol weights
    pub symbol_weights: HashMap<Symbol, u16>,
    /// Additional profile files to include
    pub includes: Vec<PathBuf>,
}

impl Profile {
    /// Create an empty profile
    pub fn new() -> Self {
        Self::default()
    }

    /// Load a profile from a file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, ProfileError> {
        let path = path.as_ref();
        let file =
            fs::File::open(path).map_err(|e| ProfileError::IoError(path.to_path_buf(), e))?;

        let mut profile = Profile::new();
        let reader = io::BufReader::new(file);

        for (line_num, line_result) in reader.lines().enumerate() {
            let line = line_result.map_err(|e| ProfileError::IoError(path.to_path_buf(), e))?;

            // Skip empty lines and comments
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Parse the line
            if let Err(e) = parse_profile_line(&mut profile, trimmed) {
                return Err(ProfileError::ParseError(
                    path.to_path_buf(),
                    line_num + 1,
                    e,
                ));
            }
        }

        Ok(profile)
    }

    /// Load the default profile chain (~/.ries_profile, ./.ries)
    pub fn load_default() -> Self {
        let mut profile = Profile::new();

        // Try to load from home directory
        if let Some(home) = dirs::home_dir() {
            let home_profile = home.join(".ries_profile");
            if home_profile.exists() {
                if let Ok(p) = Self::from_file(&home_profile) {
                    profile = profile.merge(p);
                }
            }
        }

        // Try to load from current directory
        let local_profile = PathBuf::from(".ries");
        if local_profile.exists() {
            if let Ok(p) = Self::from_file(&local_profile) {
                profile = profile.merge(p);
            }
        }

        profile
    }

    /// Load from explicit path (for -p option)
    pub fn load_from(path: Option<&Path>) -> Self {
        if let Some(p) = path {
            Self::from_file(p).unwrap_or_default()
        } else {
            Self::load_default()
        }
    }

    /// Merge another profile into this one (other takes precedence)
    pub fn merge(mut self, other: Profile) -> Self {
        // Merge constants (append, later ones override by name)
        for c in other.constants {
            // Remove existing constant with same name
            self.constants.retain(|existing| existing.name != c.name);
            self.constants.push(c);
        }

        // Merge functions (append, later ones override by name)
        for f in other.functions {
            // Remove existing function with same name
            self.functions.retain(|existing| existing.name != f.name);
            self.functions.push(f);
        }

        // Merge symbol names
        self.symbol_names.extend(other.symbol_names);

        // Merge symbol weights
        self.symbol_weights.extend(other.symbol_weights);

        // Merge includes
        self.includes.extend(other.includes);

        self
    }
}

/// Parse a single profile line
fn parse_profile_line(profile: &mut Profile, line: &str) -> Result<(), String> {
    // Handle -X (user constant) lines
    if line.starts_with("-X") {
        return parse_user_constant(profile, line);
    }

    // Handle --define (user function) lines
    if line.starts_with("--define") {
        return parse_user_function(profile, line);
    }

    // Handle --symbol-names
    if line.starts_with("--symbol-names") {
        return parse_symbol_names(profile, line);
    }

    // Handle --symbol-weights
    if line.starts_with("--symbol-weights") {
        return parse_symbol_weights(profile, line);
    }

    // Handle --include
    if line.starts_with("--include") {
        return parse_include(profile, line);
    }

    // Unknown directive - could be a comment or unsupported option
    // For now, just ignore silently
    Ok(())
}

/// Parse a user constant definition
/// Format: -X "weight:name:description:value"
fn parse_user_constant(profile: &mut Profile, line: &str) -> Result<(), String> {
    // Extract the quoted part
    let rest = line[2..].trim();

    // Handle both quoted and unquoted formats
    let content = if rest.starts_with('"') {
        // Quoted format: -X "weight:name:description:value"
        let end_quote = rest[1..]
            .find('"')
            .ok_or("Unclosed quote in -X directive")?;
        &rest[1..end_quote + 1]
    } else {
        // Unquoted format: -X weight:name:description:value
        rest
    };

    let parts: Vec<&str> = content.split(':').collect();
    if parts.len() != 4 {
        return Err(format!(
            "Invalid -X format: expected 4 colon-separated parts, got {}",
            parts.len()
        ));
    }

    let weight: u16 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid weight: {}", parts[0]))?;

    let name = parts[1].to_string();
    let description = parts[2].to_string();

    let value: f64 = parts[3]
        .parse()
        .map_err(|_| format!("Invalid value: {}", parts[3]))?;

    // Determine numeric type based on value characteristics
    let num_type = if value.fract() == 0.0 && value.abs() < 1e10 {
        NumType::Integer
    } else if is_rational(value) {
        NumType::Rational
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

/// Check if a value is likely rational (simple fraction)
fn is_rational(v: f64) -> bool {
    if !v.is_finite() || v == 0.0 {
        return true;
    }

    // Check common denominators up to 100
    for denom in 1..=100_u32 {
        let numer = v * denom as f64;
        if (numer.round() - numer).abs() < 1e-10 {
            return true;
        }
    }
    false
}

/// Parse a user function definition
/// Format: --define "weight:name:description:formula"
fn parse_user_function(profile: &mut Profile, line: &str) -> Result<(), String> {
    // Extract the quoted part
    let rest = line["--define".len()..].trim();

    // Handle both quoted and unquoted formats
    let content = if rest.starts_with('"') {
        // Quoted format: --define "weight:name:description:formula"
        let end_quote = rest[1..]
            .find('"')
            .ok_or("Unclosed quote in --define directive")?;
        &rest[1..end_quote + 1]
    } else {
        // Unquoted format: --define weight:name:description:formula
        rest
    };

    // Parse the function using UserFunction::parse
    let udf = UserFunction::parse(content)?;
    profile.functions.push(udf);

    Ok(())
}

/// Parse symbol names directive
/// Format: --symbol-names :p:π :e:ℯ :f:φ
fn parse_symbol_names(profile: &mut Profile, line: &str) -> Result<(), String> {
    let rest = line["--symbol-names".len()..].trim();

    for part in rest.split_whitespace() {
        if !part.starts_with(':') {
            continue;
        }

        let inner = &part[1..];
        if let Some(colon_pos) = inner.find(':') {
            let symbol_char = inner[..colon_pos]
                .chars()
                .next()
                .ok_or("Empty symbol in --symbol-names")?;
            let name = inner[colon_pos + 1..].to_string();

            if let Some(symbol) = Symbol::from_byte(symbol_char as u8) {
                profile.symbol_names.insert(symbol, name);
            }
        }
    }

    Ok(())
}

/// Parse symbol weights directive
/// Format: --symbol-weights :W:20 :p:25
fn parse_symbol_weights(profile: &mut Profile, line: &str) -> Result<(), String> {
    let rest = line["--symbol-weights".len()..].trim();

    for part in rest.split_whitespace() {
        if !part.starts_with(':') {
            continue;
        }

        let inner = &part[1..];
        if let Some(colon_pos) = inner.find(':') {
            let symbol_char = inner[..colon_pos]
                .chars()
                .next()
                .ok_or("Empty symbol in --symbol-weights")?;
            let weight: u16 = inner[colon_pos + 1..]
                .parse()
                .map_err(|_| format!("Invalid weight in --symbol-weights: {}", inner))?;

            if let Some(symbol) = Symbol::from_byte(symbol_char as u8) {
                profile.symbol_weights.insert(symbol, weight);
            }
        }
    }

    Ok(())
}

/// Parse include directive
/// Format: --include /path/to/profile.ries
fn parse_include(profile: &mut Profile, line: &str) -> Result<(), String> {
    let rest = line["--include".len()..].trim();

    // Remove quotes if present
    let path_str = if rest.starts_with('"') && rest.ends_with('"') {
        &rest[1..rest.len() - 1]
    } else {
        rest
    };

    profile.includes.push(PathBuf::from(path_str));
    Ok(())
}

/// Errors that can occur during profile loading
#[derive(Debug)]
pub enum ProfileError {
    /// I/O error reading file
    IoError(PathBuf, io::Error),
    /// Parse error at specific line
    ParseError(PathBuf, usize, String),
}

impl std::fmt::Display for ProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProfileError::IoError(path, e) => {
                write!(f, "Error reading {}: {}", path.display(), e)
            }
            ProfileError::ParseError(path, line, msg) => {
                write!(
                    f,
                    "Parse error in {} at line {}: {}",
                    path.display(),
                    line,
                    msg
                )
            }
        }
    }
}

impl std::error::Error for ProfileError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_user_constant() {
        let mut profile = Profile::new();
        parse_user_constant(
            &mut profile,
            r#"-X "4:gamma:Euler's constant:0.5772156649""#,
        )
        .unwrap();

        assert_eq!(profile.constants.len(), 1);
        assert_eq!(profile.constants[0].name, "gamma");
        assert_eq!(profile.constants[0].weight, 4);
        assert!((profile.constants[0].value - 0.5772156649).abs() < 1e-10);
    }

    #[test]
    fn test_parse_symbol_names() {
        let mut profile = Profile::new();
        parse_symbol_names(&mut profile, "--symbol-names :p:π :e:ℯ").unwrap();

        assert_eq!(
            profile.symbol_names.get(&Symbol::Pi),
            Some(&"π".to_string())
        );
        assert_eq!(profile.symbol_names.get(&Symbol::E), Some(&"ℯ".to_string()));
    }

    #[test]
    fn test_parse_symbol_weights() {
        let mut profile = Profile::new();
        parse_symbol_weights(&mut profile, "--symbol-weights :W:20 :p:25").unwrap();

        assert_eq!(profile.symbol_weights.get(&Symbol::LambertW), Some(&20));
        assert_eq!(profile.symbol_weights.get(&Symbol::Pi), Some(&25));
    }

    #[test]
    fn test_profile_merge() {
        let mut p1 = Profile::new();
        p1.constants.push(UserConstant {
            weight: 4,
            name: "a".to_string(),
            description: "First".to_string(),
            value: 1.0,
            num_type: NumType::Integer,
        });

        let mut p2 = Profile::new();
        p2.constants.push(UserConstant {
            weight: 5,
            name: "b".to_string(),
            description: "Second".to_string(),
            value: 2.0,
            num_type: NumType::Integer,
        });
        p2.symbol_names.insert(Symbol::Pi, "π".to_string());

        let merged = p1.merge(p2);

        assert_eq!(merged.constants.len(), 2);
        assert_eq!(merged.symbol_names.len(), 1);
    }
}

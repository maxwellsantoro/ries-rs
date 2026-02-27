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
    ///
    /// This field is part of the public API and is used when generating expressions
    /// that include user-defined constants.
    #[allow(dead_code)]
    pub weight: u32,
    /// Short name (single character)
    pub name: String,
    /// Description (for display)
    ///
    /// This field is part of the public API for documentation and display purposes.
    #[allow(dead_code)]
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
    pub symbol_weights: HashMap<Symbol, u32>,
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
        load_profile_recursive(path.as_ref(), &mut Vec::new(), 0)
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

    /// Add a validated user constant to this profile.
    ///
    /// This method centralizes validation logic to ensure consistent
    /// handling of user constants across CLI and profile file parsing.
    ///
    /// # Arguments
    ///
    /// * `weight` - Complexity weight for this constant
    /// * `name` - Short name (single character preferred)
    /// * `description` - Human-readable description
    /// * `value` - Numeric value
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// * `name` is empty
    /// * `value` is not finite (NaN or infinity)
    pub fn add_constant(
        &mut self,
        weight: u32,
        name: String,
        description: String,
        value: f64,
    ) -> Result<(), ProfileError> {
        // Validate name
        if name.is_empty() {
            return Err(ProfileError::ValidationError(
                "Constant name cannot be empty".to_string(),
            ));
        }

        // Validate value is finite
        if !value.is_finite() {
            return Err(ProfileError::ValidationError(format!(
                "Constant value must be finite (got {})",
                value
            )));
        }

        // Determine numeric type based on value characteristics
        let num_type = if value.fract() == 0.0 && value.abs() < 1e10 {
            NumType::Integer
        } else if is_rational(value) {
            NumType::Rational
        } else {
            NumType::Transcendental
        };

        self.constants.push(UserConstant {
            weight,
            name,
            description,
            value,
            num_type,
        });

        Ok(())
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

const MAX_INCLUDE_DEPTH: usize = 25;

fn load_profile_recursive(
    path: &Path,
    include_stack: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<Profile, ProfileError> {
    if depth > MAX_INCLUDE_DEPTH {
        return Err(ProfileError::ParseError(
            path.to_path_buf(),
            0,
            format!("Profile include depth exceeded {}", MAX_INCLUDE_DEPTH),
        ));
    }

    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if include_stack.contains(&canonical) {
        return Err(ProfileError::ParseError(
            path.to_path_buf(),
            0,
            "Recursive --include detected".to_string(),
        ));
    }
    include_stack.push(canonical);

    let file = fs::File::open(path).map_err(|e| ProfileError::IoError(path.to_path_buf(), e))?;
    let mut profile = Profile::new();
    let reader = io::BufReader::new(file);

    for (line_num, line_result) in reader.lines().enumerate() {
        let line = line_result.map_err(|e| ProfileError::IoError(path.to_path_buf(), e))?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("--include") {
            let include_raw = parse_include_path(trimmed)
                .map_err(|e| ProfileError::ParseError(path.to_path_buf(), line_num + 1, e))?;

            let include_resolved = resolve_include_path(path, &include_raw).ok_or_else(|| {
                ProfileError::ParseError(
                    path.to_path_buf(),
                    line_num + 1,
                    format!(
                        "Could not open '{}' or '{}.ries' for reading",
                        include_raw.display(),
                        include_raw.display()
                    ),
                )
            })?;

            profile.includes.push(include_resolved.clone());
            let nested = load_profile_recursive(&include_resolved, include_stack, depth + 1)?;
            profile = profile.merge(nested);
            continue;
        }

        if let Err(e) = parse_profile_line(&mut profile, trimmed) {
            return Err(ProfileError::ParseError(
                path.to_path_buf(),
                line_num + 1,
                e,
            ));
        }
    }

    include_stack.pop();
    Ok(profile)
}

fn resolve_include_path(current_file: &Path, include_path: &Path) -> Option<PathBuf> {
    let mut candidates = Vec::new();

    if include_path.is_absolute() {
        candidates.push(include_path.to_path_buf());
    } else {
        let base = current_file.parent().unwrap_or_else(|| Path::new("."));
        candidates.push(base.join(include_path));
    }

    let mut with_suffix = include_path.as_os_str().to_os_string();
    with_suffix.push(".ries");
    if include_path.is_absolute() {
        candidates.push(PathBuf::from(with_suffix));
    } else {
        let base = current_file.parent().unwrap_or_else(|| Path::new("."));
        candidates.push(base.join(PathBuf::from(with_suffix)));
    }

    candidates.into_iter().find(|p| p.exists())
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
    let content = if let Some(stripped) = rest.strip_prefix('"') {
        // Quoted format: -X "weight:name:description:value"
        let end_quote = stripped.find('"').ok_or("Unclosed quote in -X directive")?;
        &stripped[..end_quote]
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

    let weight: u32 = parts[0]
        .parse()
        .map_err(|_| format!("Invalid weight: {}", parts[0]))?;

    let name = parts[1].to_string();
    let description = parts[2].to_string();

    let value: f64 = parts[3]
        .parse()
        .map_err(|_| format!("Invalid value: {}", parts[3]))?;

    // Use Profile's centralized validation
    profile
        .add_constant(weight, name, description, value)
        .map_err(|e| e.to_string())?;

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
    let content = if let Some(stripped) = rest.strip_prefix('"') {
        // Quoted format: --define "weight:name:description:formula"
        let end_quote = stripped
            .find('"')
            .ok_or("Unclosed quote in --define directive")?;
        &stripped[..end_quote]
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
            let weight: u32 = inner[colon_pos + 1..]
                .parse()
                .map_err(|_| format!("Invalid weight in --symbol-weights: {}", inner))?;

            if let Some(symbol) = Symbol::from_byte(symbol_char as u8) {
                profile.symbol_weights.insert(symbol, weight);
            }
        }
    }

    Ok(())
}

/// Parse include directive path.
/// Format: --include /path/to/profile.ries
fn parse_include_path(line: &str) -> Result<PathBuf, String> {
    let rest = line["--include".len()..].trim();

    if rest.is_empty() {
        return Err("--include requires a filename".to_string());
    }

    // Remove quotes if present
    let path_str = if rest.starts_with('"') && rest.ends_with('"') {
        &rest[1..rest.len() - 1]
    } else {
        rest
    };

    Ok(PathBuf::from(path_str))
}

/// Errors that can occur during profile loading
#[derive(Debug)]
pub enum ProfileError {
    /// I/O error reading file
    IoError(PathBuf, io::Error),
    /// Parse error at specific line
    ParseError(PathBuf, usize, String),
    /// Validation error (e.g., invalid constant value)
    ValidationError(String),
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
            ProfileError::ValidationError(msg) => {
                write!(f, "Validation error: {}", msg)
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

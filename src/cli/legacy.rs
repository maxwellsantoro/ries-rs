//! Legacy argument handling for backward compatibility
//!
//! Handles quirky legacy behaviors from original RIES:
//! - `-p 2.5` means target 2.5, not profile "2.5"
//! - `-l 2.5` means liouvillian + target 2.5
//! - `-E 2.5` means enable-all + target 2.5

/// Normalized arguments after legacy handling
#[derive(Debug, Clone)]
pub struct NormalizedArgs {
    /// The resolved target value (may come from -p, -l, or -E if they look numeric)
    pub target: Option<f64>,
    /// The profile path (None if -p was interpreted as target)
    pub profile: Option<String>,
    /// The enable string (may be "all" if -E was interpreted as target)
    pub enable: Option<String>,
    /// The search level (2.0 if -l was interpreted as target)
    pub level: f32,
    /// Whether liouvillian mode should be enabled (true if -l was interpreted as target)
    pub liouvillian: bool,
}

/// Normalize legacy argument semantics
///
/// This function handles the quirky legacy behaviors from original RIES:
///
/// # -p legacy behavior
/// If `-p` looks like a number and no explicit target was provided, treat it as the target.
/// Example: `ries -p 2.5` means "use default profile and search for 2.5"
///
/// # -E legacy behavior
/// If `-E` looks like a number and no explicit target was provided, treat it as the target
/// and set enable to "all".
/// Example: `ries -E 2.5` means "enable all and search for 2.5"
///
/// # -l legacy behavior
/// If `-l` looks like a float (has decimal point) and no explicit target was provided,
/// treat it as the target and enable liouvillian mode with level 2.0.
/// Example: `ries -l 2.5` means "liouvillian mode with level 2.0 and search for 2.5"
///
/// # Arguments
/// * `profile_arg` - The value passed to `-p/--profile` flag
/// * `enable_arg` - The value passed to `-E/--enable` flag
/// * `level_arg` - The value passed to `-l/--level` flag
/// * `explicit_target` - The positional target argument (if provided)
///
/// # Returns
/// A `NormalizedArgs` struct with the resolved values
#[must_use]
pub fn normalize_legacy_args(
    profile_arg: Option<&str>,
    enable_arg: Option<&str>,
    level_arg: &str,
    explicit_target: Option<f64>,
) -> NormalizedArgs {
    // Handle -p legacy semantics: if profile looks like a number and no target, treat as target
    // Original ries behavior: "ries -p 2.5" means "use default profile and search for 2.5"
    let (profile, resolved_target) = if let Some(profile_path) = profile_arg {
        if explicit_target.is_none() {
            // Check if profile argument looks like a target (numeric)
            if let Ok(val) = profile_path.parse::<f64>() {
                // It's a number, treat as target and use default profile
                (None, Some(val))
            } else {
                // Not a number, use as profile path
                (Some(profile_path.to_string()), explicit_target)
            }
        } else {
            // Both -p and target provided, use both normally
            (Some(profile_path.to_string()), explicit_target)
        }
    } else {
        (None, explicit_target)
    };

    // Handle -E legacy semantics: if enable looks like a number and no target, treat as target
    // Original ries behavior: "ries -E 2.5" means "enable all and search for 2.5"
    let (enable, resolved_target) = if let Some(enable_str) = enable_arg {
        if resolved_target.is_none() {
            // Check if enable argument looks like a target (numeric)
            if let Ok(val) = enable_str.parse::<f64>() {
                // It's a number, treat as target and use "all" for enable
                (Some("all".to_string()), Some(val))
            } else {
                // Not a number, use as enable string
                (Some(enable_str.to_string()), resolved_target)
            }
        } else {
            // Both -E and target provided, use both normally
            (Some(enable_str.to_string()), resolved_target)
        }
    } else {
        (None, resolved_target)
    };

    // Handle -l legacy semantics: if level looks like a float and no target, treat as target + liouvillian
    // Original ries: "-l 2.5" means liouvillian mode + target 2.5
    // "-l3" or "--level 3" with an explicit target means level 3
    let (level, liouvillian, final_target) = if resolved_target.is_some() {
        // Target was explicitly provided, use -l as level
        let level = level_arg.parse::<f32>().unwrap_or(2.0);
        (level, false, resolved_target)
    } else {
        // No explicit target - check if "level" looks like a target (has decimal point)
        if level_arg.contains('.') {
            // Legacy: -l 2.5 means liouvillian + target 2.5
            if let Ok(target_val) = level_arg.parse::<f64>() {
                (2.0, true, Some(target_val))
            } else {
                // Parse error, let it fail later with proper error
                let level = level_arg.parse::<f32>().unwrap_or(2.0);
                (level, false, None)
            }
        } else {
            // It's an integer level, but no target - still an error later
            let level = level_arg.parse::<f32>().unwrap_or(2.0);
            (level, false, None)
        }
    };

    NormalizedArgs {
        target: final_target,
        profile,
        enable,
        level,
        liouvillian,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_profile_as_target() {
        // -p 2.5 without explicit target -> target=2.5, profile=None
        let result = normalize_legacy_args(Some("2.5"), None, "2", None);
        assert_eq!(result.target, Some(2.5));
        assert!(result.profile.is_none());
    }

    #[test]
    fn test_profile_with_explicit_target() {
        // -p myprofile 2.71 -> target=2.71, profile=myprofile
        let result = normalize_legacy_args(Some("myprofile"), None, "2", Some(2.71));
        assert_eq!(result.target, Some(2.71));
        assert_eq!(result.profile, Some("myprofile".to_string()));
    }

    #[test]
    fn test_profile_non_numeric() {
        // -p myprofile without target -> target=None, profile=myprofile
        let result = normalize_legacy_args(Some("myprofile"), None, "2", None);
        assert!(result.target.is_none());
        assert_eq!(result.profile, Some("myprofile".to_string()));
    }

    #[test]
    fn test_enable_as_target() {
        // -E 2.5 without explicit target -> target=2.5, enable="all"
        let result = normalize_legacy_args(None, Some("2.5"), "2", None);
        assert_eq!(result.target, Some(2.5));
        assert_eq!(result.enable, Some("all".to_string()));
    }

    #[test]
    fn test_enable_with_explicit_target() {
        // -E abc 2.71 -> target=2.71, enable="abc"
        let result = normalize_legacy_args(None, Some("abc"), "2", Some(2.71));
        assert_eq!(result.target, Some(2.71));
        assert_eq!(result.enable, Some("abc".to_string()));
    }

    #[test]
    fn test_level_as_target() {
        // -l 2.5 without explicit target -> target=2.5, liouvillian=true, level=2.0
        let result = normalize_legacy_args(None, None, "2.5", None);
        assert_eq!(result.target, Some(2.5));
        assert!(result.liouvillian);
        assert!((result.level - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_level_integer_no_target() {
        // -l 3 without explicit target -> target=None, level=3.0, liouvillian=false
        let result = normalize_legacy_args(None, None, "3", None);
        assert!(result.target.is_none());
        assert!((result.level - 3.0).abs() < f32::EPSILON);
        assert!(!result.liouvillian);
    }

    #[test]
    fn test_level_with_explicit_target() {
        // -l 3 with target 2.5 -> target=2.5, level=3.0, liouvillian=false
        let result = normalize_legacy_args(None, None, "3", Some(2.5));
        assert_eq!(result.target, Some(2.5));
        assert!((result.level - 3.0).abs() < f32::EPSILON);
        assert!(!result.liouvillian);
    }

    #[test]
    fn test_combined_legacy_args() {
        // -p 1.0 sets target=1.0 (first wins), subsequent -E and -l don't override
        // This matches the original behavior where the first numeric arg sets the target
        let result = normalize_legacy_args(Some("1.0"), Some("2.0"), "3.0", None);
        assert_eq!(result.target, Some(1.0));
        assert!(!result.liouvillian); // No liouvillian since target was set by -p
        assert_eq!(result.enable, Some("2.0".to_string())); // -E 2.0 treated as enable since target already set
        assert!(result.profile.is_none()); // -p 1.0 was numeric, so no profile
    }

    #[test]
    fn test_explicit_target_overrides_all() {
        // Explicit target should prevent all legacy interpretations
        let result = normalize_legacy_args(Some("1.0"), Some("2.0"), "3.0", Some(4.0));
        assert_eq!(result.target, Some(4.0));
        assert!(!result.liouvillian);
        // -E 2.0 is still treated as enable="all" since resolved_target was set by -p
    }
}

//! Diagnostics handling for the `-D` flag
//!
//! This module provides diagnostic output options compatible with the original
//! RIES C implementation's `-D` flag.

/// Diagnostic output options parsed from the `-D` flag.
#[derive(Debug, Default, Clone)]
pub struct DiagnosticOptions {
    /// Show work details (s, N channels)
    pub show_work: bool,
    /// Show statistics (y, M channels)
    pub show_stats: bool,
    /// Show match checks (o channel)
    pub show_match_checks: bool,
    /// Show pruned arithmetic (A, a channels)
    pub show_pruned_arith: bool,
    /// Show pruned range (B, b channels)
    pub show_pruned_range: bool,
    /// Show database adds (G, g channels)
    pub show_db_adds: bool,
    /// Show Newton-Raphson iterations (n channel)
    pub show_newton: bool,
    /// Unsupported channels that were requested but not implemented
    pub unsupported_channels: Vec<char>,
}

/// Parse diagnostic options from the `-D` flag argument.
///
/// The `-D` flag accepts a string of channel characters, each enabling
/// a specific diagnostic output. This is compatible with the original
/// RIES C implementation.
///
/// # Arguments
///
/// * `diagnostics` - Optional string from the `-D` flag
/// * `show_work_flag` - Whether `--show-work` was specified
/// * `show_stats_flag` - Whether `--stats` was specified
///
/// # Returns
///
/// A `DiagnosticOptions` struct with the parsed options.
pub fn parse_diagnostics(
    diagnostics: Option<&str>,
    show_work_flag: bool,
    show_stats_flag: bool,
) -> DiagnosticOptions {
    let mut opts = DiagnosticOptions {
        show_work: show_work_flag,
        show_stats: show_stats_flag,
        show_match_checks: false,
        show_pruned_arith: false,
        show_pruned_range: false,
        show_db_adds: false,
        show_newton: false,
        unsupported_channels: Vec::new(),
    };

    if let Some(spec) = diagnostics {
        // Channels recognized for compatibility but currently no-op
        const COMPAT_NOOP_CHANNELS: &str = "CcDdEeFfHhIiJjKkLlPpQqRrTtUuVvWwXxZz";
        for ch in spec.chars() {
            match ch {
                's' | 'N' => opts.show_work = true,
                'y' | 'M' => opts.show_stats = true,
                'o' => opts.show_match_checks = true,
                'A' | 'a' => opts.show_pruned_arith = true,
                'B' | 'b' => opts.show_pruned_range = true,
                'G' | 'g' => opts.show_db_adds = true,
                'n' => opts.show_newton = true,
                _ if COMPAT_NOOP_CHANNELS.contains(ch) => {
                    // Recognized for compatibility; currently no-op.
                }
                _ => opts.unsupported_channels.push(ch),
            }
        }
    }

    opts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diagnostics_empty() {
        let opts = parse_diagnostics(None, false, false);
        assert!(!opts.show_work);
        assert!(!opts.show_stats);
        assert!(opts.unsupported_channels.is_empty());
    }

    #[test]
    fn test_parse_diagnostics_show_work() {
        let opts = parse_diagnostics(Some("s"), false, false);
        assert!(opts.show_work);
        assert!(!opts.show_stats);
    }

    #[test]
    fn test_parse_diagnostics_show_stats() {
        let opts = parse_diagnostics(Some("y"), false, false);
        assert!(!opts.show_work);
        assert!(opts.show_stats);
    }

    #[test]
    fn test_parse_diagnostics_multiple_channels() {
        let opts = parse_diagnostics(Some("sny"), false, false);
        assert!(opts.show_work);
        assert!(opts.show_stats);
        assert!(opts.show_newton);
    }

    #[test]
    fn test_parse_diagnostics_flags_override() {
        let opts = parse_diagnostics(None, true, true);
        assert!(opts.show_work);
        assert!(opts.show_stats);
    }

    #[test]
    fn test_parse_diagnostics_unsupported_channel() {
        let opts = parse_diagnostics(Some("z9"), false, false);
        // 'z' is in COMPAT_NOOP_CHANNELS, so it's not unsupported
        // '9' is not recognized at all
        assert_eq!(opts.unsupported_channels, vec!['9']);
    }
}
//! Run manifest for reproducibility
//!
//! Provides structured output of search configuration and results
//! for academic reproducibility and verification.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Complete manifest of a search run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    /// Version of ries-rs
    pub version: String,
    /// Git commit hash (if available)
    pub git_hash: Option<String>,
    /// Timestamp of the run (ISO 8601)
    pub timestamp: String,
    /// Platform/OS info
    pub platform: PlatformInfo,
    /// Search configuration
    pub config: SearchConfigInfo,
    /// Top matches found
    pub results: Vec<MatchInfo>,
}

/// Platform information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    /// Operating system
    pub os: String,
    /// Architecture
    pub arch: String,
    /// Rust version used to compile
    pub rust_version: String,
}

/// Search configuration summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfigInfo {
    /// Target value searched for
    pub target: f64,
    /// Search level
    pub level: f32,
    /// Maximum LHS complexity
    pub max_lhs_complexity: u32,
    /// Maximum RHS complexity
    pub max_rhs_complexity: u32,
    /// Whether deterministic mode was enabled
    pub deterministic: bool,
    /// Whether parallel search was used
    pub parallel: bool,
    /// Maximum error tolerance
    pub max_error: f64,
    /// Maximum matches requested
    pub max_matches: usize,
    /// Ranking mode used
    pub ranking_mode: String,
    /// User constants (names and values)
    pub user_constants: Vec<UserConstantInfo>,
    /// Excluded symbols
    pub excluded_symbols: Vec<String>,
    /// Allowed symbols (if restricted)
    pub allowed_symbols: Option<Vec<String>>,
}

/// User constant information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserConstantInfo {
    /// Name of the constant
    pub name: String,
    /// Value of the constant
    pub value: f64,
    /// Description
    pub description: String,
}

/// Match result information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatchInfo {
    /// LHS expression (postfix)
    pub lhs_postfix: String,
    /// RHS expression (postfix)
    pub rhs_postfix: String,
    /// LHS expression (infix)
    pub lhs_infix: String,
    /// RHS expression (infix)
    pub rhs_infix: String,
    /// Error (absolute)
    pub error: f64,
    /// Whether this is an exact match
    pub is_exact: bool,
    /// Complexity score
    pub complexity: u32,
    /// X value that solves the equation
    pub x_value: f64,
    /// Stability score (0-1, higher is better)
    pub stability: Option<f64>,
}

impl RunManifest {
    /// Create a new manifest with current timestamp
    pub fn new(config: SearchConfigInfo, results: Vec<MatchInfo>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| {
                let secs = d.as_secs();
                // Convert to ISO 8601-like format
                chrono_like_timestamp(secs)
            })
            .unwrap_or_else(|_| "unknown".to_string());

        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            git_hash: get_git_hash(),
            timestamp,
            platform: PlatformInfo::current(),
            config,
            results,
        }
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Serialize to JSON with compact format
    pub fn to_json_compact(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

impl PlatformInfo {
    /// Get current platform info
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
            rust_version: rustc_version().unwrap_or_else(|| "unknown".to_string()),
        }
    }
}

/// Get git commit hash from build time
fn get_git_hash() -> Option<String> {
    // Try to get from environment variable set during build
    option_env!("GIT_HASH").map(|s| s.to_string()).or_else(|| {
        // Fallback: try to read from .git at runtime (for development)
        #[cfg(debug_assertions)]
        {
            std::process::Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
        }
        #[cfg(not(debug_assertions))]
        {
            None
        }
    })
}

/// Get rustc version
fn rustc_version() -> Option<String> {
    // In debug builds, try to get rustc version
    #[cfg(debug_assertions)]
    {
        std::process::Command::new("rustc")
            .arg("--version")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
    }
    #[cfg(not(debug_assertions))]
    {
        None
    }
}

/// Create an ISO 8601-like timestamp from unix seconds
fn chrono_like_timestamp(secs: u64) -> String {
    // Simple implementation without chrono dependency
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Unix epoch is 1970-01-01
    // Calculate year, month, day from days since epoch
    let (year, month, day) = days_to_ymd(days);

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day)
fn days_to_ymd(days: u64) -> (i32, u32, u32) {
    // Start from 1970-01-01
    let mut year = 1970_i32;
    let mut remaining_days = days as i64;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1_u32;
    for &days_in_month in &days_in_months {
        if remaining_days < days_in_month as i64 {
            break;
        }
        remaining_days -= days_in_month as i64;
        month += 1;
    }

    let day = (remaining_days + 1) as u32; // Days are 1-indexed
    (year, month, day)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_format() {
        // 2024-01-15 12:30:45 UTC = 1705318245 seconds since epoch
        let ts = chrono_like_timestamp(1705318245);
        assert!(ts.starts_with("2024-01-"));
        assert!(ts.ends_with("Z"));
    }

    #[test]
    fn test_leap_year() {
        assert!(is_leap_year(2024));
        assert!(!is_leap_year(2023));
        assert!(!is_leap_year(1900));
        assert!(is_leap_year(2000));
    }

    #[test]
    fn test_timestamp_leap_year_feb29() {
        // 2000-02-29 00:00:00 UTC = unix timestamp 951782400
        let ts = chrono_like_timestamp(951782400);
        assert!(ts.starts_with("2000-02-29"), "got: {}", ts);
    }

    #[test]
    fn test_timestamp_year_boundary() {
        // 1999-12-31 23:59:59 UTC = 946684799
        let ts = chrono_like_timestamp(946684799);
        assert!(ts.starts_with("1999-12-31"), "got: {}", ts);
        // 2000-01-01 00:00:00 UTC = 946684800
        let ts2 = chrono_like_timestamp(946684800);
        assert!(ts2.starts_with("2000-01-01"), "got: {}", ts2);
    }

    #[test]
    fn test_leap_year_century_rules() {
        // 1900: divisible by 100 but not 400 — NOT a leap year
        assert!(!is_leap_year(1900));
        // 2100: same
        assert!(!is_leap_year(2100));
        // 2000: divisible by 400 — IS a leap year
        assert!(is_leap_year(2000));
        // 2400: same
        assert!(is_leap_year(2400));
    }
}

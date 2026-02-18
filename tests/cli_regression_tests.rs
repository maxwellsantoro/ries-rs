use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn run_ries_raw(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ries-rs"))
        .args(args)
        .output()
        .expect("failed to run ries-rs")
}

fn run_ries(args: &[&str]) -> (String, String) {
    let output = run_ries_raw(args);

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    assert!(
        output.status.success(),
        "command failed\nargs: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        stdout,
        stderr
    );

    (stdout, stderr)
}

fn run_ries_owned(args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ries-rs"))
        .args(args)
        .output()
        .expect("failed to run ries-rs")
}

fn parse_stat_value(stdout: &str, key: &str) -> Option<usize> {
    stdout.lines().find_map(|line| {
        if !line.contains(key) {
            return None;
        }
        line.split_whitespace().last()?.parse::<usize>().ok()
    })
}

fn parse_generated_counts(stdout: &str) -> Option<(usize, usize)> {
    stdout.lines().find_map(|line| {
        if !line.starts_with("Generated ") {
            return None;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            return None;
        }
        let lhs = parts.get(1)?.parse::<usize>().ok()?;
        let rhs = parts.get(4)?.parse::<usize>().ok()?;
        Some((lhs, rhs))
    })
}

fn parse_first_complexity(stdout: &str) -> Option<u32> {
    stdout.lines().find_map(|line| {
        let start = line.rfind('{')?;
        let end = line.rfind('}')?;
        if end <= start + 1 {
            return None;
        }
        line[start + 1..end].trim().parse::<u32>().ok()
    })
}

fn parse_first_match_line(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find(|line| line.contains('=') && line.contains('{'))
        .map(|line| line.trim().to_string())
}

fn parse_match_lines(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| line.contains('=') && line.contains('{'))
        .map(|line| line.trim().to_string())
        .collect()
}

fn unique_tmp_path(stem: &str) -> std::path::PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "ries-rs-{}-{}-{}.ries",
        stem,
        std::process::id(),
        now
    ))
}

#[path = "cli/basics.rs"]
mod basics;
#[path = "cli/diagnostics.rs"]
mod diagnostics;
#[path = "cli/legacy.rs"]
mod legacy;
#[path = "cli/ranking.rs"]
mod ranking;

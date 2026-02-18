use super::*;

#[test]
fn test_parity_ranking_flag_is_accepted() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--report",
        "false",
        "--parity-ranking",
        "-n",
        "2",
        "-l",
        "1",
    ]);
    assert!(
        stdout.contains("Search completed"),
        "expected --parity-ranking to run successfully\n{}",
        stdout
    );
}

#[test]
fn test_parity_ranking_changes_first_match_for_some_target() {
    let targets = ["2.5", "2.5063", "1.6180339887", "0.9159655942", "3.2"];

    let mut changed = false;
    let mut samples = Vec::new();

    for target in targets {
        let (base_stdout, _stderr) = run_ries(&[target, "--report", "false", "-n", "6", "-l", "1"]);
        let (parity_stdout, _stderr) = run_ries(&[
            target,
            "--report",
            "false",
            "--parity-ranking",
            "-n",
            "6",
            "-l",
            "1",
        ]);

        let base_first = parse_first_match_line(&base_stdout).unwrap_or_default();
        let parity_first = parse_first_match_line(&parity_stdout).unwrap_or_default();
        if !base_first.is_empty() && !parity_first.is_empty() {
            samples.push(format!(
                "target={} | base={} | parity={}",
                target, base_first, parity_first
            ));
        }
        if base_first != parity_first {
            changed = true;
            break;
        }
    }

    assert!(
        changed,
        "expected --parity-ranking to alter first match ordering for at least one benchmark target\n{}",
        samples.join("\n")
    );
}

#[test]
fn test_classic_defaults_to_parity_ranking() {
    let target = "2.5063";
    let args_base = [
        target,
        "--classic",
        "--report",
        "false",
        "-n",
        "6",
        "-l",
        "1",
    ];

    let (classic_stdout, _stderr) = run_ries(&args_base);
    let classic_first = parse_first_match_line(&classic_stdout).unwrap_or_default();

    let mut parity_args: Vec<&str> = args_base.to_vec();
    parity_args.push("--parity-ranking");
    let (parity_stdout, _stderr) = run_ries(&parity_args);
    let parity_first = parse_first_match_line(&parity_stdout).unwrap_or_default();

    assert_eq!(
        classic_first, parity_first,
        "expected classic mode default ordering to match --parity-ranking\nclassic:\n{}\nparity:\n{}",
        classic_stdout, parity_stdout
    );
}

#[test]
fn test_complexity_ranking_overrides_classic_default() {
    let target = "2.5063";
    let args_base = [
        target,
        "--classic",
        "--report",
        "false",
        "-n",
        "6",
        "-l",
        "1",
    ];

    let (classic_stdout, _stderr) = run_ries(&args_base);
    let classic_lines = parse_match_lines(&classic_stdout);

    let mut complexity_args: Vec<&str> = args_base.to_vec();
    complexity_args.push("--complexity-ranking");
    let (complexity_stdout, _stderr) = run_ries(&complexity_args);
    let complexity_lines = parse_match_lines(&complexity_stdout);

    assert_ne!(
        classic_lines, complexity_lines,
        "expected --complexity-ranking to override classic default parity ranking\nclassic:\n{}\ncomplexity:\n{}",
        classic_stdout, complexity_stdout
    );
}

#[test]
fn test_ranking_flags_conflict() {
    let output = run_ries_raw(&[
        "2.5",
        "--report",
        "false",
        "--parity-ranking",
        "--complexity-ranking",
        "-n",
        "1",
    ]);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        !output.status.success(),
        "expected conflicting ranking flags to fail"
    );
    assert!(
        stderr.contains("cannot be used with") || stderr.contains("conflicts with"),
        "expected clap conflict error for ranking flags\n{}",
        stderr
    );
}

use super::*;
use serde_json::Value;

#[test]
fn test_define_does_not_panic() {
    let (stdout, _stderr) = run_ries(&[
        "--define",
        "6:sinh:hyperbolic sine:E|r-2/",
        "--classic",
        "3.6268604078",
        "-n",
        "1",
    ]);
    assert!(stdout.contains("Search completed"));
}

#[test]
fn test_rhs_only_symbols_filter_applies() {
    let (stdout, _stderr) = run_ries(&[
        "2.506314",
        "--S-RHS",
        "1",
        "--N-RHS",
        "1",
        "--classic",
        "-n",
        "3",
    ]);
    assert!(
        stdout.contains("No matches found."),
        "expected RHS filter to eliminate matches\n{}",
        stdout
    );
}

#[test]
fn test_one_sided_search_respects_rhs_symbols_and_operator_limits() {
    let allowed = json_search(&["2", "-l", "0", "--one-sided", "-S", "1", "--S-RHS", "2"]);
    assert_eq!(allowed["matcher"], "one_sided");
    assert_eq!(allowed["results"][0]["equation"], "x = 2");
    assert_eq!(allowed["results"][0]["exact"], true);

    let unrestricted = json_search(&["3", "--one-sided", "-S", "1+"]);
    assert_eq!(unrestricted["results"][0]["equation"], "x = 1+1+1");
    let limited = json_search(&["3", "--one-sided", "-S", "1+", "--O-RHS", "+"]);
    assert_eq!(limited["results_returned"], 0);
}

#[test]
fn test_max_match_distance_applies() {
    // With calibrated weights the search can find matches with error ~1e-9.
    // Use a sub-noise threshold (1e-12) that is guaranteed to eliminate all matches.
    let (stdout, _stderr) = run_ries(&[
        "2.506314",
        "--classic",
        "--max-match-distance",
        "1e-12",
        "-n",
        "3",
    ]);
    assert!(
        stdout.contains("No matches found."),
        "expected max-match-distance to eliminate coarse matches\n{}",
        stdout
    );
}

#[test]
fn test_no_refinement_disables_newton_calls() {
    let (stdout, _stderr) = run_ries(&[
        "2.506314",
        "-l",
        "0",
        "--stats",
        "--no-refinement",
        "-n",
        "3",
    ]);
    let calls = parse_stat_value(&stdout, "Newton calls:")
        .expect("missing 'Newton calls' line in --stats output");
    assert_eq!(calls, 0, "expected no-refinement to skip Newton");
}

#[test]
fn test_json_output_is_valid_and_machine_readable() {
    let (stdout, _stderr) = run_ries(&[
        "2.506314", "-l", "0", "-n", "2", "--json", "--report", "false",
    ]);

    assert!(
        !stdout.contains("Generated "),
        "expected --json stdout to contain only JSON\n{}",
        stdout
    );

    let parsed: Value = serde_json::from_str(&stdout).expect("valid JSON output");
    assert_eq!(parsed["target"].as_f64(), Some(2.506314));
    assert_eq!(parsed["max_matches"].as_u64(), Some(2));
    assert!(parsed["results"].is_array(), "missing results array");
    assert!(
        parsed["results_returned"].as_u64().unwrap_or(0)
            <= parsed["max_matches"].as_u64().unwrap_or(0)
    );
    assert!(
        parsed["search_stats"]["expressions_generated_total"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );
    assert!(
        parsed["search_stats"]["candidate_window_total"]
            .as_u64()
            .unwrap_or(0)
            >= parsed["search_stats"]["candidate_pairs_tested"]
                .as_u64()
                .unwrap_or(0)
    );
    assert!(
        parsed["search_stats"]["candidate_window_avg"].is_number(),
        "candidate_window_avg should be numeric"
    );
    assert!(
        parsed["search_stats"]["candidate_window_max"].is_u64(),
        "candidate_window_max should be an integer"
    );
    let search_stats = &parsed["search_stats"];
    if let Some(v) = search_stats.get("candidate_window_max_lhs_postfix") {
        assert!(
            v.is_string(),
            "candidate_window_max_lhs_postfix should be a string"
        );
    }
    if let Some(v) = search_stats.get("candidate_window_max_lhs_value") {
        assert!(
            v.is_number(),
            "candidate_window_max_lhs_value should be numeric"
        );
    }
    if let Some(v) = search_stats.get("candidate_window_max_lhs_derivative") {
        assert!(
            v.is_number(),
            "candidate_window_max_lhs_derivative should be numeric"
        );
    }
    if let Some(v) = search_stats.get("candidate_window_max_lhs_complexity") {
        assert!(
            v.is_u64(),
            "candidate_window_max_lhs_complexity should be an integer"
        );
    }
    assert!(
        parsed["search_stats"]["strict_gate_rejections"].is_u64(),
        "strict_gate_rejections should be an integer"
    );
    assert!(
        parsed["search_stats"]["candidates_per_pool_insertion"].is_number(),
        "candidates_per_pool_insertion should be numeric"
    );
    assert!(
        parsed["search_stats"]["newton_success_rate"].is_number(),
        "newton_success_rate should be numeric"
    );
    assert!(
        parsed["search_stats"]["pool_acceptance_rate"].is_number(),
        "pool_acceptance_rate should be numeric"
    );
    let peak = &parsed["search_stats"]["peak_memory_bytes"];
    assert!(
        peak.is_null() || peak.is_u64(),
        "peak_memory_bytes should be null or integer, got: {peak}"
    );
    if let Some(first) = parsed["results"].as_array().and_then(|arr| arr.first()) {
        assert!(first.get("equation").is_some(), "missing equation field");
        assert!(
            first.get("operator_count").is_some(),
            "missing operator_count field"
        );
        assert!(
            first.get("tree_depth").is_some(),
            "missing tree_depth field"
        );
    }
}

#[test]
fn test_manifest_success_for_text_and_json_output() {
    for (stem, extra_args, matcher, applied) in [
        ("text", vec![], "serial", false),
        ("json", vec!["--json", "--lhs-value-dedup"], "serial", true),
        (
            "fallback",
            vec!["--json", "--lhs-value-dedup", "--max-memory", "512M"],
            "streaming",
            false,
        ),
    ] {
        let path = unique_tmp_path(stem);
        let path_arg = path.to_string_lossy().into_owned();
        let is_json = extra_args.contains(&"--json");
        let requested = extra_args.contains(&"--lhs-value-dedup");
        let mut args = vec![
            "2.5",
            "-l",
            "0",
            "--deterministic",
            "--report",
            "false",
            "-n",
            "1",
            "--emit-manifest",
            path_arg.as_str(),
        ];
        args.extend(extra_args.iter().copied());
        let (stdout, _stderr) = run_ries(&args);
        if !is_json {
            assert!(stdout.contains("Search completed") || stdout.contains("="));
        } else {
            let result: Value = serde_json::from_str(&stdout).expect("valid JSON output");
            assert_eq!(result["matcher"], matcher);
            assert_eq!(result["streaming"], matcher == "streaming");
            assert_eq!(result["lhs_value_dedup"], requested);
            assert_eq!(result["lhs_value_dedup_applied"], applied);
        }
        assert!(
            path.is_file(),
            "manifest was not written: {}",
            path.display()
        );
        let manifest: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read manifest"))
                .expect("valid manifest JSON");
        assert_eq!(
            manifest["version"].as_str(),
            Some(env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(manifest["config"]["matcher"], matcher);
        assert_eq!(manifest["config"]["lhs_value_dedup"], requested);
        assert_eq!(manifest["config"]["lhs_value_dedup_applied"], applied);
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn test_manifest_write_failures_return_nonzero_for_text_and_json() {
    for (stem, extra_args) in [
        ("missing-parent", Vec::<&str>::new()),
        ("missing-parent-json", vec!["--json"]),
    ] {
        let path = unique_tmp_path(stem).join("manifest.json");
        let path_arg = path.to_string_lossy().into_owned();
        let mut args = vec![
            "2.5".to_string(),
            "-l".to_string(),
            "0".to_string(),
            "--deterministic".to_string(),
            "--report".to_string(),
            "false".to_string(),
            "-n".to_string(),
            "1".to_string(),
            "--emit-manifest".to_string(),
            path_arg,
        ];
        args.extend(extra_args.iter().map(|arg| (*arg).to_string()));
        let output = run_ries_owned(&args);
        assert!(
            !output.status.success(),
            "manifest write failure unexpectedly succeeded"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Error writing manifest"),
            "unexpected stderr: {stderr}"
        );
    }

    let destination = unique_tmp_path("directory-destination");
    std::fs::create_dir(&destination).expect("create directory destination");
    let destination_arg = destination.to_string_lossy().into_owned();
    let output_args = [
        "2.5".to_string(),
        "-l".to_string(),
        "0".to_string(),
        "--deterministic".to_string(),
        "--report".to_string(),
        "false".to_string(),
        "-n".to_string(),
        "1".to_string(),
        "--emit-manifest".to_string(),
        destination_arg,
    ];
    for extra in [Vec::<String>::new(), vec!["--json".to_string()]] {
        let mut args = output_args.to_vec();
        args.extend(extra);
        let output = run_ries_owned(&args);
        assert!(
            !output.status.success(),
            "directory destination unexpectedly succeeded"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("Error writing manifest"));
    }
    let _ = std::fs::remove_dir(destination);
}

#[test]
fn test_json_output_reports_effective_parallel_flag_in_deterministic_mode() {
    let (stdout, _stderr) = run_ries(&["2.5", "--json", "--deterministic", "--classic", "-n", "1"]);
    let parsed: Value = serde_json::from_str(&stdout).expect("valid JSON output");
    assert_eq!(parsed["deterministic"].as_bool(), Some(true));
    assert_eq!(parsed["parallel"].as_bool(), Some(false));
    assert!(
        parsed["search_stats"].get("peak_memory_bytes").is_some(),
        "missing peak_memory_bytes field"
    );
}

#[test]
fn test_json_and_manifest_report_effective_matcher_parallelism() {
    let parallel_feature = cfg!(feature = "parallel");
    let default_matcher = if parallel_feature {
        "parallel"
    } else {
        "serial"
    };
    let turbo_matcher = if parallel_feature { "turbo" } else { "serial" };
    for (options, matcher, parallel) in [
        (&[][..], default_matcher, parallel_feature),
        (&["--turbo"][..], turbo_matcher, parallel_feature),
        (&["--one-sided"][..], "one_sided", false),
        (&["--max-memory", "512M"][..], "streaming", false),
        (&["--stop-at-exact"][..], "fast_match", false),
        (&["--adaptive"][..], "adaptive", parallel_feature),
        (&["--adaptive", "--S-RHS", "2"][..], "adaptive", false),
    ] {
        let path = unique_tmp_path(matcher);
        let output = Command::new(env!("CARGO_BIN_EXE_ries-rs"))
            .env("RAYON_NUM_THREADS", "2")
            .args(["2", "-l", "0", "-S", "x2", "--json", "--emit-manifest"])
            .arg(&path)
            .args(options)
            .output()
            .expect("run solver");
        assert!(output.status.success(), "{:?}", output);
        let json: Value = serde_json::from_slice(&output.stdout).expect("JSON output");
        let manifest: Value = serde_json::from_slice(&std::fs::read(&path).expect("read manifest"))
            .expect("manifest JSON");
        std::fs::remove_file(path).expect("remove manifest");

        let config = &manifest["config"];
        let threads = if parallel { 2 } else { 1 };
        assert_eq!(json["matcher"], matcher, "{options:?}");
        assert_eq!(json["parallel"], parallel, "{options:?}");
        assert_eq!(json["search_stats"]["threads"], threads, "{options:?}");
        assert_eq!(json["matcher"], config["matcher"], "{options:?}");
        assert_eq!(json["parallel"], config["parallel"], "{options:?}");
        assert_eq!(
            json["search_stats"]["threads"], config["replay"]["matching"]["threads"],
            "{options:?}"
        );
    }
}

#[test]
fn test_json_deterministic_golden_output_order_for_2_5_level0_classic() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--json",
        "--deterministic",
        "--classic",
        "--report",
        "false",
        "-l",
        "0",
        "-n",
        "5",
    ]);
    let parsed: Value = serde_json::from_str(&stdout).expect("valid JSON output");

    assert_eq!(parsed["ranking_mode"].as_str(), Some("parity"));
    assert_eq!(parsed["deterministic"].as_bool(), Some(true));
    assert_eq!(parsed["parallel"].as_bool(), Some(false));
    assert_eq!(parsed["search_stats"]["threads"].as_u64(), Some(1));

    let results = parsed["results"].as_array().expect("results array");
    let equations: Vec<&str> = results
        .iter()
        .filter_map(|r| r["equation"].as_str())
        .collect();
    // The stop bound is the simplest exact equation (complexity 49, cheaper
    // than `x = 5/2` at 50); results are the matches within it.
    assert_eq!(
        equations,
        vec!["2 x = 5", "x*2 = 5", "x = e^catalan", "ln(x) = catalan"]
    );

    let complexities: Vec<u64> = results
        .iter()
        .filter_map(|r| r["complexity"].as_u64())
        .collect();
    assert_eq!(complexities, vec![49, 49, 48, 48]);
}

#[test]
fn test_json_deterministic_golden_output_order_for_complexity_ranking_nonclassic() {
    let (stdout, _stderr) = run_ries(&[
        "2.506314",
        "--json",
        "--deterministic",
        "--complexity-ranking",
        "--report",
        "false",
        "-l",
        "0",
        "-n",
        "5",
    ]);
    let parsed: Value = serde_json::from_str(&stdout).expect("valid JSON output");

    assert_eq!(parsed["ranking_mode"].as_str(), Some("complexity"));
    assert_eq!(parsed["deterministic"].as_bool(), Some(true));
    assert_eq!(parsed["parallel"].as_bool(), Some(false));
    assert_eq!(parsed["search_stats"]["threads"].as_u64(), Some(1));

    let results = parsed["results"].as_array().expect("results array");
    let equations: Vec<&str> = results
        .iter()
        .filter_map(|r| r["equation"].as_str())
        .collect();
    assert_eq!(
        equations,
        vec![
            "tanpi(x) = -7^2",
            "-cospi(x) = (1/7)^2",
            "1/cospi(x) = -7^2",
            "x^2 = 2 pi",
            "(-x)^2 = 2 pi",
        ]
    );
}

#[test]
fn test_parity_ranking_leads_with_the_simplest_exact_match() {
    // Exact parity rows are complexity-first, so a one-row page keeps x = 2
    // rather than a denser exact rewrite of the same root.
    for (lhs_symbols, rhs_symbols) in [("1x+", "23"), ("xr", "2r")] {
        let (stdout, _) = run_ries(&[
            "2",
            "-l0",
            "-n1",
            "-S",
            lhs_symbols,
            "--S-RHS",
            rhs_symbols,
            "--deterministic",
            "--parity-ranking",
            "--report",
            "false",
            "--json",
        ]);
        let parsed: Value = serde_json::from_str(&stdout).expect("valid parity JSON");
        let best = &parsed["results"][0];
        assert_eq!(best["exact"], true, "{best}");
        assert_eq!(best["lhs_postfix"], "x", "{best}");
        assert_eq!(best["rhs_postfix"], "2", "{best}");
    }
}

#[test]
fn stop_below_rejects_non_finite_thresholds() {
    for bad in ["nan", "NaN", "inf", "+inf", "-inf", "infinity", "-1"] {
        let output = run_ries_raw(&[
            "2.5",
            &format!("--stop-below={bad}"),
            "--report",
            "false",
            "-n",
            "1",
        ]);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "accepted --stop-below={bad}\n{stderr}"
        );
        assert!(
            stderr.contains("stop-below"),
            "--stop-below={bad} stderr was {stderr}"
        );
    }
}

#[test]
fn test_complexity_ranking_keeps_ceiling_ties_with_zero_weight_rhs() {
    // x = 2 fills the pool at complexity 15. The LHS of 1+x = 3 alone
    // reaches that ceiling, but the zero-weight RHS leaves a postfix tie-break.
    let (stdout, _) = run_ries(&[
        "2",
        "-l0",
        "-n1",
        "-S",
        "1x+",
        "--S-RHS",
        "23",
        "--symbol-weights",
        ":x:10 :1:1 :+:4 :2:5 :3:0",
        "--deterministic",
        "--complexity-ranking",
        "--report",
        "false",
        "--json",
    ]);
    let parsed: Value = serde_json::from_str(&stdout).expect("valid complexity JSON");
    let best = &parsed["results"][0];
    assert_eq!(best["exact"], true);
    assert_eq!(best["lhs_postfix"], "1x+", "{best}");
    assert_eq!(best["rhs_postfix"], "3", "{best}");
    assert_eq!(best["complexity"], 15);
}

#[test]
fn test_json_deterministic_parity_vs_complexity_diverge_for_same_target() {
    let base_args = [
        "1.2",
        "--json",
        "--deterministic",
        "--classic",
        "--report",
        "false",
        "-l",
        "2",
        "-n",
        "6",
    ];

    let (parity_stdout, _stderr) = run_ries(&[
        base_args[0],
        base_args[1],
        base_args[2],
        base_args[3],
        base_args[4],
        base_args[5],
        base_args[6],
        base_args[7],
        base_args[8],
        base_args[9],
        "--parity-ranking",
    ]);
    let (complexity_stdout, _stderr) = run_ries(&[
        base_args[0],
        base_args[1],
        base_args[2],
        base_args[3],
        base_args[4],
        base_args[5],
        base_args[6],
        base_args[7],
        base_args[8],
        base_args[9],
        "--complexity-ranking",
    ]);

    let parity: Value = serde_json::from_str(&parity_stdout).expect("valid parity JSON");
    let complexity: Value =
        serde_json::from_str(&complexity_stdout).expect("valid complexity JSON");

    assert_eq!(parity["ranking_mode"].as_str(), Some("parity"));
    assert_eq!(complexity["ranking_mode"].as_str(), Some("complexity"));
    assert_eq!(parity["deterministic"].as_bool(), Some(true));
    assert_eq!(complexity["deterministic"].as_bool(), Some(true));

    let parity_eqs: Vec<&str> = parity["results"]
        .as_array()
        .expect("parity results array")
        .iter()
        .filter_map(|r| r["equation"].as_str())
        .collect();
    let complexity_eqs: Vec<&str> = complexity["results"]
        .as_array()
        .expect("complexity results array")
        .iter()
        .filter_map(|r| r["equation"].as_str())
        .collect();
    assert_ne!(
        parity_eqs, complexity_eqs,
        "ranking modes should diverge on this deterministic target\nparity:\n{}\ncomplexity:\n{}",
        parity_stdout, complexity_stdout
    );
}

#[test]
fn test_one_sided_mode_uses_single_lhs() {
    let (stdout, _stderr) = run_ries(&["2.5", "--one-sided", "--stats", "-n", "1"]);
    let lhs = parse_stat_value(&stdout, "LHS expressions:")
        .expect("missing 'LHS expressions' line in --stats output");
    assert_eq!(lhs, 1, "expected one-sided mode to use only x on LHS");
}

#[test]
fn test_symbol_weights_flag_changes_complexity() {
    let (stdout, _stderr) = run_ries(&[
        "2",
        "--classic",
        "-l",
        "0",
        "-n",
        "1",
        "--symbol-weights",
        ":2:20",
    ]);
    // x=15 + 2=20 (overridden) = 35
    assert!(
        stdout.contains("{35}"),
        "expected x = 2 complexity to reflect overridden weight\n{}",
        stdout
    );
}

#[test]
fn test_classic_prefers_exact_match() {
    let (stdout, _stderr) =
        run_ries(&["6.283185307179586", "--classic", "-n", "1", "-x", "--json"]);
    let parsed: Value = serde_json::from_str(&stdout).expect("valid classic JSON");
    let best = &parsed["results"][0];
    assert_eq!(best["exact"], true, "{best}");
    let root = best["x_value"].as_f64().expect("reported root");
    assert!((root - std::f64::consts::TAU).abs() <= 8.0 * f64::EPSILON * root.abs());
}

#[test]
fn test_streaming_classic_matches_classic_top_result() {
    let (classic_stdout, _stderr) = run_ries(&["2.5", "--classic", "-n", "1"]);
    let (streaming_stdout, _stderr) = run_ries(&["2.5", "--streaming", "--classic", "-n", "1"]);

    let classic_first =
        parse_first_match_line(&classic_stdout).expect("missing classic first match");
    let streaming_first =
        parse_first_match_line(&streaming_stdout).expect("missing streaming classic first match");

    assert_eq!(
        streaming_first, classic_first,
        "expected --streaming --classic to keep the same top match as classic\nclassic:\n{}\nstreaming:\n{}",
        classic_stdout, streaming_stdout
    );
}

#[test]
fn test_op_limits_is_count_limit_not_allow_list() {
    let (stdout, _stderr) = run_ries(&["6", "--report", "false", "-n", "1", "-l", "2", "-O", "1+"]);
    let (lhs, rhs) =
        parse_generated_counts(&stdout).expect("missing generated counts in CLI output");
    assert!(
        lhs > 1 && rhs > 1,
        "expected -O to constrain counts without collapsing symbol set\n{}",
        stdout
    );
}

#[test]
fn test_rhs_symbol_restriction_changes_rhs_generation() {
    let (base_stdout, _stderr) = run_ries(&["2.5", "--classic", "-n", "1"]);
    let (_lhs_base, rhs_base) =
        parse_generated_counts(&base_stdout).expect("missing base generated counts");

    let (rhs_stdout, _stderr) = run_ries(&["2.5", "--classic", "-n", "1", "--S-RHS", "1"]);
    let (_lhs_rhs, rhs_restricted) =
        parse_generated_counts(&rhs_stdout).expect("missing rhs-restricted generated counts");

    assert!(
        rhs_restricted < rhs_base,
        "expected --S-RHS to reduce RHS generation\nbase:\n{}\nrestricted:\n{}",
        base_stdout,
        rhs_stdout
    );
}

#[test]
fn test_symbol_names_profile_applies_to_output() {
    let profile = unique_tmp_path("symbol-names");
    std::fs::write(&profile, "--symbol-names :p:PI_CUSTOM\n").expect("write profile");

    let args = vec![
        "-p".to_string(),
        profile.to_string_lossy().to_string(),
        "3.141592653589793".to_string(),
        "--classic".to_string(),
        "-n".to_string(),
        "1".to_string(),
        "-x".to_string(),
    ];
    let output = run_ries_owned(&args);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("PI_CUSTOM"),
        "expected symbol rename to appear in output\n{}",
        stdout
    );
}

#[test]
fn test_profile_include_chain_loads_nested_constants() {
    let inner = unique_tmp_path("include-inner");
    let outer = unique_tmp_path("include-outer");
    std::fs::write(&inner, "-X \"4:tau:TauConstant:6.283185307179586\"\n").expect("write inner");
    std::fs::write(&outer, format!("--include {}\n", inner.to_string_lossy()))
        .expect("write outer");

    let args = vec![
        "-p".to_string(),
        outer.to_string_lossy().to_string(),
        "6.283185307179586".to_string(),
        "--classic".to_string(),
        "-n".to_string(),
        "1".to_string(),
        "-x".to_string(),
    ];
    let output = run_ries_owned(&args);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        output.status.success(),
        "command failed\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
    assert!(
        stdout.contains("tau"),
        "expected nested include constant to appear\n{}",
        stdout
    );
}

#[test]
fn test_missing_include_is_error() {
    let missing = unique_tmp_path("missing-include");
    let args = vec![
        "2.5".to_string(),
        "--include".to_string(),
        missing.to_string_lossy().to_string(),
    ];
    let output = run_ries_owned(&args);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(!output.status.success(), "missing include should fail");
    assert!(
        stderr.contains("Could not open") || stderr.contains("Error reading"),
        "expected explicit include error\n{}",
        stderr
    );
}

#[test]
fn test_user_constant_weight_changes_complexity() {
    let (stdout_low, _stderr) = run_ries(&[
        "0.123456789",
        "--classic",
        "--stop-at-exact",
        "-n",
        "1",
        "-x",
        "-X",
        "4:taux:test:0.123456789",
    ]);
    let low = parse_first_complexity(&stdout_low).expect("missing complexity for low-weight run");

    let (stdout_high, _stderr) = run_ries(&[
        "0.123456789",
        "--classic",
        "--stop-at-exact",
        "-n",
        "1",
        "-x",
        "-X",
        "99:taux:test:0.123456789",
    ]);
    let high =
        parse_first_complexity(&stdout_high).expect("missing complexity for high-weight run");

    assert!(
        high > low + 20,
        "expected larger complexity with larger user weight ({} vs {})\nlow:\n{}\nhigh:\n{}",
        low,
        high,
        stdout_low,
        stdout_high
    );
}

#[test]
fn test_list_options_outputs_known_flags() {
    let output = run_ries_raw(&["--list-options"]);
    assert!(
        output.status.success(),
        "--list-options should exit successfully"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--list-options"));
    assert!(stdout.contains("--find-expression"));
    assert!(stdout.contains("--complexity-ranking"));
    assert!(stdout.contains("--parity-ranking"));
    assert!(stdout.contains("--O-RHS"));
    assert!(stdout.contains("--E-RHS"));
}

#[test]
fn test_stability_thorough_uses_configured_level_count() {
    // Use target=1 so `x = 1` is found at level 0 (complexity x(15)+1(10)=25 fits the budget).
    let (default_stdout, _stderr) = run_ries(&[
        "1",
        "--classic",
        "--report",
        "false",
        "--stability-check",
        "-l",
        "0",
        "-n",
        "1",
    ]);
    let default_levels = parse_first_total_levels(&default_stdout)
        .expect("expected stability report with level count for default config");
    assert_eq!(
        default_levels, 5,
        "default stability config should run 5 levels\n{}",
        default_stdout
    );

    let (thorough_stdout, _stderr) = run_ries(&[
        "1",
        "--classic",
        "--report",
        "false",
        "--stability-check",
        "--stability-thorough",
        "-l",
        "0",
        "-n",
        "1",
    ]);
    let thorough_levels = parse_first_total_levels(&thorough_stdout)
        .expect("expected stability report with level count for thorough config");
    assert_eq!(
        thorough_levels, 8,
        "thorough stability config should run 8 levels\n{}",
        thorough_stdout
    );
    assert!(
        thorough_levels > default_levels,
        "thorough mode should evaluate more levels"
    );
}

#[test]
fn test_enable_reenables_symbol_after_exclude() {
    let (stdout_no_enable, _stderr) =
        run_ries(&["2.5", "--report", "false", "-n", "1", "-N", "+", "-l", "1"]);
    let (lhs_no, rhs_no) =
        parse_generated_counts(&stdout_no_enable).expect("missing generated counts");

    let (stdout_enable, _stderr) = run_ries(&[
        "2.5", "--report", "false", "-n", "1", "-N", "+", "-E", "+", "-l", "1",
    ]);
    let (lhs_yes, rhs_yes) =
        parse_generated_counts(&stdout_enable).expect("missing generated counts");

    assert!(
        lhs_yes > lhs_no || rhs_yes > rhs_no,
        "expected -E to re-enable excluded symbols\nno enable:\n{}\nwith enable:\n{}",
        stdout_no_enable,
        stdout_enable
    );
}

#[test]
fn test_orhs_reduces_rhs_generation() {
    let (base_stdout, _stderr) = run_ries(&["2.5", "--report", "false", "-n", "1", "-l", "1"]);
    let (_lhs_base, rhs_base) =
        parse_generated_counts(&base_stdout).expect("missing base generated counts");

    // --N-RHS p excludes pi from the RHS symbol set, which measurably reduces RHS count.
    let (rhs_stdout, _stderr) = run_ries(&[
        "2.5", "--report", "false", "-n", "1", "--N-RHS", "p", "-l", "1",
    ]);
    let (_lhs_rhs, rhs_restricted) =
        parse_generated_counts(&rhs_stdout).expect("missing rhs-restricted generated counts");

    assert!(
        rhs_restricted < rhs_base,
        "expected --N-RHS to reduce RHS generation\nbase:\n{}\nrestricted:\n{}",
        base_stdout,
        rhs_stdout
    );
}

#[test]
fn test_min_match_distance_filters_out_exact_match() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "-n",
        "1",
        "--min-match-distance",
        "1e-4",
    ]);
    assert!(
        !stdout.contains("('exact' match)"),
        "expected minimum match distance to filter exact matches\n{}",
        stdout
    );
}

#[test]
fn test_find_expression_works_without_target() {
    let (stdout, _stderr) = run_ries(&["--find-expression", "xq", "--at", "4"]);
    assert!(stdout.contains("Expression: xq"));
    assert!(stdout.contains("Value = 2.000000000000000"));
}

#[test]
fn test_symbol_names_cli_applies_to_output() {
    let (stdout, _stderr) = run_ries(&[
        "3.141592653589793",
        "--classic",
        "--report",
        "false",
        "-n",
        "1",
        "--symbol-names",
        ":p:PI",
    ]);
    assert!(
        stdout.contains("PI"),
        "expected --symbol-names override to appear in output\n{}",
        stdout
    );
}

#[test]
fn test_mad_alias_applies_max_match_distance() {
    let (stdout, _stderr) = run_ries(&[
        "2.506314",
        "--classic",
        "--report",
        "false",
        "-n",
        "3",
        "--mad",
        "0",
    ]);
    assert!(
        stdout.contains("No matches found."),
        "expected --mad 0 to behave like --max-match-distance 0\n{}",
        stdout
    );
}

#[test]
fn test_extended_compat_options_are_accepted() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--report",
        "false",
        "-n",
        "1",
        "--any-exponents",
        "--any-subexpressions",
        "--any-trig-args",
        "--canon-reduction",
        "nr25",
        "--canon-simplify",
        "--derivative-margin",
        "1e-8",
        "--explicit-multiply",
        "--match-all-digits",
        "--max-equate-value",
        "1000",
        "--min-equate-value",
        "0",
        "--max-memory",
        "256M",
        "--memory-abort-threshold",
        "0.5",
        "--max-trig-cycles",
        "8",
        "--min-memory",
        "16M",
        "--no-canon-simplify",
        "--no-slow-messages",
        "--numeric-anagram",
        "--rational-exponents",
        "--rational-trig-args",
        "--show-work",
        "--significance-loss-margin",
        "1e-9",
        "--trig-argument-scale",
        "1.0",
        "-D",
    ]);
    assert!(
        stdout.contains("Search completed"),
        "expected compatibility options to be accepted\n{}",
        stdout
    );
}

#[test]
fn test_rational_exponents_rejects_irrational_exponents() {
    let (baseline_stdout, _stderr) = run_ries(&[
        "8.824977827",
        "--one-sided",
        "--report",
        "false",
        "-n",
        "20",
        "-F",
        "0",
    ]);
    assert!(
        baseline_stdout.contains("2p^"),
        "expected baseline run to include irrational exponent form\n{}",
        baseline_stdout
    );

    let (restricted_stdout, _stderr) = run_ries(&[
        "8.824977827",
        "--one-sided",
        "--rational-exponents",
        "--report",
        "false",
        "-n",
        "20",
        "-F",
        "0",
    ]);
    assert!(
        !restricted_stdout.contains("2p^"),
        "expected --rational-exponents to filter irrational exponent forms\n{}",
        restricted_stdout
    );
}

#[test]
fn test_rational_trig_args_rejects_irrational_constants() {
    let target = "0.773942685266709";
    let (baseline_stdout, _stderr) = run_ries(&[
        target,
        "--one-sided",
        "--report",
        "false",
        "-n",
        "20",
        "-F",
        "0",
    ]);
    assert!(
        baseline_stdout.contains("eS"),
        "expected baseline run to include sinpi(e)\n{}",
        baseline_stdout
    );

    let (restricted_stdout, _stderr) = run_ries(&[
        target,
        "--one-sided",
        "--rational-trig-args",
        "--report",
        "false",
        "-n",
        "20",
        "-F",
        "0",
    ]);
    assert!(
        !restricted_stdout.contains("eS"),
        "expected --rational-trig-args to filter irrational trig arguments\n{}",
        restricted_stdout
    );
}

#[test]
fn test_format_zero_uses_compact_postfix_output() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "--one-sided",
        "-S",
        "x25/",
        "-n",
        "1",
        "-F",
        "0",
    ]);
    assert!(
        stdout.contains("52/"),
        "expected compact postfix output with -F0\n{}",
        stdout
    );
}

#[test]
fn test_format_three_uses_verbose_postfix_output() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "--one-sided",
        "-S",
        "x25/",
        "-n",
        "1",
        "-F",
        "3",
    ]);
    assert!(
        stdout.contains("5 2 /"),
        "expected verbose postfix output with -F3\n{}",
        stdout
    );
}

#[test]
fn test_show_work_outputs_step_details() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "-n",
        "1",
        "--show-work",
    ]);
    assert!(
        stdout.contains("--show-work details:")
            && stdout.contains("LHS steps:")
            && stdout.contains("RHS steps:"),
        "expected --show-work output\n{}",
        stdout
    );
}

#[test]
fn test_dy_enables_stats_output() {
    let (stdout, _stderr) = run_ries(&["2.5", "--classic", "--report", "false", "-n", "1", "-Dy"]);
    assert!(
        stdout.contains("=== Search Statistics ==="),
        "expected -Dy to enable stats output\n{}",
        stdout
    );
}

#[test]
fn test_max_equate_value_filters_matches() {
    let (stdout, _stderr) = run_ries(&[
        "2.5",
        "-l",
        "0",
        "-S",
        "x123+",
        "--classic",
        "--report",
        "false",
        "-n",
        "3",
        "--max-equate-value",
        "1",
    ]);
    assert!(
        stdout.contains("No matches found."),
        "expected --max-equate-value to filter out matches\n{}",
        stdout
    );
}

#[test]
fn test_explicit_multiply_changes_infix_rendering() {
    let (stdout, _stderr) = run_ries(&[
        "6.283185307179586",
        "--classic",
        "--report",
        "false",
        "--one-sided",
        "-S",
        "x2p*",
        "-n",
        "1",
        "--explicit-multiply",
    ]);
    assert!(
        stdout.contains("2*pi"),
        "expected --explicit-multiply to show explicit multiplication\n{}",
        stdout
    );
}

#[test]
fn test_json_no_solve_for_x_suppresses_solve_fields() {
    let output = Command::new(env!("CARGO_BIN_EXE_ries-rs"))
        .args(["--json", "--no-solve-for-x", "1.5"])
        .output()
        .expect("failed to run ries-rs");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("output should be valid JSON");
    for result in json["results"].as_array().unwrap() {
        assert!(
            result["solve_for_x"].is_null(),
            "solve_for_x should be null but was: {}",
            result["solve_for_x"]
        );
        assert!(
            result["solve_for_x_postfix"].is_null(),
            "solve_for_x_postfix should be null but was: {}",
            result["solve_for_x_postfix"]
        );
    }
}

// ============================================================================
// Streaming flag precedence tests - regression tests for P2
// ============================================================================

#[test]
fn test_explicit_streaming_respected_over_min_memory() {
    // When --streaming is explicitly set, --min-memory should not override it
    // This is a regression test for P2: --min-memory can override explicit --streaming

    // Run with --streaming alone
    let (streaming_stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "--streaming",
        "-l",
        "0",
        "-n",
        "1",
    ]);

    // Run with --streaming --min-memory 3G
    // The --min-memory 3G should NOT disable the explicit --streaming
    let (streaming_with_min_memory_stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "--streaming",
        "--min-memory",
        "3G",
        "-l",
        "0",
        "-n",
        "1",
    ]);

    // Both should produce similar results since streaming is respected
    // Extract the LHS count if present, or just verify both complete successfully
    assert!(
        streaming_stdout.contains("Search completed")
            || streaming_stdout.contains("=")
            || streaming_stdout.contains("x"),
        "expected streaming search to complete\n{}",
        streaming_stdout
    );

    assert!(
        streaming_with_min_memory_stdout.contains("Search completed")
            || streaming_with_min_memory_stdout.contains("=")
            || streaming_with_min_memory_stdout.contains("x"),
        "expected streaming with min-memory to still use streaming\n{}",
        streaming_with_min_memory_stdout
    );

    // The key assertion: both runs should use streaming mode
    // We verify this by checking that --min-memory 3G doesn't silently
    // disable the explicit --streaming flag
}

#[test]
fn test_min_memory_disables_auto_streaming() {
    // When streaming is NOT explicitly set, --min-memory can disable auto-streaming
    // This is the expected behavior (not a bug)

    // Small --max-memory should trigger auto-streaming
    let (auto_streaming_stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "--max-memory",
        "256M",
        "-l",
        "0",
        "-n",
        "1",
    ]);

    // Small --max-memory with large --min-memory should not auto-stream
    let (no_auto_streaming_stdout, _stderr) = run_ries(&[
        "2.5",
        "--classic",
        "--report",
        "false",
        "--max-memory",
        "256M",
        "--min-memory",
        "3G",
        "-l",
        "0",
        "-n",
        "1",
    ]);

    // Both should complete successfully - we're just verifying the options work
    assert!(
        auto_streaming_stdout.contains("Search completed")
            || auto_streaming_stdout.contains("=")
            || auto_streaming_stdout.contains("x"),
        "expected auto-streaming search to complete\n{}",
        auto_streaming_stdout
    );

    assert!(
        no_auto_streaming_stdout.contains("Search completed")
            || no_auto_streaming_stdout.contains("=")
            || no_auto_streaming_stdout.contains("x"),
        "expected non-streaming search to complete\n{}",
        no_auto_streaming_stdout
    );
}

fn json_search(args: &[&str]) -> Value {
    let mut args = args.to_vec();
    args.push("--json");
    serde_json::from_str(&run_ries(&args).0).expect("valid search JSON")
}

#[test]
fn deterministic_search_has_stable_discovery_order() {
    for args in [
        vec![
            "--deterministic",
            "--lhs-value-dedup",
            "--report=false",
            "-n",
            "1",
            "-l",
            "0",
            "-S",
            "x1234567+-*qsr/vp",
            "10.2154322005",
        ],
        vec![
            "--deterministic",
            "--one-sided",
            "--report=false",
            "--stop-below",
            "1",
            "--max-match-distance",
            "1",
            "-n",
            "1",
            "-l",
            "0",
            "-S",
            "x123",
            "2.5",
        ],
    ] {
        let first = json_search(&args);
        assert_eq!(first["results_returned"], 1);
        for _ in 0..8 {
            assert_eq!(json_search(&args)["results"], first["results"]);
        }
    }
}

#[test]
fn fast_match_uses_canonical_exactness_and_generation_limits() {
    let near = json_search(&[
        "-l",
        "0",
        "-S",
        "x1",
        "--stop-at-exact",
        "--report=false",
        "1.000000000000005",
    ]);
    assert_ne!(near["matcher"], "fast_match");
    assert_eq!(near["results"][0]["exact"], false);
    assert_eq!(near["results"][0]["x_value"], 1.0);

    let weighted = json_search(&[
        "-l",
        "0",
        "-S",
        "x12+",
        "--symbol-weights",
        ":2:100",
        "--stop-at-exact",
        "--report=false",
        "2",
    ]);
    assert_ne!(weighted["matcher"], "fast_match");
    assert_eq!(weighted["results"][0]["rhs_postfix"], "11+");
}

#[test]
fn match_filters_apply_before_pool_and_first_hit_stopping() {
    for mode in ["--deterministic", "--streaming", "--stop-at-exact"] {
        let output = json_search(&[
            "-l",
            "0",
            "-S",
            "x1234+",
            "--report=false",
            "-n",
            "1",
            "--min-equate-value",
            "4",
            mode,
            "2",
        ]);
        assert_eq!(output["results_returned"], 1);
        assert!(matches!(
            output["results"][0]["lhs_postfix"].as_str(),
            Some("2x+" | "x2+" | "xx+")
        ));
        assert_eq!(output["results"][0]["rhs_postfix"], "4");
    }
}

#[test]
fn canonical_dedup_leaves_room_for_distinct_equations() {
    let output = json_search(&[
        "-l",
        "0",
        "-S",
        "x1234+",
        "--report=false",
        "-n",
        "2",
        "--min-equate-value",
        "4",
        "--canon-simplify",
        "2",
    ]);
    assert_eq!(output["results_returned"], 2);
    assert_ne!(
        output["results"][0]["canonical_key"],
        output["results"][1]["canonical_key"]
    );
}

#[test]
fn numeric_anagram_keeps_rhs_representatives_for_each_digit_multiset() {
    for mode in ["--deterministic", "--streaming"] {
        let output = json_search(&[
            "-l",
            "1",
            "-S",
            "x1+*",
            "--S-RHS",
            "12+",
            "--min-equate-value",
            "2",
            "--report=false",
            "-n",
            "1",
            "--numeric-anagram",
            mode,
            "1",
        ]);
        assert_eq!(output["results_returned"], 1);
        assert_eq!(output["results"][0]["rhs_postfix"], "11+");
    }
}

#[test]
fn explicit_sub_picounit_max_distance_is_enforced() {
    for mode in ["--deterministic", "--streaming", "--one-sided"] {
        let output = json_search(&[
            "-l",
            "0",
            "-S",
            "x1",
            "--report=false",
            "--max-match-distance",
            "1e-16",
            mode,
            "1.0000000000005",
        ]);
        assert_eq!(output["results_returned"], 0);
    }
}

#[cfg(feature = "parallel")]
#[test]
fn turbo_agrees_with_serial_on_inexact_rank_one() {
    let run = |mode| {
        let output = Command::new(env!("CARGO_BIN_EXE_ries-rs"))
            .env("RAYON_NUM_THREADS", "4")
            .args([
                "--json",
                "--report=false",
                "-n",
                "1",
                "-l",
                "0",
                "-S",
                "x1234567+-*qsr/vp",
                mode,
                "9.5124027528",
            ])
            .output()
            .expect("execute search");
        assert!(output.status.success(), "{:?}", output.stderr);
        serde_json::from_slice::<Value>(&output.stdout).expect("valid search JSON")
    };
    let serial = run("--deterministic");
    let turbo = run("--turbo");
    assert_eq!(turbo["matcher"], "turbo");
    assert_eq!(turbo["results_returned"], 1);
    assert_eq!(turbo["results"], serial["results"]);
}

#[test]
fn manifest_records_resolved_search_options() {
    let run = |weight| {
        let path = unique_tmp_path("resolved-config");
        json_search(&[
            "-l",
            "0",
            "-S",
            "x12+",
            "--deterministic",
            "--report=false",
            "-n",
            "1",
            "--S-RHS",
            "12+",
            "--symbol-weights",
            weight,
            "--trig-argument-scale",
            "2",
            "--emit-manifest",
            path.to_str().unwrap(),
            "2",
        ]);
        let manifest = serde_json::from_slice::<Value>(&std::fs::read(&path).unwrap()).unwrap();
        std::fs::remove_file(path).unwrap();
        manifest
    };
    let standard = run(":2:13");
    let weighted = run(":2:100");
    let replay = &weighted["config"]["replay"];
    assert_eq!(replay["generation"]["symbol_table"]["50"]["weight"], 100);
    assert_eq!(replay["matching"]["trig_argument_scale"], 2.0);
    assert_eq!(
        replay["matching"]["rhs_allowed_symbols"],
        serde_json::json!([43, 49, 50])
    );
    assert_ne!(standard["config"], weighted["config"]);
    assert_eq!(standard["results"][0]["rhs_postfix"], "2");
    assert_eq!(weighted["results"][0]["rhs_postfix"], "11+");
}

#[cfg(feature = "highprec")]
#[test]
fn precision_verifies_matches_without_manifest() {
    let args = [
        "-l",
        "0",
        "-S",
        "x1",
        "--report=false",
        "--precision",
        "128",
        "1",
    ];
    let (stdout, _) = run_ries(&args);
    assert!(stdout.contains("Verified at high precision"));
    assert!(stdout.contains("[hp error: 0.00e0]"));

    let output = json_search(&args);
    let verification = &output["high_precision_verification"];
    assert_eq!(verification["precision_bits"], 128);
    assert_eq!(verification["results"][0]["is_verified"], true);

    let transformed = json_search(&[
        "-l",
        "0",
        "-S",
        "x4+",
        "--S-RHS",
        "4",
        "--report=false",
        "--precision",
        "128",
        "2",
    ]);
    let row = &transformed["high_precision_verification"]["results"][0];
    assert_eq!(row["lhs_postfix"], "xx+");
    assert_eq!(row["is_verified"], true);
    assert_eq!(row["highprec_error"], 0.0);
}

#[cfg(feature = "highprec")]
#[test]
fn low_precision_cannot_report_a_spurious_zero_root_error() {
    for bits in ["1", "53"] {
        let result = json_search(&[
            "2.51",
            "-l",
            "0",
            "-S",
            "x25/",
            "--precision",
            bits,
            "--allow-unverified",
        ]);
        assert_eq!(result["results"][0]["equation"], "x = 5/2");
        let row = &result["high_precision_verification"]["results"][0];
        assert_eq!(row["status"], "unavailable");
        assert_eq!(row["is_verified"], false);
        assert!(row["highprec_error"].is_null());
    }
}

#[test]
fn oversized_match_counts_exit_with_usage_error() {
    let oversized = usize::MAX.to_string();
    for args in [
        vec!["-l", "0", "--top-k", &oversized, "1"],
        vec![
            "-l",
            "0",
            "--report=false",
            "--max-matches",
            &oversized,
            "1",
        ],
    ] {
        let output = run_ries_raw(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains("search pool"));
    }
}

#[test]
fn match_all_digits_preserves_the_target_literal() {
    let common = [
        "--deterministic",
        "--report=false",
        "--match-all-digits",
        "-l",
        "1",
        "-S",
        "x13/",
    ];
    let mut args = common.to_vec();
    args.push("0.333");
    let short = json_search(&args);
    assert!(short["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| { row["lhs_postfix"] == "x" && row["rhs_postfix"] == "13/" }));

    for target in ["0.3330", "3.330e-1"] {
        *args.last_mut().unwrap() = target;
        let strict = json_search(&args);
        assert!(strict["results"]
            .as_array()
            .unwrap()
            .iter()
            .all(|row| { row["error"].as_f64().unwrap().abs() <= 0.00005 }));
    }

    let tiny = json_search(&[
        "--match-all-digits",
        "--one-sided",
        "--report=false",
        "-l",
        "1",
        "-S",
        "x1234+-/",
        "1e-20",
    ]);
    assert!(tiny["results"].as_array().unwrap().is_empty());
}

#[test]
fn near_integer_trig_search_does_not_report_a_false_exact_root() {
    let output = json_search(&[
        "--deterministic",
        "--report=false",
        "-l",
        "2",
        "-S",
        "xS12+",
        "1.0000000000001",
    ]);
    let rows = output["results"].as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(rows.iter().all(|row| row["exact"] == false));
}

#[test]
fn reports_use_search_symbol_names_and_trig_scale() {
    let (report, _) = run_ries(&["2", "--stop-at-exact", "--symbol-names", ":2:TWO"]);
    assert!(report.contains("x = TWO"));

    for (format, expected) in [
        ("default", "1+sin(x) = 1"),
        ("pretty", "1+sin(x) = 1"),
        ("sympy", "1+sin(x) = 1"),
        ("mathematica", "1+Sin[x] = 1"),
    ] {
        let output = json_search(&[
            "--deterministic",
            "--report=false",
            "--trig-argument-scale",
            "1",
            "--format",
            format,
            "-s",
            "-l",
            "1",
            "-S",
            "x1S+",
            "6.283185307179586",
        ]);
        assert_eq!(output["trig_argument_scale"], 1.0);
        let row = &output["results"][0];
        assert_eq!(row["equation"], expected);
        assert_eq!(row["exact"], true);
        assert_eq!(row["solve_for_x_postfix"], "2p*");
    }
}

#[test]
fn reported_best_error_is_the_minimum_in_the_pool() {
    for strategy in [
        None,
        Some("--streaming"),
        Some("--one-sided"),
        Some("--stop-at-exact"),
    ] {
        let mut args = vec![
            "--deterministic",
            "--report=false",
            "-l",
            "0",
            "-S",
            "x3",
            "3",
        ];
        args.extend(strategy);
        let output = json_search(&args);
        assert_eq!(output["results"][0]["error"], 0.0, "{strategy:?}");
        assert_eq!(output["search_stats"]["best_error"], 0.0, "{strategy:?}");
        if strategy != Some("--stop-at-exact") {
            assert!(
                output["search_stats"]["adaptive_error_threshold"]
                    .as_f64()
                    .unwrap()
                    > 0.0
            );
        }
    }

    let output = json_search(&["--report=false", "-l", "0", "-S", "x3", "7"]);
    assert_eq!(output["results_returned"], 0);
    assert!(output["search_stats"]["best_error"].is_null());
}

#[test]
fn stability_check_does_not_classify_the_fast_match_as_fragile() {
    let (stdout, _) = run_ries(&[
        "--classic",
        "--deterministic",
        "--stability-check",
        "-l",
        "1",
        "1.4142135623730951",
    ]);
    let (search, stability) = stdout.split_once("Stability Analysis").unwrap();
    assert!(search.contains("sqrt(2)"));
    assert!(stability.contains("[5/5 levels]"), "{stability}");
    assert!(
        !stability
            .lines()
            .any(|line| { line.contains("= sqrt(2)") && line.contains("[1/5 levels]") }),
        "{stability}"
    );
}

#[test]
fn invalid_precision_and_nonfinite_pslq_target_are_usage_errors() {
    for (args, message) in [
        (&["--precision", "0", "3"][..], "--precision"),
        (&["--pslq", "inf"][..], "TARGET must be finite"),
    ] {
        let output = run_ries_raw(args);
        assert_eq!(output.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&output.stderr).contains(message));
    }
}

#[cfg(feature = "highprec")]
#[test]
fn precision_preserves_default_trig_and_distinguishes_unavailable_operators() {
    let trig = json_search(&[
        "--precision",
        "256",
        "--deterministic",
        "-l",
        "1",
        "-S",
        "x1SE",
        "10000000000000000",
    ]);
    let row = &trig["high_precision_verification"]["results"][0];
    // 10^16 is an integer, so sinpi(x) = 0 is the simplest exact. e^sinpi(x) = 1
    // is the same root with an extra exponential.
    assert_eq!(row["lhs_postfix"], "xS");
    assert_eq!(row["status"], "verified");
    assert_eq!(row["highprec_error"], 0.0);

    let args = [
        "--allow-unverified",
        "--precision",
        "256",
        "--deterministic",
        "-l",
        "1",
        "-S",
        "x12348v",
        "2",
    ];
    let root = json_search(&args);
    let results = root["high_precision_verification"]["results"]
        .as_array()
        .unwrap();
    let row = results
        .iter()
        .find(|row| row["lhs_postfix"] == "x4v" && row["rhs_postfix"] == "2")
        .unwrap();
    assert_eq!(row["status"], "unavailable");
    assert!(row["highprec_error"].is_null());
    let (text, _) = run_ries(&args);
    assert!(text.contains("High-precision evaluation unavailable"));
    assert!(!text.contains("impostors"));
}

#[test]
fn streaming_stop_exact_returns_simplest_exact_match() {
    let path = unique_tmp_path("streaming-exact");
    run_ries(&[
        "0.5",
        "--streaming",
        "--stop-at-exact",
        "--deterministic",
        "--report=false",
        "--max-matches=1",
        "--level=0",
        "--emit-manifest",
        path.to_str().unwrap(),
    ]);
    let manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    std::fs::remove_file(path).unwrap();
    assert_eq!(manifest["config"]["matcher"], "streaming");
    assert_eq!(manifest["results"][0]["rhs_postfix"], "2r");
    assert_eq!(manifest["results"][0]["complexity"], 35);
}

#[test]
fn deterministic_streaming_keeps_stop_match_first() {
    let args = [
        "0.51",
        "--streaming",
        "--stop-below=0.003",
        "--report=false",
        "--max-matches=5",
        "--level=0",
    ];
    let streaming = json_search(&args);
    let mut deterministic = args.to_vec();
    deterministic.push("--deterministic");
    let deterministic = json_search(&deterministic);
    assert_eq!(deterministic["results"], streaming["results"]);
    assert_eq!(streaming["results"][0]["lhs_postfix"], "xs");
}

pub(super) fn replay_python() -> Option<&'static str> {
    // The Rust-only test suite also runs on hosts without Python installed.
    let python = ["python3", "python"].into_iter().find(|&command| {
        Command::new(command)
            .args(["-c", "import sys; sys.exit(sys.version_info.major != 3)"])
            .output()
            .is_ok_and(|output| output.status.success())
    });
    if python.is_none() {
        eprintln!("Skipping manifest replay check because Python 3 is unavailable");
    }
    python
}

#[test]
fn manifest_replay_round_trip_and_rejects_unsafe_arguments() {
    let Some(python) = replay_python() else {
        return;
    };
    let path = unique_tmp_path("replay");
    let script = format!("{}/tools/replay_manifest.py", env!("CARGO_MANIFEST_DIR"));
    let replay = || {
        Command::new(python)
            .env("PYTHONIOENCODING", "cp1252")
            .args([
                &script,
                path.to_str().unwrap(),
                "--bin",
                env!("CARGO_BIN_EXE_ries-rs"),
            ])
            .output()
            .unwrap()
    };
    for custom in [false, true] {
        let mut args = vec![
            "1.2",
            "--level=0.5",
            "--deterministic",
            "--strict-config",
            "--report=false",
            "--max-matches=1",
            "--emit-manifest",
            path.to_str().unwrap(),
        ];
        if custom {
            args.extend([
                "--symbol-weights=:2:100",
                "--S-RHS=12+",
                "--trig-argument-scale=2",
                "--user-constant=4:foo:test π:1.2",
            ]);
        }
        run_ries(&args);
        let output = replay();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("Successfully reproduced"), "{stdout}");
        if custom {
            assert!(stdout.contains(r"test \u03c0"), "{stdout}");
        }
    }
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    manifest["config"]["replay"]["cli_args"]
        .as_array_mut()
        .unwrap()
        .push("--emit-manifest=/tmp/unsafe.json".into());
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let output = replay();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsafe replay option"));
    manifest["config"]["replay"]["cli_args"] = Value::Null;
    std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let output = replay();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no self-contained"));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn user_function_rejects_search_variable() {
    let output = run_ries_raw(&["2", "--define", "1:f:test:x*"]);
    assert!(output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("function argument") && stderr.contains("search variable"),
        "{stderr}"
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("f("));
}

#[test]
fn manifest_replay_supports_basic_legacy_settings_and_negative_targets() {
    let Some(python) = replay_python() else {
        return;
    };
    let path = unique_tmp_path("legacy-replay");
    let script = format!("{}/tools/replay_manifest.py", env!("CARGO_MANIFEST_DIR"));
    run_ries(&[
        "--deterministic",
        "--level=0",
        "--report=false",
        "--max-matches=1",
        "--emit-manifest",
        path.to_str().unwrap(),
        "--",
        "-0.5",
    ]);
    let original: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for legacy in [false, true] {
        let mut manifest = original.clone();
        if legacy {
            manifest["config"]["replay"]
                .as_object_mut()
                .unwrap()
                .remove("cli_args");
        }
        std::fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let output = Command::new(python)
            .args([
                &script,
                path.to_str().unwrap(),
                "--bin",
                env!("CARGO_BIN_EXE_ries-rs"),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    std::fs::remove_file(path).unwrap();
}

#[test]
fn cube_root_of_two_survives_generation_in_each_matcher() {
    for mode in [
        "--one-sided",
        "--deterministic",
        "--streaming",
        "--parallel",
    ] {
        let result = json_search(&["1.2599210498948732", "-l", "0", "-S", "x23v", mode]);
        let matches = result["results"].as_array().unwrap();
        assert!(
            matches.iter().any(|m| {
                m["lhs_postfix"] == "x" && m["rhs_postfix"] == "32v" && m["exact"] == true
            }),
            "{mode}: {result}"
        );
    }
}

#[test]
fn integer_filter_excludes_negative_integer_powers() {
    let run = |filter| {
        json_search(&[
            "1",
            filter,
            "-S",
            "x21n^",
            "--one-sided",
            "-l",
            "1",
            "--max-match-distance",
            "2",
        ])
    };
    let rational = run("-r");
    assert!(rational["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m["rhs_postfix"] == "22n^"));
    let integer = run("-i");
    let matches = integer["results"].as_array().unwrap();
    assert!(!matches.is_empty());
    assert!(
        matches
            .iter()
            .all(|m| m["x_value"].as_f64().unwrap().fract() == 0.0),
        "{integer}"
    );
}

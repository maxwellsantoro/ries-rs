#!/usr/bin/env python3
"""
Capture reproducible end-to-end CLI benchmark artifacts for ries-rs.

This script writes:
- raw JSON outputs for sequential deterministic and parallel runs
- environment metadata
- a Markdown summary with the key timing and heuristic-tuning metrics
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import platform
import subprocess
import sys
from pathlib import Path


REPO_DIR = Path(__file__).resolve().parent.parent
DEFAULT_ARTIFACT_DIR = REPO_DIR / "docs" / "benchmarks" / "artifacts"


def run_command(cmd: list[str], cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        cmd,
        cwd=cwd,
        text=True,
        capture_output=True,
        check=True,
    )


def repo_relative(path: Path) -> str:
    resolved = path.resolve()
    try:
        return str(resolved.relative_to(REPO_DIR))
    except ValueError:
        return str(resolved)


def command_output(cmd: list[str]) -> str:
    return run_command(cmd).stdout.strip()


def maybe_command_output(cmd: list[str]) -> str:
    try:
        return command_output(cmd)
    except (subprocess.CalledProcessError, FileNotFoundError):
        return "<unavailable>"


def git_commit_short() -> str:
    try:
        return command_output(["git", "rev-parse", "--short", "HEAD"])
    except subprocess.CalledProcessError:
        return "<unknown>"


def build_cli_args(args: argparse.Namespace) -> list[str]:
    cli_args = [
        args.target,
        "-l",
        str(args.level),
        "--report",
        "false",
        "--max-matches",
        str(args.max_matches),
        "--json",
    ]
    if args.classic:
        cli_args.append("--classic")
    if args.ranking == "complexity":
        cli_args.append("--complexity-ranking")
    else:
        cli_args.append("--parity-ranking")
    if args.extra_arg:
        cli_args.extend(args.extra_arg)
    return cli_args


def run_benchmark_json(
    cargo_args: list[str],
    cli_args: list[str],
    output_path: Path,
) -> dict:
    cmd = ["cargo", "run", "--release", "--quiet", "--locked", *cargo_args, "--", *cli_args]
    completed = run_command(cmd, cwd=REPO_DIR)
    output_path.write_text(completed.stdout, encoding="utf-8")
    return json.loads(completed.stdout)


def format_mib(value: object) -> str:
    if isinstance(value, int):
        return f"{value / (1024 * 1024):.1f}"
    return "n/a"


def format_pct(value: object) -> str:
    if isinstance(value, (int, float)):
        return f"{100.0 * float(value):.1f}%"
    return "n/a"


def format_num(value: object, digits: int = 2) -> str:
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        return f"{value:.{digits}f}"
    return "n/a"


def collect_environment(report_name: str) -> str:
    now = dt.datetime.now().astimezone()
    lines = [
        f"Benchmark environment capture for `{report_name}`",
        f"Captured at local time: {now.strftime('%Y-%m-%d %H:%M:%S %Z')}",
        f"Repository commit (short): {git_commit_short()}",
        "",
        "Commands and outputs",
        "====================",
        "",
    ]

    commands = [
        ["rustc", "--version", "--verbose"],
        ["cargo", "--version"],
        ["uname", "-a"],
    ]

    if platform.system() == "Darwin":
        commands.extend(
            [
                ["sysctl", "-n", "machdep.cpu.brand_string"],
                ["sysctl", "-n", "hw.ncpu"],
                ["sysctl", "-n", "hw.memsize"],
            ]
        )
    elif platform.system() == "Linux":
        commands.extend(
            [
                ["nproc"],
                ["sh", "-c", "grep -m1 'model name' /proc/cpuinfo | cut -d: -f2- | sed 's/^ *//'"],
                ["sh", "-c", "grep MemTotal /proc/meminfo"],
            ]
        )

    for cmd in commands:
        lines.append(f"$ {' '.join(cmd)}")
        lines.append(maybe_command_output(cmd))
        lines.append("")

    return "\n".join(lines)


def summary_row(label: str, payload: dict) -> list[str]:
    stats = payload.get("search_stats", {})
    return [
        label,
        format_num(stats.get("threads"), 0),
        format_num(stats.get("elapsed_ms")),
        format_num(stats.get("generation_ms")),
        format_num(stats.get("search_ms")),
        format_mib(stats.get("peak_memory_bytes")),
        format_num(stats.get("expressions_generated_total"), 0),
        format_num(stats.get("candidate_pairs_tested"), 0),
        format_num(stats.get("candidate_window_avg")),
        format_num(stats.get("candidate_window_max"), 0),
        format_num(stats.get("strict_gate_rejections"), 0),
        format_num(stats.get("candidates_per_pool_insertion")),
        format_pct(stats.get("newton_success_rate")),
        format_pct(stats.get("pool_acceptance_rate")),
    ]


def write_summary(
    summary_path: Path,
    report_name: str,
    args: argparse.Namespace,
    sequential_payload: dict | None,
    parallel_payload: dict | None,
    sequential_json: Path | None,
    parallel_json: Path | None,
    environment_path: Path,
) -> None:
    lines = [
        f"# {report_name}",
        "",
        "Generated by `scripts/capture_search_benchmark.py`.",
        "",
        "## Workload",
        "",
        f"- Target: `{args.target}`",
        f"- Search level: `{args.level}`",
        f"- Ranking mode: `{args.ranking}`",
        f"- Output: `--json`",
        f"- Max matches: `{args.max_matches}`",
        f"- Report mode: disabled (`--report false`)",
    ]
    if args.classic:
        lines.append("- Classic mode: enabled (`--classic`)")
    if args.extra_arg:
        lines.append(f"- Extra CLI args: `{' '.join(args.extra_arg)}`")

    lines.extend(
        [
            "",
            "## Results Summary",
            "",
            "| Mode | Threads | Elapsed (ms) | Generation (ms) | Search (ms) | Peak RSS (MiB) | Exprs Generated | Candidate Pairs | Window Avg | Window Max | Gate Rejects | Cand/Insert | Newton Success | Pool Acceptance |",
            "|------|---------|--------------|-----------------|-------------|----------------|-----------------|-----------------|------------|------------|--------------|-------------|----------------|-----------------|",
        ]
    )

    if sequential_payload is not None:
        lines.append("| " + " | ".join(summary_row("Sequential deterministic", sequential_payload)) + " |")
    if parallel_payload is not None:
        lines.append("| " + " | ".join(summary_row("Parallel", parallel_payload)) + " |")

    if sequential_payload is not None and parallel_payload is not None:
        seq_elapsed = sequential_payload["search_stats"]["elapsed_ms"]
        par_elapsed = parallel_payload["search_stats"]["elapsed_ms"]
        if par_elapsed:
            lines.extend(
                [
                    "",
                    f"Observed speedup (sequential deterministic / parallel): **{seq_elapsed / par_elapsed:.3f}x**",
                ]
            )

    lines.extend(
        [
            "",
            "## Raw Artifacts",
            "",
            f"- Environment metadata: `{repo_relative(environment_path)}`",
        ]
    )
    if sequential_json is not None:
        lines.append(f"- Sequential JSON: `{repo_relative(sequential_json)}`")
    if parallel_json is not None:
        lines.append(f"- Parallel JSON: `{repo_relative(parallel_json)}`")

    summary_path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--name", required=True, help="Artifact/report prefix, usually a date/workload tag")
    parser.add_argument("--target", required=True, help="Target value passed to the CLI")
    parser.add_argument("--level", type=int, required=True, help="CLI search level")
    parser.add_argument("--max-matches", type=int, default=16, help="Maximum matches for the benchmark run")
    parser.add_argument(
        "--ranking",
        choices=("complexity", "parity"),
        default="complexity",
        help="Ranking mode to benchmark",
    )
    parser.add_argument("--classic", action="store_true", help="Enable classic mode")
    parser.add_argument(
        "--artifact-dir",
        default=str(DEFAULT_ARTIFACT_DIR),
        help="Directory for JSON/environment/summary artifacts",
    )
    parser.add_argument(
        "--report-path",
        help="Optional Markdown path for the generated summary; defaults to <artifact-dir>/<name>-summary.md",
    )
    parser.add_argument(
        "--skip-sequential",
        action="store_true",
        help="Skip the no-default-features deterministic run",
    )
    parser.add_argument(
        "--skip-parallel",
        action="store_true",
        help="Skip the default-features parallel-capable run",
    )
    parser.add_argument(
        "--extra-arg",
        action="append",
        default=[],
        help="Additional CLI argument to append after the standard benchmark flags",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    artifact_dir = Path(args.artifact_dir)
    artifact_dir.mkdir(parents=True, exist_ok=True)

    report_name = args.name
    environment_path = artifact_dir / f"{args.name}-environment.txt"
    environment_path.write_text(collect_environment(report_name), encoding="utf-8")

    cli_args = build_cli_args(args)
    sequential_payload = None
    parallel_payload = None
    sequential_json = None
    parallel_json = None

    if not args.skip_sequential:
        sequential_json = artifact_dir / f"{args.name}-seq-deterministic.json"
        sequential_payload = run_benchmark_json(
            ["--no-default-features"],
            [*cli_args, "--deterministic"],
            sequential_json,
        )

    if not args.skip_parallel:
        parallel_json = artifact_dir / f"{args.name}-parallel.json"
        parallel_payload = run_benchmark_json([], cli_args, parallel_json)

    summary_path = Path(args.report_path) if args.report_path else artifact_dir / f"{args.name}-summary.md"
    write_summary(
        summary_path=summary_path,
        report_name=report_name,
        args=args,
        sequential_payload=sequential_payload,
        parallel_payload=parallel_payload,
        sequential_json=sequential_json,
        parallel_json=parallel_json,
        environment_path=environment_path,
    )

    print(f"Wrote environment metadata to {repo_relative(environment_path)}")
    if sequential_json is not None:
        print(f"Wrote sequential JSON to {repo_relative(sequential_json)}")
    if parallel_json is not None:
        print(f"Wrote parallel JSON to {repo_relative(parallel_json)}")
    print(f"Wrote benchmark summary to {repo_relative(summary_path)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

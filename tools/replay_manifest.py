#!/usr/bin/env python3
"""
RIES-RS Manifest Replay Tool
----------------------------
Reads a generated JSON manifest from `ries-rs --emit-manifest` and perfectly
replays the execution arguments against the ries-rs binary.

Usage:
  python3 tools/replay_manifest.py <path/to/manifest.json> [--bin <path/to/ries-rs>]
"""

import argparse
import json
import os
import subprocess
import sys
import tempfile
from typing import Dict, Any, List


def parse_args():
    parser = argparse.ArgumentParser(description="Replay a RIES-RS manifest")
    parser.add_argument("manifest", help="Path to the manifest.json file")
    parser.add_argument(
        "--bin",
        default="cargo run --release --",
        help="Path or command to the ries-rs binary (default: 'cargo run --release --')"
    )
    return parser.parse_args()


def build_command_args(config: Dict[str, Any]) -> List[str]:
    """Reconstruct CLI arguments from the manifest config block."""
    args = []

    # Target always goes first
    args.append(str(config["target"]))

    # Core parameters
    args.extend(["-l", str(int(config.get("level", 2)))])
    args.extend(["--max-matches", str(config.get("max_matches", 16))])
    args.extend(["--max-error", str(config.get("max_error", 0.01))])

    # Ranking
    ranking = config.get("ranking_mode", "complexity")
    if ranking == "parity":
        args.append("--parity-ranking")
    else:
        args.append("--complexity-ranking")

    # Features
    if config.get("deterministic", False):
        args.append("--deterministic")

    if not config.get("parallel", True):
        args.append("--no-parallel")

    # Exclusions
    for sym in config.get("excluded_symbols", []):
        args.extend(["-N", sym])

    # Allowlist
    allowed = config.get("allowed_symbols")
    if allowed is not None:
        args.extend(["-S", "".join(allowed)])

    return args


def check_parity(original_results: List[Dict], replayed_results: List[Dict]) -> bool:
    """Compare the primary match properties for reproduction success."""
    if len(original_results) != len(replayed_results):
        print(f"❌ Match count mismatch! Original: {len(original_results)}, Replayed: {len(replayed_results)}")
        return False

    success = True
    for i, (orig, repl) in enumerate(zip(original_results, replayed_results)):
        # Check symbolic identity
        if orig["lhs_postfix"] != repl["lhs_postfix"] or orig["rhs_postfix"] != repl["rhs_postfix"]:
            print(f"❌ Index {i} symbolic mismatch.")
            print(f"   Original: {orig['lhs_postfix']} = {orig['rhs_postfix']}")
            print(f"   Replayed: {repl['lhs_postfix']} = {repl['rhs_postfix']}")
            success = False

        # Numeric bounds
        err_diff = abs(orig["error"] - repl["error"])
        if err_diff > 1e-12:
            print(f"❌ Index {i} error margin differs by {err_diff:e}")
            success = False

    return success


def main():
    args = parse_args()

    if not os.path.isfile(args.manifest):
        print(f"Error: Manifest file '{args.manifest}' not found.", file=sys.stderr)
        sys.exit(1)

    with open(args.manifest, "r") as f:
        try:
            manifest = json.load(f)
        except json.JSONDecodeError as e:
            print(f"Error parsing manifest: {e}", file=sys.stderr)
            sys.exit(1)

    config = manifest.get("config")
    if not config:
        print("Invalid manifest: missing 'config' block.", file=sys.stderr)
        sys.exit(1)

    print(f"Replaying manifest generated on {manifest.get('timestamp', 'Unknown')}")
    print(f"Target: {config['target']}, Level: {config['level']}")

    # Create temporary file for the new manifest
    fd, temp_path = tempfile.mkstemp(suffix=".json")
    os.close(fd)

    try:
        cmd_args = build_command_args(config)
        # Using shell=True for 'cargo run --release --'
        # If user passed a single binary path, we could use a list.
        base_cmd = args.bin.split()
        full_cmd = base_cmd + cmd_args + ["--emit-manifest", temp_path]

        print(f"Executing: {' '.join(full_cmd)}")
        
        # Run the command and capture output so it doesn't pollute the terminal
        result = subprocess.run(
            full_cmd, 
            stdout=subprocess.PIPE, 
            stderr=subprocess.PIPE,
            text=True
        )

        if result.returncode != 0:
            print("❌ Execution failed!", file=sys.stderr)
            print(result.stderr, file=sys.stderr)
            sys.exit(1)

        # Parse the new manifest
        with open(temp_path, "r") as f:
            new_manifest = json.load(f)

        is_parity = check_parity(
            manifest.get("results", []), 
            new_manifest.get("results", [])
        )

        if is_parity:
            print(f"✅ Successfully reproduced {len(new_manifest['results'])} matches!")
            sys.exit(0)
        else:
            print("❌ Reproduction failed. Outputs diverged.")
            sys.exit(1)

    finally:
        # Cleanup
        if os.path.exists(temp_path):
            os.remove(temp_path)


if __name__ == "__main__":
    main()

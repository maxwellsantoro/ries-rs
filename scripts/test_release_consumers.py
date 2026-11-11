#!/usr/bin/env python3
"""Build and smoke-test installed artifacts from an immutable clean RC checkout.

Writes logs and a commit/tree-bound receipt outside the checkout. Never tags or
publishes. Requires Cargo, nightly wasm-pack prerequisites, npm/Chromium, and uv.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.is_relative_to(ROOT):
        parser.error("output must be outside the source checkout")
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT):
        parser.error("consumer validation requires a clean committed checkout")
    if output.exists() and any(output.iterdir()):
        parser.error("a fresh empty output directory is required; preserve previous receipts")
    output.mkdir(parents=True, exist_ok=True)
    receipt = {"schema": "ries-installed-consumers-v1",
               "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
               "git_tree": subprocess.check_output(["git", "rev-parse", "HEAD^{tree}"], cwd=ROOT, text=True).strip(),
               "commands": [], "passed": False}

    def run(command, *, cwd=ROOT, env=None, ok=(0,)):
        print("Consumer check:", command[0], " ".join(map(str, command[1:3])), flush=True)
        result = subprocess.run(list(map(str, command)), cwd=cwd, env=env, text=True, capture_output=True)
        index = len(receipt["commands"])
        (output / f"{index:02d}.stdout").write_text(result.stdout)
        (output / f"{index:02d}.stderr").write_text(result.stderr)
        receipt["commands"].append({"argv": list(map(str, command)), "returncode": result.returncode})
        if result.returncode not in ok:
            raise RuntimeError(f"Consumer check failed: {command}; see {output}/{index:02d}.stderr")
        return result

    def sha(path):
        return hashlib.sha256(path.read_bytes()).hexdigest()

    try:
        run(["cargo", "package", "--locked", "--target-dir", output / "build-package"])
        metadata = json.loads(run(["cargo", "metadata", "--no-deps", "--format-version=1"]).stdout)
        version = next(p["version"] for p in metadata["packages"] if p["name"] == "ries")
        crate = output / "build-package/package" / f"ries-{version}.crate"
        shutil.copyfile(crate, output / crate.name)
        extracted = output / "crate"
        extracted.mkdir(exist_ok=True)
        run(["tar", "-xzf", output / crate.name, "-C", extracted])
        run(["cargo", "install", "--path", extracted / f"ries-{version}", "--locked", "--root", output / "cli", "--target-dir", output / "build-cli", "--force"])
        cli = output / "cli/bin" / ("ries-rs.exe" if os.name == "nt" else "ries-rs")
        receipt["cli_sha256"] = sha(cli)
        literal = "1.000000000000000000000000000000"
        manifest = output / "installed-manifest.json"
        result = run([cli, literal, "--classic", "-l0", "-Sx1", "--match-all-digits", "--precision", "53", "--allow-unverified", "--json", "--emit-manifest", manifest])
        assert json.loads(result.stdout)["high_precision_verification"]["results"][0]["status"] == "unavailable"
        assert json.loads(manifest.read_text())["config"]["target_literal"] == literal
        run([sys.executable, ROOT / "tools/replay_manifest.py", manifest, "--bin", cli])
        for target, flags in [("1e-400", []), ("1.000000000000000000000000000001", ["--match-all-digits"]), ("1.000000000000001", ["--match-all-digits"])]:
            rejected = run([cli, target, "--pslq", "--pslq-max-coeff", "10", *flags], ok=(2,))
            assert not rejected.stdout

        env = dict(os.environ, CARGO_TARGET_DIR=str(output / "build-python"), RIES_PY_TEST_VENV=str(output / "python-env"), RIES_PY_TEST_DIST=str(output / "wheels"))
        run(["bash", ROOT / "scripts/test_ries_py_python.sh", "-q"], env=env)
        python = output / "python-env/bin/python"
        run([python, "-I", "-c", "import ries_rs; assert ries_rs.search(2, level=0, max_matches=1); print(ries_rs.__file__)"])
        receipt["wheel_sha256"] = {p.name: sha(p) for p in (output / "wheels").glob("*.whl")}

        run(["npm", "ci", "--no-fund", "--no-audit", "--cache", output / "npm-cache"])
        run(["npm", "run", "build:all"], env=dict(os.environ, CARGO_TARGET_DIR=str(output / "build-wasm")))
        package = json.loads(run(["npm", "pack", "--ignore-scripts", "--json", "--pack-destination", output]).stdout)[0]
        tarball = output / package["filename"]
        receipt["npm_sha256"] = sha(tarball)
        consumer = output / "npm-consumer"
        run(["npm", "install", "--ignore-scripts", "--no-fund", "--no-audit", "--prefix", consumer, tarball])
        run(["node", "-e", "const m=require('ries-rs'); if(!m.search(2,{level:0,maxMatches:1}).length) throw Error('no results'); console.log(m.version())"], cwd=consumer)
        run(["npm", "run", "build:css"])
        run(["bash", ROOT / "scripts/build_web_site.sh"])
        packaged_wasm = consumer / "node_modules/ries-rs/pkg"
        site_wasm = ROOT / "dist/web-site/pkg"
        shutil.rmtree(site_wasm)
        shutil.copytree(packaged_wasm, site_wasm)
        receipt["browser_wasm_sha256"] = sha(site_wasm / "ries_rs_bg.wasm")
        assert receipt["browser_wasm_sha256"] == sha(packaged_wasm / "ries_rs_bg.wasm")
        run(["npm", "run", "test:web:smoke"])
        assert not subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)
        receipt["passed"] = True
    finally:
        (output / "consumer-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print("Installed CLI, wheel, npm/browser, and exact manifest replay passed:", receipt["commit"])


if __name__ == "__main__":
    main()

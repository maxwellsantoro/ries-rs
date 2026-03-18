#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
CANONICAL_PROJECT_URL = "https://maxwellsantoro.com/projects/ries-rs"


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


def load_toml(path: Path) -> dict:
    with path.open("rb") as handle:
        return tomllib.load(handle)


def load_json(path: Path) -> dict:
    return json.loads(read_text(path))


def capture_required(pattern: str, text: str, label: str) -> str:
    match = re.search(pattern, text, flags=re.MULTILINE)
    if not match:
        raise ValueError(f"Missing expected {label}.")
    return match.group(1).strip()


def local_markdown_images(markdown: str) -> list[str]:
    paths: list[str] = []
    for target in re.findall(r"!\[[^\]]*\]\(([^)]+)\)", markdown):
        cleaned = target.strip()
        if cleaned.startswith(("http://", "https://", "#")):
            continue
        paths.append(cleaned)
    return paths


def main() -> int:
    cargo_toml = load_toml(ROOT / "Cargo.toml")
    package_json = load_json(ROOT / "package.json")
    package_lock_path = ROOT / "package-lock.json"
    package_lock = load_json(package_lock_path) if package_lock_path.is_file() else None
    ries_py_cargo = load_toml(ROOT / "ries-py" / "Cargo.toml")
    ries_py_project = load_toml(ROOT / "ries-py" / "pyproject.toml")
    citation_text = read_text(ROOT / "CITATION.cff")
    readme_text = read_text(ROOT / "README.md")

    expected_version = cargo_toml["package"]["version"]
    versions = {
        "Cargo.toml": expected_version,
        "package.json": package_json["version"],
        "ries-py/Cargo.toml": ries_py_cargo["package"]["version"],
        "ries-py/Cargo.toml dependency": ries_py_cargo["dependencies"]["ries_core"]["version"],
        "ries-py/pyproject.toml": ries_py_project["project"]["version"],
        "CITATION.cff": capture_required(r'^version:\s*"?([^"\n]+)"?\s*$', citation_text, "CITATION version"),
        "README BibTeX": capture_required(
            r"version\s*=\s*\{([^}]+)\}",
            readme_text,
            "README BibTeX version",
        ),
    }
    if package_lock is not None:
        versions["package-lock.json"] = package_lock["version"]
        versions['package-lock.json packages[""]'] = package_lock["packages"][""]["version"]

    homepage_checks = {
        "Cargo.toml homepage": cargo_toml["package"]["homepage"],
        "package.json homepage": package_json["homepage"],
        "ries-py/Cargo.toml homepage": ries_py_cargo["package"]["homepage"],
        "ries-py/pyproject.toml Homepage": ries_py_project["project"]["urls"]["Homepage"],
        "CITATION.cff url": capture_required(r'^url:\s*"([^"\n]+)"\s*$', citation_text, "CITATION url"),
        "README BibTeX url": capture_required(
            r"url\s*=\s*\{([^}]+)\}",
            readme_text,
            "README BibTeX url",
        ),
    }

    errors: list[str] = []

    for label, version in versions.items():
        if version != expected_version:
            errors.append(
                f"{label} has version {version}, expected {expected_version}."
            )

    for label, url in homepage_checks.items():
        if url != CANONICAL_PROJECT_URL:
            errors.append(
                f"{label} is {url}, expected {CANONICAL_PROJECT_URL}."
            )

    release_notes_path = ROOT / "docs" / "releases" / f"v{expected_version}.md"
    if not release_notes_path.is_file():
        errors.append(
            f"Missing release notes file {release_notes_path.relative_to(ROOT)}."
        )

    for target in local_markdown_images(readme_text):
        path = (ROOT / target).resolve()
        try:
            relative = path.relative_to(ROOT)
        except ValueError:
            errors.append(f"README image target {target} resolves outside the repository.")
            continue
        if not path.is_file():
            errors.append(f"README image target {relative} does not exist.")

    if errors:
        print("Release integrity check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"Release integrity check passed for version {expected_version}.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

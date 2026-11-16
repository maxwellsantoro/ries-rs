#!/usr/bin/env python3
"""Check local link targets in active Markdown docs, without network requests.

Historical archives, release notes, benchmark evidence and reviews are excluded.
Checks inline links/images and reference definitions; does not validate remote
URLs, Markdown anchors, or resolve reference uses. Fenced code is ignored.
"""
from __future__ import annotations

import argparse
from pathlib import Path, PurePosixPath
import re
import subprocess
from urllib.parse import unquote, urlsplit


ROOT = Path(__file__).resolve().parents[1]


def is_active_doc(name: str) -> bool:
    path = PurePosixPath(name)
    if path.suffix.lower() != ".md":
        return False
    return (
        len(path.parts) == 1
        or (len(path.parts) == 2 and path.parts[0] in {"docs", ".github"})
        or name in {
            "tests/README.md", "web/README.md", "web/vendor/README.md",
            "ries-py/PYPI.md", "docs/benchmarks/README.md",
        }
        or (name.startswith(".agents/skills/") and path.name == "SKILL.md")
    )


def active_docs(root: Path) -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        cwd=root, check=True, stdout=subprocess.PIPE,
    )
    names = set(result.stdout.decode("utf-8", errors="surrogateescape").split("\0"))
    return [root / name for name in sorted(names) if is_active_doc(name) and (root / name).is_file()]


def destination(text: str, start: int) -> str | None:
    """Read a Markdown destination after ]( or a reference definition's colon."""
    text = text[start:].lstrip()
    if text.startswith("<"):
        close = re.search(r"(?<!\\)>", text[1:])
        return text[1:close.start() + 1] if close else None
    depth = 0
    escaped = False
    for index, char in enumerate(text):
        if escaped:
            escaped = False
            continue
        if char == "\\":
            escaped = True
        elif char == "(":
            depth += 1
        elif char == ")":
            if depth == 0:
                return text[:index]
            depth -= 1
        elif char.isspace() and depth == 0:
            return text[:index]
    return text or None


def link_targets(markdown: str):
    fence_char = None
    fence_length = 0
    for line_number, line in enumerate(markdown.splitlines(), 1):
        fence = re.match(r"^ {0,3}(`{3,}|~{3,})(.*)$", line)
        if fence_char:
            if (fence and fence.group(1)[0] == fence_char
                    and len(fence.group(1)) >= fence_length and not fence.group(2).strip()):
                fence_char = None
            continue
        if fence:
            fence_char, fence_length = fence.group(1)[0], len(fence.group(1))
            continue
        # Inline code may contain illustrative Markdown which is not a link.
        line = re.sub(r"(`+).*?\1", "", line)
        reference = re.match(r"^ {0,3}\[[^\]]+\]:\s*", line)
        if reference:
            target = destination(line, reference.end())
            if target:
                yield line_number, target
        for match in re.finditer(r"(?<!\\)\]\(", line):
            target = destination(line, match.end())
            if target:
                yield line_number, target


def check_file(root: Path, source: Path) -> list[str]:
    errors = []
    for line, target in link_targets(source.read_text(encoding="utf-8")):
        # Markdown escapes are separate from URL percent-encoding.
        target = re.sub(r"\\([\\`*{}\[\]()#+.!_<> -])", r"\1", target)
        try:
            url = urlsplit(target)
        except ValueError:
            errors.append(f"{source.relative_to(root)}:{line}: invalid link {target!r}")
            continue
        if url.scheme or url.netloc or not url.path:
            continue
        path = unquote(url.path)
        resolved = ((root / path.lstrip("/")) if path.startswith("/") else source.parent / path).resolve()
        if not resolved.is_relative_to(root) or not resolved.exists():
            errors.append(f"{source.relative_to(root)}:{line}: missing local target {target!r}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    args = parser.parse_args()
    root = args.root.resolve()
    try:
        docs = active_docs(root)
        errors = [error for source in docs for error in check_file(root, source)]
    except (OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Documentation check failed: {error}\n")
    if errors:
        print("\n".join(errors))
        return 1
    print(f"Checked local links in {len(docs)} active Markdown documents (no network or anchor checks).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

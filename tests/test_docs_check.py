"""Behavioral checks for the cheap active-documentation link gate."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "scripts/check_docs.py"
SPEC = importlib.util.spec_from_file_location("check_docs", SCRIPT)
DOCS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(DOCS)


class DocumentationLinks(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name).resolve()
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)

    def write(self, name, content=""):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        return path

    def test_relative_encoded_directory_image_and_reference_targets(self):
        self.write("docs/data/my file(1).png")
        self.write("docs/other.md")
        source = self.write("docs/README.md", "\n".join([
            "![Image](data/my%20file(1).png)",
            "[Other](other.md#unknown-anchor)",
            "[Directory](data/)",
            '[picture]: <data/my file(1).png> "Image title"',
            "[Root](/docs/other.md)",
            "[Escaped](data/my\\ file\\(1\\).png)",
        ]))
        self.assertEqual(DOCS.check_file(self.root, source), [])

    def test_missing_and_outside_targets_report_source_lines(self):
        source = self.write("README.md", "Title\n[Missing](absent.md)\n[Outside](../outside.md)\n[ref]: absent.png\n")
        errors = DOCS.check_file(self.root, source)
        self.assertEqual(len(errors), 3)
        self.assertTrue(errors[0].startswith("README.md:2:"))
        self.assertIn("absent.md", errors[0])
        self.assertTrue(errors[1].startswith("README.md:3:"))
        self.assertTrue(errors[2].startswith("README.md:4:"))

    def test_external_urls_anchors_and_code_examples_are_ignored(self):
        source = self.write("README.md", "\n".join([
            "[Web](https://example.com/missing.md)",
            "[Email](mailto:person@example.com)",
            "[Anchor](#unknown)",
            "[CDN](//example.com/missing.png)",
            "`[Inline example](missing.md)`",
            "````markdown", "[Example](missing.md)", "```", "[Still code](missing.md)", "````",
            "~~~", "[Example](missing.md)", "~~~",
        ]))
        self.assertEqual(DOCS.check_file(self.root, source), [])

    def test_inventory_checks_active_untracked_docs_but_preserves_history(self):
        self.write("README.md", "[Missing](missing.md)")
        subprocess.run(["git", "add", "README.md"], cwd=self.root, check=True)
        self.write("docs/ROADMAP.md", "[Missing](missing.md)")
        self.write(".agents/skills/ries-delivery/SKILL.md")
        self.write("docs/archive/plans/old.md", "[Missing](missing.md)")
        self.write("docs/releases/v1.md", "[Missing](missing.md)")
        self.write("docs/benchmarks/result.md", "[Missing](missing.md)")
        self.write("docs/benchmarks/README.md")
        self.write("reviews/old.md", "[Missing](missing.md)")
        self.write(".gitignore", "ignored.md\n")
        self.write("ignored.md", "[Missing](missing.md)")
        names = {path.relative_to(self.root).as_posix() for path in DOCS.active_docs(self.root)}
        self.assertEqual(names, {
            "README.md", "docs/ROADMAP.md", "docs/benchmarks/README.md",
            ".agents/skills/ries-delivery/SKILL.md",
        })
        result = subprocess.run(["python3", str(SCRIPT), "--root", str(self.root)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn("README.md:1:", result.stdout)
        self.assertIn("docs/ROADMAP.md:1:", result.stdout)
        self.assertNotIn("archive", result.stdout)


if __name__ == "__main__":
    unittest.main()

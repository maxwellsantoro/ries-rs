"""Behavioral regressions for changed-path CI routing and fail-open behavior."""

import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("ci_scope", ROOT / "scripts/ci_scope.py")
scope = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(scope)


class ScopeTests(unittest.TestCase):
    def selected(self, *paths):
        return {lane for lane, enabled in scope.classify(paths).items() if enabled}

    def test_documentation_only_skips_executable_lanes(self):
        self.assertEqual(self.selected("README.md", "AGENTS.md", "docs/assets/web-ui.png",
                                       ".agents/skills/ries-delivery/SKILL.md",
                                       "docs/archive/artifacts/old-debug.mjs"), set())

    def test_unknown_executable_or_configuration_under_docs_fails_open(self):
        for path in ("docs/new-tool.py", "docs/assets/tool.py", "docs/new-config.json"):
            with self.subTest(path=path):
                self.assertEqual(self.selected(path), set(scope.LANES))

    def test_ui_only_changes_do_not_rebuild_installed_consumers(self):
        self.assertEqual(self.selected("web/index.html", "web/search-worker.js", "web/styles.css",
                                       "web/vendor/katex/katex.min.js", "tests/web-smoke.spec.ts",
                                       "playwright.config.ts"), {"web"})

    def test_package_inputs_keep_installed_consumer_validation(self):
        for path in ("package.json", "package-lock.json", "scripts/build_web_site.sh",
                     "scripts/build_web_threads.sh", "tests/wasm-package.mjs"):
            with self.subTest(path=path):
                self.assertEqual(self.selected(path), {"web", "consumers"})

    def test_always_run_tool_checks_do_not_rebuild_engine(self):
        self.assertEqual(self.selected("scripts/check_docs.py", "tests/test_docs_check.py",
                                       "tests/test_review_tools.py"), set())

    def test_mixed_changes_union_surface_requirements(self):
        self.assertEqual(self.selected("web/index.html", "ries-py/src/lib.rs", "README.md"),
                         {"web", "python", "features", "consumers"})

    def test_shared_and_unknown_inputs_fail_open(self):
        for path in ("src/report.rs", "Cargo.lock", ".github/workflows/ci.yml",
                     "tests/numeric_contract.rs", "tests/web-future.rs", "future-engine/config.json"):
            with self.subTest(path=path):
                self.assertEqual(self.selected(path), set(scope.LANES))

    def test_binding_dependency_changes_include_audit(self):
        self.assertIn("audit", self.selected("ries-py/Cargo.lock"))

    def test_qualification_and_packaging_tools_select_their_consumers(self):
        self.assertEqual(self.selected("scripts/qualify_search.py", "tests/corpus/search-contract.json"),
                         {"qualification"})
        self.assertEqual(self.selected("scripts/test_release_consumers.py"), {"consumers"})
        self.assertEqual(self.selected("scripts/qualify_discovery.py", "scripts/verify_discovery.py",
                                       "tests/corpus/discovery-quality.json"), {"qualification"})

    def test_unknown_corpus_fixtures_fail_open(self):
        for path in ("tests/corpus/future.json", "tests/corpus/fixture.md", "tests/corpus/fixture.py"):
            with self.subTest(path=path):
                self.assertEqual(self.selected(path), set(scope.LANES))

    def test_non_pr_events_always_use_full_matrix(self):
        for event in ("push", "workflow_call", "workflow_dispatch", "schedule", "local"):
            with self.subTest(event=event):
                result, _ = scope.determine_scope(event, "missing.json")
                self.assertTrue(all(result.values()))

    def test_missing_malformed_or_unresolvable_pr_metadata_fails_open(self):
        with tempfile.TemporaryDirectory() as directory:
            event = Path(directory) / "event.json"
            for content in ("", "{}", '{"pull_request": null}',
                            json.dumps({"pull_request": {"base": {"sha": "--bad"}, "head": {"sha": "x"}}})):
                event.write_text(content)
                result, _ = scope.determine_scope("pull_request", event)
                self.assertTrue(all(result.values()))
            event.write_text(json.dumps({"pull_request": {"base": {"sha": "a" * 40},
                                                           "head": {"sha": "b" * 40}}}))
            with patch.object(scope, "changed_paths", side_effect=subprocess.CalledProcessError(1, "git")):
                result, _ = scope.determine_scope("pull_request", event)
                self.assertTrue(all(result.values()))

    def test_diff_uses_merge_base_and_preserves_rename_sources_and_unusual_names(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def git(*args):
                return subprocess.check_output(["git", *args], cwd=root, text=True).strip()
            git("init", "-q")
            git("config", "user.name", "CI test")
            git("config", "user.email", "ci@example.invalid")
            (root / "source.rs").write_text("source")
            git("add", ".")
            git("commit", "-qm", "initial")
            ancestor = git("rev-parse", "HEAD")
            git("checkout", "-qb", "feature")
            (root / "docs").mkdir()
            git("mv", "source.rs", "docs/source.md")
            strange = "line\nbreak $(echo nope).md"
            (root / strange).write_text("documentation")
            git("add", ".")
            git("commit", "-qm", "rename")
            head = git("rev-parse", "HEAD")
            git("checkout", "-qb", "base", ancestor)
            (root / "base-only.rs").write_text("base branch change")
            git("add", ".")
            git("commit", "-qm", "base advanced")
            base = git("rev-parse", "HEAD")
            paths = scope.changed_paths(base, head, cwd=root)
            self.assertEqual(set(paths), {"source.rs", "docs/source.md", strange})
            self.assertEqual({lane for lane, enabled in scope.classify(paths).items() if enabled},
                             set(scope.LANES))

    def job_results(self, selected):
        result = {job: {"result": "success" if lane in selected else "skipped"}
                  for job, lane in scope.JOB_LANES.items()}
        result["scope"] = {"result": "success", "outputs": {
            lane: str(lane in selected).lower() for lane in scope.LANES}}
        result["release-integrity"] = {"result": "success"}
        return result

    def test_aggregate_accepts_only_successful_selected_and_intentionally_skipped_jobs(self):
        for selected in (set(), {"web", "consumers"}, set(scope.LANES)):
            scope.validate_results(self.job_results(selected))

    def test_aggregate_rejects_failed_cancelled_missing_or_unexpected_skipped_jobs(self):
        for job in ("scope", "release-integrity", "test-web"):
            for status in ("failure", "cancelled", "skipped", None):
                with self.subTest(job=job, status=status):
                    needs = self.job_results({"web"})
                    needs[job]["result"] = status
                    with self.assertRaises(ValueError):
                        scope.validate_results(needs)
        needs = self.job_results(set())
        needs["test"]["result"] = "failure"
        with self.assertRaises(ValueError):
            scope.validate_results(needs)
        needs = self.job_results(set())
        del needs["scope"]["outputs"]["rust"]
        with self.assertRaises(ValueError):
            scope.validate_results(needs)

    def test_coverage_gate_accepts_selected_success_and_intentional_skips(self):
        for selected, result in (("true", "success"), ("false", "success"), ("false", "skipped")):
            scope.validate_coverage_results({
                "scope": {"result": "success", "outputs": {"rust": selected}},
                "coverage": {"result": result},
            })

    def test_coverage_gate_rejects_scope_failures_and_invalid_outputs(self):
        for result in (None, "failure", "cancelled", "skipped"):
            with self.subTest(result=result):
                with self.assertRaises(ValueError):
                    scope.validate_coverage_results({
                        "scope": {"result": result, "outputs": {"rust": "false"}},
                        "coverage": {"result": "skipped"},
                    })
        for rust in (None, "", True, "unknown"):
            with self.subTest(rust=rust):
                with self.assertRaises(ValueError):
                    scope.validate_coverage_results({
                        "scope": {"result": "success", "outputs": {"rust": rust}},
                        "coverage": {"result": "success"},
                    })

    def test_coverage_gate_rejects_failed_cancelled_missing_or_unexpected_skipped_coverage(self):
        for selected in ("true", "false"):
            for result in (None, "failure", "cancelled", "unknown", "skipped"):
                if selected == "false" and result == "skipped":
                    continue
                with self.subTest(selected=selected, result=result):
                    with self.assertRaises(ValueError):
                        scope.validate_coverage_results({
                            "scope": {"result": "success", "outputs": {"rust": selected}},
                            "coverage": {"result": result},
                        })
if __name__ == "__main__":
    unittest.main()

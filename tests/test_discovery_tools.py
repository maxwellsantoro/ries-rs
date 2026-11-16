"""Independent counterexamples for discovery recovery metrics and evidence bindings."""

import copy
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from qualify_discovery import measure, validate_corpus
from verify_discovery import (digest, selected_evidence, structural_postfix_key,
                              unpack_selection, validate_run, verify_evidence)


def equation(lhs="x", rhs="2q", solved="2q", root=2**0.5, target=1.41421356):
    return {"lhs_postfix": lhs, "rhs_postfix": rhs, "solve_for_x_postfix": solved,
            "x_value": root, "error": root - target, "complexity": 37}


def hypothesis(key="sqrt2", row=None, support=None):
    return {"key": key, "representative": row or equation(), "supporting_equations": support or [],
            "within_input_precision": True, "validation": {"polynomial_certificate": {
                "status": "unavailable", "reason": "not computed by discovery"}}}


def run():
    return {"schema": "ries-discovery-v1", "engine_version": "test-fixture",
            "target_literal": "1.41421356", "target": 1.41421356,
            "config": {"input_tolerance": 5e-9, "max_matches": 8, "level": 3,
                       "preset": None, "constants": [], "selection_policy": "fixture"},
            "stats": {}, "hypotheses": [hypothesis(support=[equation("xs", "2")])]}


def case():
    return {"name": "sqrt2", "kind": "formula", "target_literal": "1.41421356",
            "options": {"level": 3, "max_matches": 8}, "expected_solved_postfix": ["2q"],
            "required_top_k": 1}


class RecoveryMetrics(unittest.TestCase):
    def test_required_answer_missing_or_wrong_first_fails(self):
        self.assertTrue(measure(case(), run())["passed"])
        missing = run()
        missing["hypotheses"] = []
        self.assertFalse(measure(case(), missing)["passed"])
        wrong = run()
        wrong["hypotheses"].insert(0, hypothesis("other", equation(rhs="3q", solved="3q")))
        result = measure(case(), wrong)
        self.assertFalse(result["passed"])
        self.assertEqual(result["known_answer_rank"], 2)
        self.assertFalse(result["top1_recovered"])
        self.assertTrue(result["top3_recovered"])

    def test_duplicate_keys_and_duplicate_presentations_are_measured_and_rejected(self):
        duplicate = run()
        duplicate["hypotheses"].append(copy.deepcopy(duplicate["hypotheses"][0]))
        result = measure(case(), duplicate)
        self.assertFalse(result["passed"])
        self.assertEqual(result["unique_hypotheses"], 1)
        self.assertEqual(result["duplicate_hypothesis_rate"], 0.5)
        duplicate["hypotheses"][1]["key"] = "different-key-same-displayed-root"
        result = measure(case(), duplicate)
        self.assertFalse(result["passed"])
        self.assertEqual(result["duplicate_displayed_rate"], 0.5)

    def test_precision_flags_cannot_claim_incompatible_root(self):
        changed = run()
        changed["hypotheses"][0]["within_input_precision"] = False
        self.assertFalse(measure(case(), changed)["passed"])

    def test_expected_spelling_without_input_precision_agreement_is_not_recovery(self):
        changed = run()
        changed["hypotheses"] = [hypothesis(row=equation(root=1.42))]
        changed["hypotheses"][0]["within_input_precision"] = False
        result = measure(case(), changed)
        self.assertFalse(result["passed"])
        self.assertIsNone(result["known_answer_rank"])
        self.assertEqual(result["precision_compatible_hypotheses"], 0)

    def test_controls_observe_formulas_without_false_positive_claims(self):
        control = dict(case(), kind="control")
        result = measure(control, run())
        self.assertTrue(result["passed"])
        self.assertIsNone(result["known_answer_rank"])
        self.assertIsNone(result["top1_recovered"])
        self.assertIsNone(result["topk_recovered"])

    def test_named_constant_recognition_uses_declared_slot_not_formula_recovery(self):
        data = run()
        data["config"]["constants"] = [["named", 2**0.5]]
        row = data["hypotheses"][0]["representative"]
        row["rhs_postfix"] = chr(128)
        row["solve_for_x_postfix"] = chr(128)
        data["hypotheses"][0]["supporting_equations"] = []
        named = dict(case(), kind="named_constant", expected_constant="named")
        self.assertEqual(measure(named, data)["known_answer_rank"], 1)

    def test_unknown_corpus_schema_and_unsafe_names_fail(self):
        for corpus in ({"schema": "unknown", "cases": [case()]},
                       {"schema": "ries-discovery-corpus-v1", "cases": [dict(case(), name="../escape")]}):
            with self.assertRaises(ValueError):
                validate_corpus(corpus)


class SelectedEvidence(unittest.TestCase):
    def test_representative_can_fall_back_to_supported_retained_equation(self):
        data = run()
        report = selected_evidence(data, 0)
        self.assertEqual(report["evidence"]["status"], "independently_bounded")
        self.assertEqual(report["evidence"]["source_equation_index"], 1)
        self.assertEqual(report["evidence"]["certificate"]["lhs_postfix"], "xs")
        self.assertEqual(verify_evidence(report), "independently_bounded")
        self.assertEqual(report["run"], data)
        self.assertEqual(report["target_literal"], data["target_literal"])

    def test_tampered_literal_configuration_certificate_or_selected_index_fails(self):
        original = selected_evidence(run(), 0)
        mutations = [
            lambda r: r["run"].update(target_literal="1.414213560"),
            lambda r: r["run"]["config"].update(selection_policy="changed"),
            lambda r: r["evidence"]["certificate"].update(target_literal="1.5"),
            lambda r: r["evidence"]["certificate"].update(coefficients=["-3", "0", "1"]),
            lambda r: r["evidence"]["certificate"].update(numerical_root="1"),
            lambda r: r.update(hypothesis_index=1),
            lambda r: r.update(hypothesis_key="different"),
            lambda r: r.update(schema="unknown"),
        ]
        for mutation in mutations:
            changed = copy.deepcopy(original)
            mutation(changed)
            with self.assertRaises(ValueError):
                verify_evidence(changed)

    def test_recomputed_hash_cannot_rebind_certificate_to_changed_equation(self):
        changed = selected_evidence(run(), 0)
        changed["run"]["hypotheses"][0]["supporting_equations"][0]["rhs_postfix"] = "3"
        changed["run_sha256"] = digest(changed["run"])
        with self.assertRaises(ValueError):
            verify_evidence(changed)

    def test_stale_polynomial_support_cannot_certify_a_different_selected_root(self):
        changed = run()
        h = changed["hypotheses"][0]
        h.update(key="solved:q(3)", representative=equation(rhs="3q", solved="3q", root=3**0.5),
                 within_input_precision=False)
        with self.assertRaisesRegex(ValueError, "different structural"):
            selected_evidence(changed, 0)

    def test_supporting_root_must_be_coherent_even_when_its_solved_text_is_unchanged(self):
        changed = run()
        row = changed["hypotheses"][0]["supporting_equations"][0]
        row.update(x_value=1.8, error=1.8 - changed["target"])
        with self.assertRaisesRegex(ValueError, "different numerical root"):
            selected_evidence(changed, 0)

    def test_structural_grouping_only_normalizes_safe_commutation_and_outer_double_negation(self):
        self.assertEqual(structural_postfix_key("23+nn", True), structural_postfix_key("32+", True))
        self.assertNotEqual(structural_postfix_key("2q"), structural_postfix_key("2qn"))
        self.assertNotEqual(structural_postfix_key("2rr"), structural_postfix_key("2"))
        negative = run()
        negative.update(target_literal="-1.41421356", target=-1.41421356)
        h = negative["hypotheses"][0]
        h["representative"] = equation(rhs="2qn", solved="2qn", root=-(2**0.5), target=negative["target"])
        h["supporting_equations"] = [equation("xs", "2", "2qnnn", -(2**0.5), negative["target"])]
        self.assertEqual(selected_evidence(negative, 0)["evidence"]["status"], "independently_bounded")

    def test_unsupported_grammar_is_explicitly_unavailable(self):
        data = run()
        data["hypotheses"][0]["supporting_equations"] = []
        data["hypotheses"][0]["representative"].update(rhs_postfix="p", solve_for_x_postfix="p")
        report = selected_evidence(data, 0)
        self.assertEqual(report["evidence"]["status"], "unavailable")
        self.assertIn("unsupported symbol", report["evidence"]["attempts"][0]["reason"])
        self.assertEqual(verify_evidence(report), "unavailable")

    def test_selection_wrapper_and_invalid_indices_or_run_schema(self):
        wrapper = {"schema": "ries-discovery-selection-v1", "run": run(), "hypothesis_index": 0}
        data, index = unpack_selection(wrapper, None)
        self.assertEqual(selected_evidence(data, index)["hypothesis_index"], 0)
        with self.assertRaises(ValueError):
            unpack_selection(wrapper, 1)
        for index in (-1, 1, True, None):
            with self.assertRaises(ValueError):
                selected_evidence(run(), index)
        invalid = run()
        invalid["schema"] = "unknown"
        with self.assertRaises(ValueError):
            validate_run(invalid)

    def test_input_precision_and_underflow_tampering_are_rejected(self):
        for literal, target, tolerance in (("1.000000000000000000000000000001", 1.0, 5e-31),
                                           ("1e-400", 0.0, 0.0)):
            data = run()
            data.update(target_literal=literal, target=target)
            data["config"]["input_tolerance"] = tolerance
            with self.assertRaises(ValueError):
                validate_run(data)


if __name__ == "__main__":
    unittest.main()

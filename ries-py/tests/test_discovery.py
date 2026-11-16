"""Installed-extension regressions for the additive decimal discovery API."""
import json
from threading import Event, Thread
from time import perf_counter

import pytest

import ries_rs


@pytest.mark.parametrize("literal,solved", [
    ("3.14159265", "x = pi"),
    ("1.41421356", "x = sqrt(2)"),
    ("1.64872127", "x = sqrt(e)"),
])
def test_discovery_recovers_simple_hypotheses_before_a_single_result_limit(literal, solved):
    run = ries_rs.discover(literal, level=3, max_matches=1)
    assert run["schema"] == "ries-discovery-v1"
    assert run["target_literal"] == literal
    assert len(run["hypotheses"]) == 1
    hypothesis = run["hypotheses"][0]
    assert hypothesis["representative"]["solve_for_x"] == solved
    assert hypothesis["within_input_precision"]
    assert run["config"]["effective_total_complexity_bound"] == hypothesis["representative"]["complexity"]


def test_discovery_preserves_literal_and_serializes_full_run_without_custom_objects():
    literal = " 1.4142135600 "
    run = ries_rs.discover(literal, level=3, max_matches=3, parallel=True)
    assert isinstance(run, dict)
    assert json.loads(json.dumps(run, allow_nan=False)) == run
    assert run["target_literal"] == literal
    assert run["config"]["input_tolerance"] == 5e-11
    assert run["config"]["parallel_requested"]
    assert run["config"]["max_lhs_complexity"] == 46
    assert run["stats"]["matcher"] in {"serial", "streaming"}
    assert run["stats"]["lhs_count"] > 0
    assert run["hypotheses"]
    for hypothesis in run["hypotheses"]:
        checks = hypothesis["validation"]
        assert checks["numerical"]["status"] == "numerical"
        assert checks["high_precision"]["status"] == "not_requested"
        assert checks["polynomial_certificate"]["status"] == "unavailable"
        assert all(check["reason"] for check in checks.values())


def test_discovery_unavailable_precision_and_raw_search_compatibility_are_explicit():
    run = ries_rs.discover("2", verification_bits=53)
    check = run["hypotheses"][0]["validation"]["high_precision"]
    assert check["status"] == "unavailable"
    assert check["precision_bits"] == 53
    assert check["root_error"] is None
    raw = ries_rs.search(2.0, level=0, max_matches=1, parallel=False)
    assert len(raw) == 1
    assert isinstance(raw[0], ries_rs.PyMatch)
    assert isinstance(raw[0].to_dict(), dict)
    assert not isinstance(raw[0], dict)


@pytest.mark.parametrize("literal", ["", "NaN", "inf", "1e-400", "1+2", "9007199254740993"])
def test_discovery_rejects_invalid_decimal_literals(literal):
    with pytest.raises(ValueError):
        ries_rs.discover(literal)


@pytest.mark.parametrize("options", [
    {"level": 6}, {"max_matches": 0}, {"max_matches": 1001},
    {"preset": "does-not-exist"}, {"verification_bits": 4097},
])
def test_discovery_rejects_unsupported_options(options):
    with pytest.raises(ValueError):
        ries_rs.discover("2", **options)


def test_discovery_requires_literal_text_and_rejects_unknown_keywords():
    with pytest.raises(TypeError):
        ries_rs.discover(3.14159265)
    with pytest.raises(TypeError):
        ries_rs.discover("2", made_up_option=True)


def test_discovery_search_releases_the_gil():
    stop = Event()
    ready = Event()
    ticks = []

    def heartbeat():
        ready.set()
        while not stop.wait(0.001):
            ticks.append(perf_counter())

    thread = Thread(target=heartbeat)
    thread.start()
    try:
        assert ready.wait(2)
        begin = perf_counter()
        ries_rs.discover("9.5124027528", level=5, max_matches=3)
        end = perf_counter()
        assert any(begin < tick < end for tick in ticks)
    finally:
        stop.set()
        thread.join()

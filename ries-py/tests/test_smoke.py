from threading import Event, Thread
from time import perf_counter

import ries_rs


def test_version_and_presets_are_available():
    version = ries_rs.version()
    presets = ries_rs.list_presets()

    assert isinstance(version, str)
    assert version
    assert presets


def test_search_returns_typed_matches():
    results = ries_rs.search(3.141592653589793, level=2, max_matches=3, parallel=False)

    assert results, "expected at least one match"

    first = results[0]
    payload = first.to_dict()

    assert isinstance(first.lhs, str)
    assert isinstance(first.rhs, str)
    assert "lhs" in payload
    assert "rhs" in payload
    assert "error" in payload


def test_search_rejects_non_finite_target():
    for bad_target in (float("nan"), float("inf"), float("-inf")):
        try:
            ries_rs.search(bad_target)
        except ValueError as exc:
            assert "finite" in str(exc).lower()
        else:
            raise AssertionError(f"expected ValueError for target={bad_target!r}")


def test_search_rejects_unknown_preset():
    try:
        ries_rs.search(3.141592653589793, preset="does-not-exist")
    except ValueError as exc:
        assert "Unknown preset" in str(exc)
    else:
        raise AssertionError("expected ValueError for unknown preset")


def test_search_allows_another_python_thread_to_run():
    stop = Event()
    ready = Event()
    ticks = []

    def heartbeat():
        ready.set()
        while not stop.wait(0.002):
            ticks.append(perf_counter())

    thread = Thread(target=heartbeat)
    thread.start()
    try:
        assert ready.wait(2)
        begin = perf_counter()
        ries_rs.search(9.5124027528, level=5, max_matches=16, parallel=False)
        end = perf_counter()
        assert any(begin < tick < end for tick in ticks)
    finally:
        stop.set()
        thread.join()

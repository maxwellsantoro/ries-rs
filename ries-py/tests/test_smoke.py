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


def test_search_rejects_unknown_preset():
    try:
        ries_rs.search(3.141592653589793, preset="does-not-exist")
    except ValueError as exc:
        assert "Unknown preset" in str(exc)
    else:
        raise AssertionError("expected ValueError for unknown preset")

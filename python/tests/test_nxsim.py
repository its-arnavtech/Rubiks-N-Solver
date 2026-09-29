"""M5.1: the native `nxsim` module (built with `just py-build`)."""

from pathlib import Path

import numpy as np
import pytest

nxsim = pytest.importorskip("nxsim")

LIBRARY = Path(__file__).resolve().parents[2] / "artifacts" / "macros" / "library.json"


def moves_array(b: bytes) -> np.ndarray:
    return np.frombuffer(b, dtype="<u4")


def test_version_and_sizes() -> None:
    assert nxsim.version()
    assert nxsim.sticker_count(5) == 150
    assert nxsim.is_solved(5, nxsim.solved(5))


def test_scramble_apply_verify_cancel() -> None:
    facelets, moves = nxsim.scramble(6, 80, 3)
    assert len(facelets) == 6 * 36
    assert len(moves_array(moves)) == 80
    assert not nxsim.is_solved(6, facelets)
    assert nxsim.apply_moves(6, nxsim.solved(6), moves) == facelets
    nxsim.validate(6, facelets)
    doubled = moves + moves[-4:]
    assert len(moves_array(nxsim.cancel(doubled))) <= len(moves_array(doubled))
    assert nxsim.format_moves(moves[:4]) != ""


def test_errors_raise_value_error() -> None:
    with pytest.raises(ValueError):
        nxsim.solved(1)
    with pytest.raises(ValueError):
        nxsim.apply_moves(3, b"\x00" * 53, b"")
    bad = bytearray(nxsim.solved(3))
    bad[0], bad[9] = bad[9], bad[0]
    with pytest.raises(ValueError):
        nxsim.validate(3, bytes(bad))


def test_orbits_and_extract() -> None:
    n = 7
    orbits = nxsim.orbits(n)
    assert [o[1] for o in orbits[:3]] == ["Corner", "MidEdge", "FixedCenter"]
    ids = np.frombuffer(nxsim.sticker_orbit(n), dtype="<u4")
    assert ids.shape == (6 * n * n,) and ids.max() == len(orbits) - 1
    groups = nxsim.extract(n, nxsim.solved(n))
    ids_x, contents_x = groups["XCenter"]
    x = np.frombuffer(contents_x, dtype=np.uint8).reshape(len(ids_x), 24)
    assert (x == np.arange(24) // 4).all()
    wings = np.frombuffer(groups["Wing"][1], dtype=np.uint8).reshape(-1, 24)
    assert (wings == np.arange(24)).all()


def test_insert_round_trip() -> None:
    n = 6
    state = nxsim.random_state(n, 11)
    rebuilt = nxsim.solved(n)
    for name, (ids, contents) in nxsim.extract(n, state).items():
        width = len(contents) // len(ids)
        for k, oid in enumerate(ids):
            rebuilt = nxsim.insert(n, rebuilt, oid, contents[k * width : (k + 1) * width])
    assert rebuilt == state


@pytest.mark.skipif(not LIBRARY.exists(), reason="library.json missing")
def test_library_phases_baseline_and_emit() -> None:
    lib = nxsim.Library(str(LIBRARY))
    assert len(lib.sha256) == 64
    n = 9
    state = nxsim.random_state(n, 5)
    segments, after = nxsim.phases(n, state)
    for phase, _oid, moves in segments:
        assert phase in ("parity", "core_frame") and len(moves) % 4 == 0
    # Solve every orbit with the baseline plan and emitted moves, core first.
    all_moves = b"".join(m for _, _, m in segments)
    cur = after
    for kind in ("Corner", "MidEdge"):
        ids, contents = nxsim.extract(n, cur)[kind]
        plan = lib.baseline_orbit(kind, contents)
        moves, ends = lib.emit(n, ids[0], plan)
        assert len(ends) == len(plan)
        cur = nxsim.apply_moves(n, cur, moves)
        all_moves += moves
    for kind, (ids, contents) in nxsim.extract(n, cur).items():
        if kind in ("Corner", "MidEdge", "FixedCenter"):
            continue
        width = len(contents) // len(ids)
        for k, oid in enumerate(ids):
            plan = lib.baseline_orbit(kind, contents[k * width : (k + 1) * width])
            all_moves += lib.emit(n, oid, plan)[0]
    assert nxsim.verify(n, state, all_moves)
    result = lib.solve_baseline(n, state)
    assert '"verified":true' in result

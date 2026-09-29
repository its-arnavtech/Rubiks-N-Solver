"""M7.1: the neural pipeline returns only verified solves; an untrained model exercises the
fallback path; results are deterministic."""

import base64

import numpy as np
import pytest
import torch

nxsim = pytest.importorskip("nxsim")

from nxnn.config import PYTHON_DIR, load_config  # noqa: E402
from nxnn.envs import Envs  # noqa: E402
from nxnn.library import DEFAULT_LIBRARY, load_library  # noqa: E402
from nxnn.model import QNet  # noqa: E402
from nxnn.solve import Solver, plan_greedy, solve  # noqa: E402

LIB = load_library()


@pytest.fixture(scope="module")
def solver() -> Solver:
    torch.manual_seed(0)
    model = QNet(load_config(PYTHON_DIR / "configs" / "smoke.yaml").model, LIB).eval()
    return Solver(LIB, nxsim.Library(str(DEFAULT_LIBRARY)), Envs(LIB), model, None, torch.device("cpu"))


def check(res: dict, n: int, start: bytes) -> None:
    assert res["verified"] is True
    raw = base64.b64decode(res["moves_b64"])
    assert nxsim.verify(n, start, raw)
    assert nxsim.verify(n, start, base64.b64decode(res["cancelled_moves_b64"]))
    at = 0
    for s in res["segments"]:
        assert s["start"] == at and s["end"] >= s["start"]
        at = s["end"]
    assert at == res["raw_len"] == len(raw) // 4


@pytest.mark.parametrize("n", [2, 3, 4, 5, 6, 7])
def test_untrained_model_falls_back_and_verifies(solver: Solver, n: int) -> None:
    start = nxsim.random_state(n, n)
    res = solve(solver, n, start, "nn", max_steps=4)
    check(res, n, start)
    assert res["stats"]["orbits_fallback"] >= 1
    assert {s["phase"] for s in res["segments"]} <= {"parity", "core_frame", "core", "orbits", "fallback"}


def test_beam_and_determinism(solver: Solver) -> None:
    start = nxsim.random_state(6, 9)
    a = solve(solver, 6, start, "nn", beam=2, max_steps=3)
    b = solve(solver, 6, start, "nn", beam=2, max_steps=3)
    check(a, 6, start)
    assert a["moves_b64"] == b["moves_b64"] and a["segments"] == b["segments"]


def test_baseline_method_and_solved_input(solver: Solver) -> None:
    start = nxsim.random_state(8, 1)
    res = solve(solver, 8, start, "baseline")
    check(res, 8, start)
    res = solve(solver, 5, nxsim.solved(5), "nn")
    assert res["raw_len"] == 0


def test_invalid_input_raises(solver: Solver) -> None:
    bad = bytearray(nxsim.solved(4))
    bad[0], bad[20] = bad[20], bad[0]
    with pytest.raises(ValueError):
        solve(solver, 4, bytes(bad), "nn")


def test_greedy_plans_solve_in_orbit_space(solver: Solver) -> None:
    """A plan reported as ok really solves its orbit (checked by replay in the env)."""
    env = solver.envs["Wing"]
    # One-action states: the untrained model may or may not find them; whatever it
    # reports as ok must replay to solved.
    s = env.apply(env.solved(50), torch.arange(50))
    acts, _, ok = plan_greedy(solver.model, env, s, 3)
    for i in np.flatnonzero(ok.numpy()):
        cur = s[i : i + 1]
        for a in acts[:, i].tolist():
            if a >= 0:
                cur = env.apply(cur, torch.tensor([a]))
        assert env.is_solved(cur).all()

"""M5.4: the orbit-level baseline solves 10k random states per type, and its plans equal the
Rust baseline's (via `nxsim`, when built)."""

import numpy as np
import pytest
import torch

from nxnn.baseline import Baseline
from nxnn.envs import Envs
from nxnn.library import DEFAULT_LIBRARY, TYPE_NAMES, load_library

LIB = load_library()
ENVS = Envs(LIB)


@pytest.mark.parametrize("name", TYPE_NAMES)
def test_solves_10k_random_states(name: str) -> None:
    env = ENVS[name]
    states = env.random_states(10_000, generator=torch.Generator().manual_seed(7))
    base = Baseline(LIB.types[name])
    plans, costs = base.solve(states.numpy())
    # Replay the plans in the tensor env: every state ends solved.
    width = max(len(p) for p in plans)
    cur = states.clone()
    for step in range(width):
        acts = torch.tensor([p[step] if step < len(p) else 0 for p in plans])
        live = torch.tensor([step < len(p) for p in plans])
        cur = torch.where(live[:, None], env.apply(cur, acts), cur)
    assert env.is_solved(cur).all()
    assert (costs == np.array([LIB.types[name].cost[p].sum() for p in plans])).all()
    assert costs.mean() > 0


def test_matches_rust_baseline() -> None:
    nxsim = pytest.importorskip("nxsim")
    lib = nxsim.Library(str(DEFAULT_LIBRARY))
    for name in TYPE_NAMES:
        states = ENVS[name].random_states(200, generator=torch.Generator().manual_seed(3))
        plans, _ = Baseline(LIB.types[name]).solve(states.numpy())
        for row, plan in zip(states.tolist(), plans):
            assert lib.baseline_orbit(name, bytes(row)) == plan, name

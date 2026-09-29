"""M5.3: envs ≡ real cube. An action applied in orbit space equals its moves applied to a
real cube with `nxsim` and re-extracted."""

import random

import numpy as np
import pytest
import torch

from nxnn.envs import Envs
from nxnn.library import DEFAULT_LIBRARY, TYPE_NAMES, load_library

nxsim = pytest.importorskip("nxsim")


def test_envs_equal_real_cube() -> None:
    lib_py = load_library()
    envs = Envs(lib_py)
    lib = nxsim.Library(str(DEFAULT_LIBRARY))
    assert lib.sha256 == lib_py.sha256
    rng = random.Random(3)
    for case in range(60):
        n = rng.randint(4, 30)
        state = nxsim.random_state(n, case)
        groups = nxsim.extract(n, state)
        name = rng.choice([t for t in TYPE_NAMES if t in groups])
        ids, contents = groups[name]
        env = envs[name]
        k = rng.randrange(len(ids))
        width = len(contents) // len(ids)
        cur = torch.tensor(list(contents[k * width : (k + 1) * width]), dtype=torch.long)[None]
        for _ in range(6):
            a = rng.randrange(env.num_actions)
            cur = env.apply(cur, torch.tensor([a]))
            moves, _ = lib.emit(n, ids[k], [a])
            state = nxsim.apply_moves(n, state, moves)
            real = np.frombuffer(nxsim.extract(n, state)[name][1], dtype=np.uint8)
            assert cur[0].tolist() == real[k * width : (k + 1) * width].tolist(), (n, name, a)

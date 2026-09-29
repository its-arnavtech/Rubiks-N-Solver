"""M5.2: library loading and tensor environments (no `nxsim`)."""

import json

import pytest
import torch

from nxnn.envs import Envs, permutation_parity
from nxnn.library import DEFAULT_LIBRARY, TYPE_NAMES, LibraryError, content_hash, load_library

LIB = load_library()


def test_library_loads() -> None:
    assert len(LIB.sha256) == 64
    assert [LIB.by_id(i).name for i in range(7)] == TYPE_NAMES
    corner = LIB.types["Corner"]
    assert corner.slots == 8 and corner.orientation_mod == 3
    assert corner.perm.shape == (corner.num_actions, 8)
    assert (LIB.types["XCenter"].cost > 0).all()


def test_tampering_is_detected(tmp_path) -> None:
    raw = json.loads(DEFAULT_LIBRARY.read_text(encoding="utf-8"))
    raw["types"]["Wing"]["actions"][0]["cost"] += 1
    bad = tmp_path / "library.json"
    bad.write_text(json.dumps(raw), encoding="utf-8")
    with pytest.raises(LibraryError):
        load_library(bad)
    # With a matching hash it loads: the hash covers the content, not the file bytes.
    raw["sha256"] = content_hash(raw)
    bad.write_text(json.dumps(raw, indent=1), encoding="utf-8")
    assert load_library(bad).types["Wing"].cost[0] == LIB.types["Wing"].cost[0] + 1
    raw["library_version"] = 99
    bad.write_text(json.dumps(raw), encoding="utf-8")
    with pytest.raises(LibraryError):
        load_library(bad)


def test_apply_and_solved() -> None:
    envs = Envs(LIB)
    g = torch.Generator().manual_seed(0)
    for env in envs:
        s = env.solved(16)
        assert env.is_solved(s).all()
        a = torch.randint(env.num_actions, (16,), generator=g)
        t = env.apply(s, a)
        if not env.is_color:
            # Every action moves some piece. (For color types a 3-cycle inside one face's
            # color class is a no-op in color space.)
            assert not env.is_solved(t).any(), env.name
        if env.m == 1:
            # A pure 3-cycle applied three times is the identity.
            assert env.is_solved(env.apply(env.apply(t, a), a)).all(), env.name


def test_scramble_and_random_states_are_legal() -> None:
    envs = Envs(LIB)
    g = torch.Generator().manual_seed(1)
    for env in envs:
        s, k = env.scramble(256, 5, generator=g)
        assert ((k >= 1) & (k <= 5)).all()
        r = env.random_states(512, generator=g)
        for states in (s, r):
            if env.is_color:
                counts = torch.stack([(states == c).sum(1) for c in range(6)], 1)
                assert (counts == 4).all(), env.name
            else:
                pieces = states // env.m
                assert (pieces.sort(1).values == torch.arange(env.slots)).all(), env.name
                assert (permutation_parity(pieces) == 0).all(), env.name
                assert ((states % env.m).sum(1) % env.m == 0).all(), env.name
                if env.name == "Corner":
                    assert (states[:, 6] == 18).all()
        assert len({tuple(x) for x in r.tolist()}) > 400, env.name

"""M6.3–M6.5: smoke training on CPU, checkpoints, evaluation."""

import math
import time

import numpy as np
import pytest
import torch

from nxnn.baseline import Baseline
from nxnn.checkpoint import load_checkpoint
from nxnn.config import PYTHON_DIR, load_config
from nxnn.envs import Envs
from nxnn.evaluate import beam, evaluate, greedy
from nxnn.library import TYPE_NAMES, Library, LibraryError, load_library
from nxnn.model import QNet
from nxnn.train import train

LIB = load_library()


@pytest.fixture(scope="module")
def smoke(tmp_path_factory):
    tmp = tmp_path_factory.mktemp("smoke")
    cfg = load_config(PYTHON_DIR / "configs" / "smoke.yaml")
    cfg.checkpoint.dir = tmp / "checkpoints"
    cfg.checkpoint.tensorboard_dir = tmp / "runs"
    t0 = time.time()
    hist = train(cfg, run_id="test-smoke", quiet=True)
    return cfg, hist, time.time() - t0


def test_smoke_trains_fast_without_nan_and_loss_decreases(smoke) -> None:
    cfg, hist, seconds = smoke
    assert len(hist.loss) == cfg.train.steps == 200
    assert seconds < 120, seconds
    assert all(math.isfinite(x) for x in hist.loss)
    # Targets jump at every target-net sync (Q-iteration), so "loss decreases" is checked
    # while the target is fixed: within each sync window, the loss falls on average.
    w = cfg.train.target_sync
    drops = [np.mean(hist.loss[i : i + 10]) - np.mean(hist.loss[i + w - 10 : i + w]) for i in range(0, 200, w)]
    assert np.mean(drops) > 0, drops
    assert set(hist.evals) == {100, 200}
    assert any(hist.tb_dir.iterdir())  # TensorBoard events written


def test_checkpoint_round_trip_and_hash_check(smoke) -> None:
    cfg, hist, _ = smoke
    path = hist.last_checkpoint
    assert path.name == "step_200.pt" and path.parent.name == "test-smoke"
    model, ckpt = load_checkpoint(path, LIB)
    assert ckpt["step"] == 200 and ckpt["library_sha256"] == LIB.sha256
    assert ckpt["git_commit"] and ckpt["eval"]
    s = Envs(LIB)["Wing"].random_states(4)
    q1 = model.q_type(2, s)
    model2, _ = load_checkpoint(path, LIB)
    assert torch.equal(q1, model2.q_type(2, s))
    other = Library(version=LIB.version, sha256="0" * 64, types=LIB.types)
    with pytest.raises(LibraryError):
        load_checkpoint(path, other)


def test_resume_continues_from_the_checkpoint(smoke, tmp_path) -> None:
    cfg, hist, _ = smoke
    cfg = cfg.model_copy(deep=True)
    cfg.train.steps = 210
    cfg.checkpoint.dir = tmp_path / "c"
    cfg.checkpoint.tensorboard_dir = tmp_path / "r"
    h2 = train(cfg, resume=hist.last_checkpoint, quiet=True)
    assert len(h2.loss) == 10
    assert h2.last_checkpoint.name == "step_210.pt"


def test_evaluate_reports_every_type(smoke) -> None:
    cfg, hist, _ = smoke
    model, _ = load_checkpoint(hist.last_checkpoint, LIB)
    envs = Envs(LIB)
    baselines = {n: Baseline(LIB.types[n]) for n in TYPE_NAMES}
    res = evaluate(model, envs, cfg.eval, baselines, torch.Generator().manual_seed(0), states_per_type=16)
    assert set(res) == set(TYPE_NAMES)
    for m in res.values():
        assert 0.0 <= m["greedy_solve_rate"] <= 1.0
        assert 0.0 <= m["beam_solve_rate"] <= 1.0
        assert m["baseline_mean_cost"] > 0


@pytest.mark.parametrize("tabu", [1, 16])
def test_plan_greedy_never_revisits_and_ok_means_solved(tabu: int) -> None:
    from nxnn.evaluate import plan_greedy

    torch.manual_seed(0)
    model = QNet(load_config(PYTHON_DIR / "configs" / "smoke.yaml").model, LIB).eval()
    env = Envs(LIB)["MidEdge"]
    s, _ = env.scramble(64, 3, generator=torch.Generator().manual_seed(2))
    acts, qs, ok = plan_greedy(model, env, s, 12, tabu=tabu)
    for i in range(s.shape[0]):
        cur = s[i : i + 1]
        seen = {tuple(cur[0].tolist())}
        for a in acts[:, i].tolist():
            if a < 0:
                continue
            cur = env.apply(cur, torch.tensor([a]))
            key = tuple(cur[0].tolist())
            if ok[i]:
                assert key not in seen  # an accepted plan never revisits
            seen.add(key)
        assert bool(ok[i]) <= bool(env.is_solved(cur).item())


def test_beam_check_counts_failures_in_batches() -> None:
    from nxnn.evaluate import beam_check

    torch.manual_seed(0)
    model = QNet(load_config(PYTHON_DIR / "configs" / "smoke.yaml").model, LIB).eval()
    env = Envs(LIB)["Corner"]
    n_bad, bad, _ = beam_check(model, env, 25, 2, 2, torch.Generator().manual_seed(1), batch=10)
    assert n_bad == bad.shape[0] and 0 <= n_bad <= 25
    assert bad.shape[1:] == (8,) if n_bad else True
    # Untrained and only 2 steps: random states are not solved.
    assert n_bad > 0


def test_greedy_and_full_width_beam_on_one_step_states() -> None:
    torch.manual_seed(0)
    model = QNet(load_config(PYTHON_DIR / "configs" / "smoke.yaml").model, LIB).eval()
    env = Envs(LIB)["Corner"]
    solved = env.solved(3)
    ok, cost, steps, _, _ = greedy(model, env, solved, 5)
    assert ok.all() and (cost == 0).all() and (steps == 0).all()
    # One action from solved: a beam as wide as the action set always finds the solution.
    s = env.apply(env.solved(4), torch.tensor([0, 5, 100, 600]))
    done, best = beam(model, env, s, env.num_actions, 1)
    assert done.all()
    inverse_cost = best
    assert (inverse_cost > 0).all()

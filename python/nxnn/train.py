"""Self-play Q-value iteration with a target network and a curriculum (ARCHITECTURE §9,
ADR-005). No teacher data: states come from the solved state and the network's own targets.

CLI: `python -m nxnn.train --config configs/default.yaml [--resume <ckpt>] [--steps N]`
"""

from __future__ import annotations

import argparse
import copy
import json
import math
import time
from contextlib import nullcontext
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

import torch
import torch.nn.functional as F
from torch.utils.tensorboard import SummaryWriter

from .baseline import Baseline
from .checkpoint import git_commit, load_checkpoint, save_checkpoint
from .config import Config, load_config
from .envs import BatchEnv, Envs
from .evaluate import evaluate, greedy
from .library import TYPE_NAMES, LibraryError, load_library
from .model import QNet, pad_contents


@dataclass
class Curriculum:
    """Per type: scramble depth K_t and the share of uniform random states."""

    k: list[int]
    p_uniform: list[float]

    @classmethod
    def start(cls, cfg: Config) -> Curriculum:
        return cls([cfg.curriculum.k_start] * 7, [0.0] * 7)


@dataclass
class History:
    loss: list[float] = field(default_factory=list)
    evals: dict[int, dict] = field(default_factory=dict)
    run_id: str = ""
    run_dir: Path | None = None
    tb_dir: Path | None = None
    last_checkpoint: Path | None = None
    seconds: float = 0.0


def make_run_id(cfg: Config) -> str:
    return f"{datetime.now():%Y%m%d-%H%M}-{cfg.run_name}"


def lr_lambda(cfg: Config):
    warm, total = cfg.train.warmup_steps, cfg.train.steps

    def f(step: int) -> float:
        if step < warm:
            return (step + 1) / warm
        frac = min(1.0, (step - warm) / max(1, total - warm))
        return 0.5 * (1 + math.cos(math.pi * frac))

    return f


def train(cfg: Config, resume: Path | None = None, run_id: str | None = None, quiet: bool = False) -> History:
    torch.manual_seed(cfg.seed)
    if cfg.device.startswith("cuda") and not torch.cuda.is_available():
        raise SystemExit("config asks for CUDA but torch.cuda.is_available() is False")
    dev = torch.device(cfg.device)
    lib = load_library(cfg.resolve(cfg.library))
    envs = Envs(lib, dev)
    benv = BatchEnv(lib, dev)
    gen = torch.Generator(device=dev).manual_seed(cfg.seed)
    cpu_gen = torch.Generator().manual_seed(cfg.seed + 1)
    model = QNet(cfg.model, lib).to(dev)
    opt = torch.optim.AdamW(model.parameters(), lr=cfg.train.lr, weight_decay=cfg.train.weight_decay)
    sched = torch.optim.lr_scheduler.LambdaLR(opt, lr_lambda(cfg))
    cur = Curriculum.start(cfg)
    start = 0
    hist = History()
    if resume is not None:
        _, ckpt = load_checkpoint(resume, lib, dev)  # checks the library hash
        model.load_state_dict(ckpt["model"])
        opt.load_state_dict(ckpt["optimizer"])
        sched.load_state_dict(ckpt["scheduler"])
        start = ckpt["step"]
        cur = Curriculum(**ckpt["curriculum"])
        run_id = run_id or ckpt.get("run_id")
    hist.run_id = run_id or make_run_id(cfg)
    hist.run_dir = cfg.resolve(cfg.checkpoint.dir) / hist.run_id
    hist.tb_dir = cfg.resolve(cfg.checkpoint.tensorboard_dir) / hist.run_id
    hist.run_dir.mkdir(parents=True, exist_ok=True)
    (hist.run_dir / "config.json").write_text(json.dumps(cfg.model_dump(mode="json"), indent=2), encoding="utf-8")
    writer = SummaryWriter(str(hist.tb_dir))
    target = copy.deepcopy(model).eval()
    for p in target.parameters():
        p.requires_grad_(False)
    baselines = {name: Baseline(lib.types[name]) for name in TYPE_NAMES}
    weights = torch.tensor([cfg.train.type_weights.get(t, 0.0) for t in TYPE_NAMES])
    use_bf16 = cfg.train.bf16 and dev.type == "cuda"
    amp = (lambda: torch.autocast("cuda", dtype=torch.bfloat16)) if use_bf16 else nullcontext
    k_act = cfg.train.actions_per_state
    commit = git_commit()
    t_start = time.time()
    last_eval: dict = {}

    def checkpoint(step: int) -> Path:
        path = hist.run_dir / f"step_{step}.pt"
        save_checkpoint(path, {
            "model": model.state_dict(), "optimizer": opt.state_dict(), "scheduler": sched.state_dict(),
            "config": cfg.model_dump(mode="json"), "library_sha256": lib.sha256, "git_commit": commit,
            "step": step, "run_id": hist.run_id, "curriculum": cur.__dict__, "eval": last_eval,
        })
        hist.last_checkpoint = path
        return path

    pending: list[torch.Tensor] = []

    def flush() -> float:
        """Move pending per-step losses to the history (one GPU sync); return the last."""
        if pending:
            vals = torch.stack(pending).tolist()
            pending.clear()
            if not all(math.isfinite(v) for v in vals):
                raise FloatingPointError(f"non-finite loss before step {len(hist.loss) + len(vals)}")
            hist.loss.extend(vals)
        return hist.loss[-1] if hist.loss else float("nan")

    for step in range(start, cfg.train.steps):
        model.train()
        # A type-sorted batch: slice sizes are known on the CPU, so no per-step GPU syncs.
        counts = torch.multinomial(weights, cfg.train.batch_size, replacement=True, generator=cpu_gen)
        counts = counts.bincount(minlength=7).tolist()
        bounds = [0]
        for c in counts:
            bounds.append(bounds[-1] + c)
        slices = [(t, bounds[t], bounds[t + 1]) for t in range(7) if counts[t] > 0]
        types = torch.repeat_interleave(torch.arange(7, device=dev), torch.tensor(counts, device=dev))
        kmax = torch.repeat_interleave(torch.tensor(cur.k, device=dev), torch.tensor(counts, device=dev))
        with torch.no_grad():
            s = benv.scramble(types, kmax, max(cur.k[t] for t, _, _ in slices), generator=gen)
            for t, a, b in slices:
                if cur.p_uniform[t] > 0:
                    n_uni = int(torch.binomial(torch.tensor(float(b - a)), torch.tensor(cur.p_uniform[t]),
                                               generator=cpu_gen))
                    if n_uni:
                        s[a : a + n_uni] = pad_contents(envs[t].random_states(n_uni, generator=gen))
            live = (~benv.is_solved(types, s)).float()
            with amp():
                h, v = model.encode(types, s)
                greedy_a = torch.cat([
                    model._type_q(h[a:b], v[a:b], t).topk(k_act // 2, dim=1, largest=False).indices
                    for t, a, b in slices
                ])
            acts = torch.cat([greedy_a, benv.random_actions(types, k_act - k_act // 2, gen)], 1)
            types_k = types.repeat_interleave(k_act)
            nxt = benv.apply(types_k, s.repeat_interleave(k_act, 0), acts.reshape(-1))
            solved = benv.is_solved(types_k, nxt)
            with amp():
                h2, v2 = target.encode(types_k, nxt)
                vmin = torch.cat([
                    target._type_q(h2[a * k_act : b * k_act], v2[a * k_act : b * k_act], t).min(1).values
                    for t, a, b in slices
                ])
            y = (benv.cost(types, acts).reshape(-1) + torch.where(solved, 0.0, vmin.float())).reshape(-1, k_act)
        with amp():
            q = model.q_actions(types, s, acts)
        per = F.huber_loss(q.float(), y, delta=cfg.train.huber_delta, reduction="none").mean(1)
        loss = (per * live).sum() / live.sum().clamp(min=1.0)
        opt.zero_grad(set_to_none=True)
        loss.backward()
        torch.nn.utils.clip_grad_norm_(model.parameters(), cfg.train.grad_clip)
        opt.step()
        sched.step()
        pending.append(loss.detach())
        done = step + 1
        sync_early = False
        if cfg.train.target_sync_loss is not None:
            sync_early = flush() < cfg.train.target_sync_loss
        if done % cfg.train.target_sync == 0 or sync_early:
            target.load_state_dict(model.state_dict())
        if done % cfg.train.log_every == 0 or done == cfg.train.steps:
            lv = flush()
            writer.add_scalar("train/loss", lv, done)
            writer.add_scalar("train/lr", sched.get_last_lr()[0], done)
            writer.add_scalar("train/q_mean", q.float().mean().item(), done)
            writer.add_scalar("train/y_mean", y.mean().item(), done)
            writer.add_scalar("train/steps_per_sec", (done - start) / max(1e-9, time.time() - t_start), done)
            for t, name in enumerate(TYPE_NAMES):
                writer.add_scalar(f"curriculum/k/{name}", cur.k[t], done)
            if not quiet:
                ks = " ".join(f"{n[:2]}{k}" for n, k in zip(TYPE_NAMES, cur.k))
                print(f"step {done:>7} loss {lv:9.4f} q {q.float().mean().item():7.2f} "
                      f"lr {sched.get_last_lr()[0]:.2e} K[{ks}] {time.time() - t_start:7.1f}s", flush=True)
        if done % cfg.curriculum.check_every == 0:
            model.eval()
            for t, env in enumerate(envs):
                if cur.k[t] >= cfg.curriculum.k_max:
                    continue
                s, _ = env.scramble(cfg.curriculum.check_states, cur.k[t], generator=gen)
                with amp():
                    ok = greedy(model, env, s, 4 * cur.k[t])[0].float().mean().item()
                writer.add_scalar(f"curriculum/solve_rate/{TYPE_NAMES[t]}", ok, done)
                if ok >= cfg.curriculum.advance_solve_rate:
                    cur.k[t] += 1
                    if cur.k[t] >= cfg.curriculum.k_uniform:
                        cur.p_uniform[t] = cfg.curriculum.p_uniform_after
        if done % cfg.eval.every == 0 or done == cfg.train.steps:
            with amp():
                last_eval = evaluate(model, envs, cfg.eval, baselines, gen)
            hist.evals[done] = last_eval
            for name, m in last_eval.items():
                for key, val in m.items():
                    writer.add_scalar(f"eval/{key}/{name}", val, done)
            if not quiet:
                for name, m in last_eval.items():
                    print(f"  eval {name:<11} greedy {m['greedy_solve_rate']:.3f} beam {m['beam_solve_rate']:.3f} "
                          f"ratio {m['greedy_baseline_ratio']:.3f}", flush=True)
        if done % cfg.checkpoint.every == 0 or done == cfg.train.steps:
            checkpoint(done)
    writer.close()
    hist.seconds = time.time() - t_start
    return hist


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--config", type=Path, required=True)
    p.add_argument("--resume", type=Path, default=None)
    p.add_argument("--steps", type=int, default=None, help="override train.steps")
    p.add_argument("--device", default=None, help="override device")
    args = p.parse_args()
    cfg = load_config(args.config)
    if args.steps is not None:
        cfg.train.steps = args.steps
    if args.device is not None:
        cfg.device = args.device
    try:
        hist = train(cfg, resume=args.resume)
    except LibraryError as e:
        raise SystemExit(str(e)) from e
    print(f"done: {hist.run_id} in {hist.seconds:.1f}s, last checkpoint {hist.last_checkpoint}")


if __name__ == "__main__":
    main()

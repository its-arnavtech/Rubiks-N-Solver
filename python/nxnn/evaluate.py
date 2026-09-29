"""Evaluation (ARCHITECTURE §9): per type, on uniform random states — greedy and beam solve
rates, mean primitive cost, ratio to the baseline on the same states, per-decision latency.

CLI: `python -m nxnn.evaluate --checkpoint <path> [--states N] [--beam W] [--device cuda]`
"""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path

import torch

from .baseline import Baseline
from .config import EvalConfig
from .envs import Envs, OrbitEnv
from .library import TYPE_NAMES, load_library
from .model import QNet, pad_contents


@torch.no_grad()
def q_values(model: QNet, env: OrbitEnv, states: torch.Tensor, chunk: int = 8192) -> torch.Tensor:
    """`[B, A_t]` Q values, computed in chunks."""
    parts = [
        model.q_type(env.type_id, pad_contents(states[i : i + chunk]))
        for i in range(0, states.shape[0], chunk)
    ]
    return torch.cat(parts) if parts else torch.empty(0, env.num_actions, device=states.device)


# Candidates tried per step by the solver's greedy rollout (ADR-017).
TABU = 16


@torch.no_grad()
def plan_greedy(model: QNet, env: OrbitEnv, states: torch.Tensor, max_steps: int, tabu: int = TABU):
    """Greedy rounds for all orbits of one type, with a revisit rule (ADR-017): each step
    takes the lowest-Q action among the `tabu` best whose resulting state this orbit has not
    visited; `tabu=1` is plain greedy that stops on a revisit.

    Returns `(actions[R, n], q[R, n], ok[n])` with -1 where an orbit took no action; `ok` is
    False when the step cap is hit or every candidate revisits.
    """
    n = states.shape[0]
    dev = states.device
    g = torch.Generator(device="cpu").manual_seed(0)
    w = torch.randint(1, 2**61, (states.shape[1],), generator=g).to(dev)
    cur = states.clone()
    solved = env.is_solved(cur)
    bad = torch.zeros(n, dtype=torch.bool, device=dev)
    seen = [(cur * w).sum(1)]
    acts, qs = [], []
    k = max(1, min(tabu, env.num_actions))
    for _ in range(max_steps):
        live = (~solved & ~bad).nonzero().squeeze(1)
        if live.numel() == 0:
            break
        q = q_values(model, env, cur[live])
        cand_q, cand = q.topk(k, dim=1, largest=False)
        hist = torch.stack(seen, 1)[live]
        base = cur[live]
        choice = torch.full((live.numel(),), -1, dtype=torch.long, device=dev)
        for j in range(k):
            h = (env.apply(base, cand[:, j]) * w).sum(1)
            fresh = (choice < 0) & ~(hist == h[:, None]).any(1)
            choice = torch.where(fresh, j, choice)
        stuck = choice < 0
        choice = choice.clamp(min=0)
        rows = torch.arange(live.numel(), device=dev)
        a, qv = cand[rows, choice], cand_q[rows, choice]
        nxt = env.apply(base, a)
        row_a = torch.full((n,), -1, dtype=torch.long, device=dev)
        row_q = torch.zeros(n, device=dev)
        row_a[live], row_q[live] = a, qv.float()
        acts.append(row_a)
        qs.append(row_q)
        cur[live] = nxt
        new_seen = seen[-1].clone()
        new_seen[live] = (nxt * w).sum(1)
        seen.append(new_seen)
        bad[live[stuck]] = True
        solved = env.is_solved(cur)
    ok = solved & ~bad
    if not acts:
        return torch.empty(0, n, dtype=torch.long), torch.empty(0, n), ok.cpu()
    return torch.stack(acts).cpu(), torch.stack(qs).cpu(), ok.cpu()


@torch.no_grad()
def greedy(model: QNet, env: OrbitEnv, states: torch.Tensor, step_cap: int):
    """Greedy rollout. Returns `(solved[B], cost[B], steps[B], decisions, seconds)`."""
    cur = states.clone()
    cost = torch.zeros(cur.shape[0], device=cur.device)
    steps = torch.zeros(cur.shape[0], dtype=torch.long, device=cur.device)
    solved = env.is_solved(cur)
    decisions, seconds = 0, 0.0
    for _ in range(step_cap):
        live = (~solved).nonzero().squeeze(1)
        if live.numel() == 0:
            break
        t0 = time.perf_counter()
        a = q_values(model, env, cur[live]).argmin(1)
        if cur.is_cuda:
            torch.cuda.synchronize()
        seconds += time.perf_counter() - t0
        decisions += live.numel()
        cur[live] = env.apply(cur[live], a)
        cost[live] += env.cost[a]
        steps[live] += 1
        solved = env.is_solved(cur)
    return solved, cost, steps, decisions, seconds


@torch.no_grad()
def beam(model: QNet, env: OrbitEnv, states: torch.Tensor, width: int, step_cap: int, chunk: int = 512):
    """Beam search on `g + Q(s, a)` (Q estimates the total remaining cost including `a`),
    with the no-revisit rule (ADR-017). Returns `(solved[B], cost[B])`."""
    out_solved, out_cost = [], []
    for i in range(0, states.shape[0], chunk):
        s, c, _, _ = beam_paths(model, env, states[i : i + chunk], width, step_cap)
        out_solved.append(s)
        out_cost.append(c)
    return torch.cat(out_solved), torch.cat(out_cost)


@torch.no_grad()
def beam_paths(model: QNet, env: OrbitEnv, states: torch.Tensor, w: int, step_cap: int):
    """Beam search that records paths. Each step scores the `4w` best `(beam, action)`
    candidates, drops any whose resulting state is already on that beam's own path (ADR-017),
    and keeps the best `w`. Returns `(solved[B], cost[B], plans, q_per_step)` where the plans are
    the cheapest solution found per state (empty if none)."""
    b, s = states.shape
    dev = states.device
    a_n = env.num_actions
    hw = torch.randint(1, 2**61, (s,), generator=torch.Generator().manual_seed(0)).to(dev)
    beams = states[:, None, :].expand(b, w, s).clone()
    g = torch.zeros(b, w, device=dev)
    alive = torch.zeros(b, w, dtype=torch.bool, device=dev)
    alive[:, 0] = True
    hist = (beams * hw).sum(-1)[:, :, None]  # [b, w, t] state hashes along each beam's path
    paths = torch.zeros(b, w, 0, dtype=torch.long, device=dev)
    pq = torch.zeros(b, w, 0, device=dev)
    done = env.is_solved(states)
    best = torch.where(done, 0.0, float("inf")).to(dev)
    plans: list[list[int]] = [[] for _ in range(b)]
    qs: list[list[float]] = [[] for _ in range(b)]
    k = min(4 * w, w * a_n)
    for _ in range(step_cap):
        idx = (~done).nonzero().squeeze(1)
        if idx.numel() == 0:
            break
        n = idx.numel()
        sub, gi, al = beams[idx], g[idx], alive[idx]
        q = q_values(model, env, sub.reshape(-1, s)).reshape(n, w, a_n)
        score = gi[:, :, None] + q
        score[~al] = float("inf")
        vals, pos = score.reshape(n, w * a_n).topk(k, dim=1, largest=False)
        parent, act = pos // a_n, pos % a_n
        rows = torch.arange(n, device=dev)[:, None]
        child = env.apply(sub[rows, parent].reshape(-1, s), act.reshape(-1)).reshape(n, k, s)
        ch = (child * hw).sum(-1)
        revisit = (hist[idx][rows, parent] == ch[:, :, None]).any(-1)
        vals = torch.where(revisit, float("inf"), vals)
        vals, keep = vals.topk(w, dim=1, largest=False)
        parent, act = parent.gather(1, keep), act.gather(1, keep)
        new = child[rows, keep]
        new_g = gi.gather(1, parent) + env.cost[act]
        new_alive = torch.isfinite(vals)
        qa = q[rows, parent, act]
        new_paths = torch.cat([paths[idx][rows, parent], act[:, :, None]], 2)
        new_pq = torch.cat([pq[idx][rows, parent], qa[:, :, None]], 2)
        new_hist = torch.cat([hist[idx][rows, parent], ((new * hw).sum(-1))[:, :, None]], 2)
        hit = env.is_solved(new.reshape(-1, s)).reshape(n, w) & new_alive
        if hit.any():
            hit_cost = torch.where(hit, new_g, float("inf"))
            for r in hit.any(1).nonzero().squeeze(1).tolist():
                j = int(hit_cost[r].argmin())
                i = int(idx[r])
                if new_g[r, j] < best[i]:
                    best[i] = new_g[r, j]
                    plans[i] = new_paths[r, j].tolist()
                    qs[i] = new_pq[r, j].tolist()
            done[idx] = done[idx] | hit.any(1)
        t = new_paths.shape[2]
        paths = torch.cat([paths, torch.zeros(b, w, 1, dtype=torch.long, device=dev)], 2)
        pq = torch.cat([pq, torch.zeros(b, w, 1, device=dev)], 2)
        hist = torch.cat([hist, hist[:, :, -1:]], 2)
        paths[idx], pq[idx], hist[idx] = new_paths, new_pq, new_hist
        beams[idx], g[idx], alive[idx] = new, new_g, new_alive
        assert paths.shape[2] == t
    return done, best, plans, qs


def evaluate(
    model: QNet,
    envs: Envs,
    cfg: EvalConfig,
    baselines: dict[str, Baseline],
    generator: torch.Generator | None = None,
    types: list[str] | None = None,
    states_per_type: int | None = None,
) -> dict[str, dict[str, float]]:
    model.eval()
    n = states_per_type or cfg.states_per_type
    out: dict[str, dict[str, float]] = {}
    for name in types or TYPE_NAMES:
        env = envs[name]
        states = env.random_states(n, generator=generator)
        solved, cost, steps, decisions, seconds = greedy(model, env, states, cfg.step_cap)
        b_solved, b_cost = beam(model, env, states, cfg.beam_width, cfg.step_cap)
        t_acts, _, t_ok = plan_greedy(model, env, states, cfg.step_cap, tabu=TABU)
        t_ok = t_ok.to(states.device)
        t_cost = torch.where(t_acts >= 0, env.cost.cpu()[t_acts.clamp(min=0)], 0.0).sum(0).to(states.device)
        _, base = baselines[name].solve(states.cpu().numpy())
        base = torch.as_tensor(base, dtype=torch.float32, device=states.device)
        mask = solved & (base > 0)
        ratio = (cost[mask].sum() / base[mask].sum()).item() if mask.any() else float("nan")
        tmask = t_ok & (base > 0)
        t_ratio = (t_cost[tmask].sum() / base[tmask].sum()).item() if tmask.any() else float("nan")
        bmask = b_solved & (base > 0)
        b_ratio = (b_cost[bmask].sum() / base[bmask].sum()).item() if bmask.any() else float("nan")
        out[name] = {
            "greedy_solve_rate": solved.float().mean().item(),
            "beam_solve_rate": b_solved.float().mean().item(),
            "tabu_solve_rate": t_ok.float().mean().item(),
            "tabu_baseline_ratio": t_ratio,
            "greedy_mean_cost": cost[solved].mean().item() if solved.any() else float("nan"),
            "beam_mean_cost": b_cost[b_solved].mean().item() if b_solved.any() else float("nan"),
            "baseline_mean_cost": base.mean().item(),
            "greedy_baseline_ratio": ratio,
            "beam_baseline_ratio": b_ratio,
            "greedy_mean_steps": steps[solved].float().mean().item() if solved.any() else float("nan"),
            "latency_us_per_decision": 1e6 * seconds / max(decisions, 1),
        }
    return out


@torch.no_grad()
def beam_check(model: QNet, env: OrbitEnv, total: int, width: int, step_cap: int,
               generator: torch.Generator | None = None, batch: int = 10_000):
    """Acceptance check (ARCHITECTURE §9): beam search on `total` uniform random states, in
    batches. Returns `(failures, failed_states, mean_cost_of_solved)`."""
    failed, cost_sum, solved_n = [], 0.0, 0
    for i in range(0, total, batch):
        s = env.random_states(min(batch, total - i), generator=generator)
        ok, cost = beam(model, env, s, width, step_cap)
        failed.append(s[~ok].cpu())
        cost_sum += cost[ok].sum().item()
        solved_n += int(ok.sum().item())
    bad = torch.cat(failed)
    return bad.shape[0], bad, cost_sum / max(solved_n, 1)


def main() -> None:
    from .checkpoint import current_checkpoint, load_checkpoint

    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--checkpoint", type=Path, default=None, help="default: artifacts/checkpoints/CURRENT")
    p.add_argument("--states", type=int, default=None)
    p.add_argument("--beam", type=int, default=None)
    p.add_argument("--step-cap", type=int, default=None)
    p.add_argument("--device", default="cuda" if torch.cuda.is_available() else "cpu")
    p.add_argument("--seed", type=int, default=12345)
    p.add_argument("--types", nargs="*", default=None)
    p.add_argument("--beam-check", type=int, default=None, metavar="N",
                   help="only run the beam acceptance check on N states per type")
    args = p.parse_args()
    path = args.checkpoint or current_checkpoint()
    if path is None:
        raise SystemExit("no --checkpoint and no artifacts/checkpoints/CURRENT")
    lib = load_library()
    model, ckpt = load_checkpoint(path, lib, args.device)
    envs = Envs(lib, args.device)
    cfg = EvalConfig.model_validate(ckpt["config"]["eval"])
    if args.beam:
        cfg.beam_width = args.beam
    if args.step_cap:
        cfg.step_cap = args.step_cap
    gen = torch.Generator(device=args.device).manual_seed(args.seed)
    if args.beam_check:
        print(f"checkpoint {path} (step {ckpt.get('step')}): beam-{cfg.beam_width} on "
              f"{args.beam_check:,} uniform random states per type, step cap {cfg.step_cap}", flush=True)
        report = {}
        for name in args.types or TYPE_NAMES:
            t0 = time.perf_counter()
            n_bad, bad, mean_cost = beam_check(model, envs[name], args.beam_check, cfg.beam_width,
                                               cfg.step_cap, gen)
            report[name] = {"states": args.beam_check, "failures": n_bad, "mean_cost": mean_cost,
                            "failed_states": bad.tolist()}
            print(f"{name:<11} failures {n_bad:>6} / {args.beam_check:,}  mean cost {mean_cost:7.2f}  "
                  f"({time.perf_counter() - t0:.0f} s)", flush=True)
        out = Path(path).with_suffix(f".beam{cfg.beam_width}-{args.beam_check}.json")
        out.write_text(json.dumps(report), encoding="utf-8")
        print(f"wrote {out}")
        return
    baselines = {name: Baseline(lib.types[name]) for name in TYPE_NAMES}
    res = evaluate(model, envs, cfg, baselines, gen, args.types, args.states)
    print(f"checkpoint {path} (step {ckpt.get('step')})")
    print(f"{'type':<11} {'greedy':>8} {'tabu':>8} {'beam':>8} {'cost':>8} {'base':>8} {'ratio':>6} "
          f"{'t.ratio':>7} {'us/dec':>8}")
    for name, m in res.items():
        print(
            f"{name:<11} {m['greedy_solve_rate']:8.4f} {m['tabu_solve_rate']:8.4f} {m['beam_solve_rate']:8.4f} "
            f"{m['greedy_mean_cost']:8.2f} {m['baseline_mean_cost']:8.2f} "
            f"{m['greedy_baseline_ratio']:6.3f} {m['tabu_baseline_ratio']:7.3f} {m['latency_us_per_decision']:8.2f}"
        )
    out = Path(path).with_suffix(".eval.json")
    out.write_text(json.dumps(res, indent=2), encoding="utf-8")
    print(f"wrote {out}")


if __name__ == "__main__":
    main()

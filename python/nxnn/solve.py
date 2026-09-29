"""Neural solver pipeline (ARCHITECTURE §7, §10).

Phases and all move-level work go through `nxsim` (Rust): validation, phase 0/1a/1b, extraction,
emission, cancellation, verification. The network plans every orbit in orbit space; all orbits
of a type advance together, one forward pass per type per round. An orbit that hits the step
cap or revisits a state goes to the baseline (phase `fallback`). Nothing unverified is returned
(ADR-007).

CLI: `python -m nxnn.solve --n N --seed S --solver nn|baseline [--beam W] [--random-state]`
"""

from __future__ import annotations

import argparse
import base64
import json
import time
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import torch

import nxsim

from .checkpoint import current_checkpoint, load_checkpoint
from .envs import Envs, OrbitEnv
from .evaluate import TABU, plan_greedy, q_values
from .library import DEFAULT_LIBRARY, TYPE_NAMES, Library, load_library
from .model import QNet

CORE = ("Corner", "MidEdge")


class SolveError(RuntimeError):
    pass


@dataclass
class Solver:
    lib: Library
    native: nxsim.Library
    envs: Envs
    model: QNet | None
    checkpoint: str | None
    device: torch.device

    @classmethod
    def load(cls, checkpoint: str | Path | None = None, device: str | None = None, need_model: bool = True) -> Solver:
        dev = torch.device(device or ("cuda" if torch.cuda.is_available() else "cpu"))
        lib = load_library()
        native = nxsim.Library(str(DEFAULT_LIBRARY))
        if native.sha256 != lib.sha256:
            raise SolveError("nxsim and nxnn loaded different libraries")
        model, ck = None, None
        if need_model:
            path = Path(checkpoint) if checkpoint else current_checkpoint()
            if path is None:
                raise SolveError("no checkpoint given and artifacts/checkpoints/CURRENT is missing")
            model, _ = load_checkpoint(path, lib, dev)
            ck = str(path)
        return cls(lib, native, Envs(lib, dev), model, ck, dev)



@torch.no_grad()
def plan_beam(model: QNet, env: OrbitEnv, states: torch.Tensor, width: int, max_steps: int, chunk: int = 256):
    """Beam search with paths. Returns `(plans, q_per_step, ok)` as Python lists."""
    plans: list[list[int]] = []
    qs: list[list[float]] = []
    oks: list[bool] = []
    for i in range(0, states.shape[0], chunk):
        p, q, ok = _beam_paths(model, env, states[i : i + chunk], width, max_steps)
        plans += p
        qs += q
        oks += ok
    return plans, qs, oks


def _beam_paths(model: QNet, env: OrbitEnv, states: torch.Tensor, w: int, max_steps: int):
    b, s = states.shape
    dev = states.device
    a_n = env.num_actions
    beams = states[:, None, :].expand(b, w, s).clone()
    g = torch.zeros(b, w, device=dev)
    alive = torch.zeros(b, w, dtype=torch.bool, device=dev)
    alive[:, 0] = True
    paths = torch.zeros(b, w, 0, dtype=torch.long, device=dev)
    pq = torch.zeros(b, w, 0, device=dev)
    done = env.is_solved(states)
    best: list[tuple[list[int], list[float]] | None] = [([], []) if d else None for d in done.tolist()]
    for _ in range(max_steps):
        idx = (~done).nonzero().squeeze(1)
        if idx.numel() == 0:
            break
        n = idx.numel()
        sub = beams[idx]
        q = q_values(model, env, sub.reshape(-1, s)).reshape(n, w, a_n)
        score = g[idx][:, :, None] + q
        score[~alive[idx]] = float("inf")
        vals, pos = score.reshape(n, w * a_n).topk(w, dim=1, largest=False)
        parent, act = pos // a_n, pos % a_n
        rows = torch.arange(n, device=dev)[:, None]
        new = env.apply(sub[rows, parent].reshape(-1, s), act.reshape(-1)).reshape(n, w, s)
        new_g = g[idx].gather(1, parent) + env.cost[act]
        new_alive = torch.isfinite(vals)
        qa = q[rows, parent, act]
        new_paths = torch.cat([paths[idx][rows, parent], act[:, :, None]], 2)
        new_pq = torch.cat([pq[idx][rows, parent], qa[:, :, None]], 2)
        hit = env.is_solved(new.reshape(-1, s)).reshape(n, w) & new_alive
        for r in hit.any(1).nonzero().squeeze(1).tolist():
            j = int(torch.where(hit[r], new_g[r], float("inf")).argmin())
            best[int(idx[r])] = (new_paths[r, j].tolist(), new_pq[r, j].tolist())
        done[idx] = done[idx] | hit.any(1)
        beams[idx], g[idx], alive[idx] = new, new_g, new_alive
        full_paths = torch.zeros(b, w, new_paths.shape[2], dtype=torch.long, device=dev)
        full_pq = torch.zeros(b, w, new_paths.shape[2], device=dev)
        full_paths[idx], full_pq[idx] = new_paths, new_pq
        paths, pq = full_paths, full_pq
    plans = [x[0] if x else [] for x in best]
    qs = [x[1] if x else [] for x in best]
    return plans, qs, [x is not None for x in best]


class _Emitter:
    def __init__(self) -> None:
        self.raw = bytearray()
        self.segments: list[dict] = []

    def add(self, moves: bytes, ends: list[int], meta: list[dict]) -> None:
        start = len(self.raw) // 4
        self.raw += moves
        prev = 0
        for end, m in zip(ends, meta):
            self.segments.append({**m, "start": start + prev, "end": start + end})
            prev = end

    def add_phase(self, phase: str, orbit_id: int, orbit_type: str, moves: bytes) -> None:
        count = len(moves) // 4
        self.add(moves, [count], [_meta(phase, orbit_id, orbit_type, None, None, None)])


def _meta(phase, orbit_id, orbit_type, rnd, action, q) -> dict:
    return {"phase": phase, "orbit_id": orbit_id, "orbit_type": orbit_type,
            "round": rnd, "action_id": action, "q": q}


def _plan_type(solver: Solver, name: str, states: np.ndarray, beam: int, max_steps: int):
    """Per-orbit `(plan, q_list)` or None (→ fallback) for one type's orbits."""
    env = solver.envs[name]
    x = torch.as_tensor(np.array(states, dtype=np.int64), device=solver.device)
    if beam <= 1:
        acts, qs, ok = plan_greedy(solver.model, env, x, max_steps, tabu=TABU)
        a, q = acts.numpy(), qs.numpy()
        out = []
        for i in range(x.shape[0]):
            if not ok[i]:
                out.append(None)
                continue
            col = a[:, i] if a.size else np.zeros(0, dtype=np.int64)
            keep = col >= 0
            out.append((col[keep].tolist(), q[:, i][keep].tolist() if a.size else []))
        return out
    plans, qs, ok = plan_beam(solver.model, env, x, beam, max_steps)
    return [(p, q) if good else None for p, q, good in zip(plans, qs, ok)]


def solve(solver: Solver, n: int, facelets: bytes, method: str = "nn", beam: int = 1, max_steps: int = 64) -> dict:
    """Solve and verify; returns a `SolveResult` dict (CONVENTIONS §8)."""
    t_total = time.perf_counter()
    nxsim.validate(n, facelets)
    if method == "baseline":
        res = json.loads(solver.native.solve_baseline(n, facelets))
        res["checkpoint"] = None
        return res
    if method != "nn" or solver.model is None:
        raise SolveError(f"solver {method!r} needs a loaded model" if method == "nn" else f"unknown solver {method!r}")
    types = {oid: t for oid, t, _, _ in nxsim.orbits(n)}
    em = _Emitter()
    timings = {"extract": 0.0, "plan": 0.0, "emit": 0.0, "verify": 0.0}
    fallbacks = 0
    rounds = 0

    phase_segments, cur = nxsim.phases(n, facelets)
    for phase, oid, moves in phase_segments:
        em.add_phase(phase, oid, types[oid], moves)

    # Core: corners, then middle edges; their moves change the other orbits, so apply them.
    for name in CORE:
        groups = nxsim.extract(n, cur)
        if name not in groups:
            continue
        ids, contents = groups[name]
        states = np.frombuffer(contents, dtype=np.uint8).reshape(len(ids), -1)
        t = time.perf_counter()
        (res,) = _plan_type(solver, name, states, beam, max_steps)
        timings["plan"] += time.perf_counter() - t
        if res is None:
            fallbacks += 1
            plan = solver.native.baseline_orbit(name, contents)
            meta = [_meta("fallback", ids[0], name, r, a, None) for r, a in enumerate(plan)]
        else:
            plan, qs = res
            rounds = max(rounds, len(plan))
            meta = [_meta("core", ids[0], name, r, a, q) for r, (a, q) in enumerate(zip(plan, qs))]
        moves, ends = solver.native.emit(n, ids[0], plan)
        em.add(moves, ends, meta)
        cur = nxsim.apply_moves(n, cur, moves)

    # Every other orbit, batched per type.
    t = time.perf_counter()
    groups = nxsim.extract(n, cur)
    timings["extract"] += time.perf_counter() - t
    by_round: dict[int, list[tuple[int, int, float, str]]] = {}
    failed: dict[str, list[tuple[int, bytes]]] = {}
    for name in TYPE_NAMES:
        if name in CORE or name not in groups:
            continue
        ids, contents = groups[name]
        states = np.frombuffer(contents, dtype=np.uint8).reshape(len(ids), -1)
        t = time.perf_counter()
        results = _plan_type(solver, name, states, beam, max_steps)
        timings["plan"] += time.perf_counter() - t
        for oid, row, res in zip(ids, states, results):
            if res is None:
                failed.setdefault(name, []).append((oid, row.tobytes()))
                continue
            plan, qs = res
            rounds = max(rounds, len(plan))
            for r, (a, q) in enumerate(zip(plan, qs)):
                by_round.setdefault(r, []).append((oid, a, float(q), name))
    t = time.perf_counter()
    for r in sorted(by_round):
        items = sorted(by_round[r])
        moves, ends = solver.native.emit_batch(n, [i[0] for i in items], [i[1] for i in items])
        em.add(moves, ends, [_meta("orbits", o, nm, r, a, q) for o, a, q, nm in items])
    for name, rows in failed.items():
        fallbacks += len(rows)
        plans = solver.native.baseline_batch(name, b"".join(c for _, c in rows), len(rows))
        pairs = [(oid, r, a) for (oid, _), plan in zip(rows, plans) for r, a in enumerate(plan)]
        moves, ends = solver.native.emit_batch(n, [p[0] for p in pairs], [p[2] for p in pairs])
        em.add(moves, ends, [_meta("fallback", o, name, r, a, None) for o, r, a in pairs])
    raw = bytes(em.raw)
    cancelled = nxsim.cancel(raw)
    timings["emit"] += time.perf_counter() - t

    t = time.perf_counter()
    if not (nxsim.verify(n, facelets, raw) and nxsim.verify(n, facelets, cancelled)):
        raise SolveError("solution failed verification")  # never reported as a solve
    timings["verify"] += time.perf_counter() - t
    timings = {k: 1e3 * v for k, v in timings.items()}
    timings["total"] = 1e3 * (time.perf_counter() - t_total)
    return {
        "n": n, "solver": "nn", "verified": True,
        "moves_b64": base64.b64encode(raw).decode(),
        "cancelled_moves_b64": base64.b64encode(cancelled).decode(),
        "raw_len": len(raw) // 4, "cancelled_len": len(cancelled) // 4,
        "segments": em.segments,
        "stats": {"orbits_total": len(types), "orbits_fallback": fallbacks, "rounds": rounds},
        "timings_ms": timings,
        "checkpoint": solver.checkpoint,
        "library_sha256": solver.lib.sha256,
        "beam": beam,
    }


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--n", type=int, required=True)
    p.add_argument("--seed", type=int, default=0)
    p.add_argument("--solver", choices=["nn", "baseline"], default="nn")
    p.add_argument("--beam", type=int, default=1)
    p.add_argument("--max-steps", type=int, default=64)
    p.add_argument("--random-state", action="store_true", help="uniform random state instead of a scramble")
    p.add_argument("--len", type=int, default=None, help="scramble length (default 20·N)")
    p.add_argument("--checkpoint", type=Path, default=None)
    p.add_argument("--device", default=None)
    p.add_argument("--json", type=Path, default=None, help="write the SolveResult here")
    args = p.parse_args()
    solver = Solver.load(args.checkpoint, args.device, need_model=args.solver == "nn")
    if args.random_state:
        facelets = nxsim.random_state(args.n, args.seed)
    else:
        facelets, _ = nxsim.scramble(args.n, args.len or 20 * args.n, args.seed)
    res = solve(solver, args.n, facelets, args.solver, args.beam, args.max_steps)
    st, tm = res["stats"], res["timings_ms"]
    print(f"N={args.n} seed={args.seed} {args.solver}: verified, {res['raw_len']} raw / "
          f"{res['cancelled_len']} cancelled moves, {st['orbits_total']} orbits, "
          f"{st['orbits_fallback']} fallback, {st['rounds']} rounds")
    print("time: " + ", ".join(f"{k} {v:.1f} ms" for k, v in tm.items()))
    if args.json:
        args.json.write_text(json.dumps(res), encoding="utf-8")
        print(f"wrote {args.json}")


if __name__ == "__main__":
    main()

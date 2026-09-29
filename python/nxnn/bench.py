"""M7.3 benchmark: neural solver vs baseline on the same uniform random states.

`python -m nxnn.bench [--sizes 2 3 4 ...] [--device cuda]` prints a markdown table:
wall time per solve (in-process, including verification), raw / cancelled moves, fallback orbits.
"""

from __future__ import annotations

import argparse
import statistics
import time

import nxsim

from .solve import Solver, solve

SIZES = [2, 3, 4, 5, 7, 10, 20, 50, 100]


def seeds_for(n: int) -> int:
    return 20 if n <= 10 else 10 if n <= 20 else 5 if n <= 50 else 3


def run(solver: Solver, n: int, count: int, method: str, beam: int) -> dict:
    times, raw, canc, fb, orbits = [], [], [], [], 0
    for seed in range(count):
        start = nxsim.random_state(n, 1_000_000 + seed)
        t = time.perf_counter()
        res = solve(solver, n, start, method, beam)
        times.append(1e3 * (time.perf_counter() - t))
        assert res["verified"]
        raw.append(res["raw_len"])
        canc.append(res["cancelled_len"])
        fb.append(res["stats"]["orbits_fallback"])
        orbits = res["stats"]["orbits_total"]
    return {
        "ms": statistics.mean(times), "ms_max": max(times),
        "raw": statistics.mean(raw), "cancelled": statistics.mean(canc),
        "fallback": sum(fb), "orbits": orbits * count,
    }


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--sizes", type=int, nargs="*", default=SIZES)
    p.add_argument("--device", default=None)
    p.add_argument("--beam", type=int, default=1)
    args = p.parse_args()
    solver = Solver.load(None, args.device, need_model=True)
    solve(solver, 5, nxsim.random_state(5, 0), "nn")  # warm up CUDA and caches
    print(f"checkpoint {solver.checkpoint}, device {solver.device}, beam {args.beam}\n")
    print("| N | solves | NN mean ms | NN max ms | baseline mean ms | NN cancelled moves | "
          "baseline cancelled moves | NN / baseline | NN raw moves | fallback orbits |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    for n in args.sizes:
        k = seeds_for(n)
        nn = run(solver, n, k, "nn", args.beam)
        base = run(solver, n, k, "baseline", 1)
        print(f"| {n} | {k} | {nn['ms']:.1f} | {nn['ms_max']:.1f} | {base['ms']:.1f} | "
              f"{nn['cancelled']:,.1f} | {base['cancelled']:,.1f} | {nn['cancelled'] / base['cancelled']:.3f} | "
              f"{nn['raw']:,.1f} | {nn['fallback']} / {nn['orbits']} |", flush=True)


if __name__ == "__main__":
    main()

# Results

Measured numbers only. Each section says how to reproduce it.

## Neural solver vs baseline, end to end (M7.3)

Checkpoint `20260929-midedge-finetune/step_130000.pt` (the `CURRENT` checkpoint), tabu greedy
(ADR-017), beam 1, RTX 4060 laptop, 2026-09-29. Uniform random states with seeds
1,000,000 + i, the same states for both solvers. Time is in-process wall time per solve, including
validation, phases, planning, emission, cancellation and verification of both move lists; the
baseline is the Rust solver through `nxsim`. Every solve was verified.
Reproduce: `cd python; uv run python -m nxnn.bench` (log: `runs/m7.3-bench.log`, not committed).

| N | solves | NN mean ms | NN max ms | baseline mean ms | NN cancelled moves | baseline cancelled moves | NN / baseline | NN raw moves | fallback orbits |
|---|---|---|---|---|---|---|---|---|---|
| 2 | 20 | 35.4 | 47.3 | 0.3 | 35.6 | 81.0 | 0.440 | 37.0 | 0 / 20 |
| 3 | 20 | 95.3 | 125.4 | 0.3 | 76.0 | 145.6 | 0.522 | 78.8 | 0 / 60 |
| 4 | 20 | 183.5 | 208.9 | 0.3 | 202.2 | 266.3 | 0.759 | 207.5 | 0 / 60 |
| 5 | 20 | 296.0 | 355.1 | 0.4 | 304.9 | 421.7 | 0.723 | 313.1 | 0 / 120 |
| 7 | 20 | 474.0 | 554.5 | 0.7 | 671.1 | 861.0 | 0.780 | 681.1 | 0 / 220 |
| 10 | 20 | 355.6 | 389.5 | 1.1 | 1,471.0 | 1,827.2 | 0.805 | 1,490.8 | 0 / 420 |
| 20 | 10 | 390.5 | 417.5 | 3.8 | 6,131.7 | 7,676.8 | 0.799 | 6,197.7 | 0 / 910 |
| 50 | 5 | 614.8 | 693.8 | 33.3 | 39,211.2 | 49,968.2 | 0.785 | 39,653.6 | 0 / 3005 |
| 100 | 3 | 1584.3 | 1639.5 | 189.4 | 157,799.7 | 202,544.0 | 0.779 | 159,486.7 | 0 / 7353 |

- **Correctness:** 138 NN solves, 100% verified, and no orbit needed the baseline fallback.
- **Moves:** the network needs 0.44–0.81× the baseline's moves; ~0.78× from N=7 up, where center orbits dominate.
- **Time:** N=100 takes 1.6 s end to end (goal < 5 s). N=10 takes 356 ms (goal < 200 ms, not met). At small N the time is per-round Python and GPU-launch overhead, not compute (the baseline takes < 1 ms). The N=7 solve is slower than N=10 because odd N has the extra MidEdge and PlusCenter types (more sequential forward passes).

## Network, first full training run (M6.6)

Run `20260928-2351-default`: default config (d=256, 4 layers, batch 1024 × 4 actions, 100k steps,
ADR-014/015 + adaptive type weights), RTX 4060 laptop, 22,572 s (6.3 h). Every type reached
curriculum depth K=30 (24-slot types by step ~21–25k; MidEdge only at step 88k).

Eval of `step_100000.pt` on 4,096 uniform random states per type, greedy step cap 64, beam width 8,
cost ratio against the baseline on the same states. Reproduce:
`cd python; uv run python -m nxnn.evaluate --checkpoint ../artifacts/checkpoints/20260928-2351-default/step_100000.pt --states 4096 --beam 8`

| Type | greedy solved | beam-8 solved | NN mean cost | baseline mean cost | ratio | µs / decision |
|---|---|---|---|---|---|---|
| Corner | 100.00% | 100.00% | 36.28 | 77.58 | **0.468** | 72.2 |
| MidEdge | **93.99%** | **99.73%** | 40.47 | 73.30 | **0.553** | 46.8 |
| Wing | 99.93% | 100.00% | 106.74 | 107.55 | 0.992 | 51.4 |
| XCenter | 100.00% | 100.00% | 64.86 | 84.68 | **0.766** | 49.4 |
| PlusCenter | 100.00% | 100.00% | 64.06 | 82.88 | **0.773** | 50.3 |
| ObliqueA | 100.00% | 100.00% | 64.19 | 83.22 | **0.771** | 51.1 |
| ObliqueB | 100.00% | 100.00% | 64.07 | 83.04 | **0.772** | 50.5 |

Against the M6 acceptance (greedy ≥ 99.5%, beam-8 with 0 failures on 100k states, cost ≤ 1.0× baseline, goal ≤ 0.85×):
- **Cost:** met for every type; the ≤ 0.85× goal is met for all but Wing (0.99×).
- **Greedy ≥ 99.5%:** met for all but **MidEdge (94.0%)**. It plateaued at 89–95% from step 55k on.
- **Beam-8, 0 failures:** MidEdge fails (11 of 4,096 unsolved). The other types had 0 failures on 4,096; the 100k-state check has not been run.

The solver falls back to the baseline for any orbit the network does not finish, so solves stay 100% verified either way.

### MidEdge follow-up (2026-09-29)

**Fine-tune:** resumed `step_100000.pt` for 30k steps with MidEdge sampled 6× as often (`configs/midedge_finetune.yaml`, run `20260929-midedge-finetune`, 1.85 h). Same 4,096-state eval of `step_130000.pt`: MidEdge greedy 94.82% (was 93.99%), beam-8 99.88% (was 99.73%), ratio 0.551. Other types unchanged (Corner 0.467, Wing 99.95% at 0.991, centers 100% at 0.766–0.773). **More training did not fix MidEdge.**

**Why it fails:** all 236 greedy failures (of 4,096, step 130k) are revisit loops after 5–11 actions, not the 64-step cap. At the loop, every one has 0 flipped pieces and 231 of 236 have exactly 4 misplaced pieces, i.e. a double swap, which needs two 3-cycles where the first one looks like a step backwards.

**Tabu greedy:** at each step, take the best action whose resulting state has not been visited (up to the 16 best). On the same 4,096 states it solves **100%** of MidEdge, at 0.572× baseline cost (step 130k) or 0.581× (step 100k), in at most 26 actions. Now in the solver and in `nxnn.evaluate` (ADR-017). `nxnn.evaluate` of step 130k with tabu: **100% on all seven types** (4,096 states each); tabu cost ratios are Corner 0.467, MidEdge 0.568, Wing 0.991, the four center types 0.766–0.773. Beam-8 (which has no tabu rule) is still 99.88% on MidEdge, so the "beam-8, 0 failures on 100k" check is not met for MidEdge and has not been run at 100k for the others.

## Baseline solver (M4)

Deterministic baseline (no network), uniform random states (CONVENTIONS §6), seeds `0..count`.
Library `88e142fb…` (M3.5). Release build, RTX 4060 laptop (CPU only here), Windows 11, 2026-09-28.
"Raw" is the emitted move list; "cancelled" is after merging same-layer turns (ARCHITECTURE §7).
Every solve is verified by replaying both lists (ADR-007); time includes validation, planning,
emission, cancellation and both replays.

Reproduce: `target/release/nx solve --baseline --n N --seed 0 --count C --random-state`
(full sweep log: `runs/m4-acceptance.log`, not committed).

| N | solves | verified | mean raw moves | mean cancelled moves | mean time (ms) | max time (ms) |
|---|---|---|---|---|---|---|
| 2 | 1000 | 1000 | 78.3 | 75.6 | 0.06 | 1.64 |
| 3 | 1000 | 1000 | 152.7 | 147.5 | 0.22 | 9.18 |
| 4 | 1000 | 1000 | 271.6 | 264.5 | 0.15 | 0.67 |
| 5 | 1000 | 1000 | 427.9 | 416.5 | 0.31 | 1.43 |
| 6 | 1000 | 1000 | 628.9 | 614.7 | 0.31 | 1.15 |
| 7 | 1000 | 1000 | 867.9 | 847.7 | 0.41 | 2.25 |
| 8 | 1000 | 1000 | 1152.6 | 1128.3 | 0.44 | 2.79 |
| 9 | 1000 | 1000 | 1478.7 | 1446.6 | 0.60 | 3.72 |
| 10 | 1000 | 1000 | 1847.4 | 1810.1 | 0.77 | 3.68 |
| 11 | 1000 | 1000 | 2251.3 | 2204.8 | 0.73 | 1.59 |
| 12 | 1000 | 1000 | 2705.0 | 2651.5 | 0.72 | 4.03 |
| 13 | 1000 | 1000 | 3193.5 | 3128.8 | 0.81 | 2.30 |
| 14 | 1000 | 1000 | 3728.8 | 3656.8 | 0.97 | 3.27 |
| 15 | 1000 | 1000 | 4299.3 | 4214.4 | 1.31 | 4.57 |
| 16 | 1000 | 1000 | 4920.4 | 4826.5 | 1.58 | 15.09 |
| 17 | 1000 | 1000 | 5573.6 | 5465.5 | 1.61 | 6.02 |
| 18 | 1000 | 1000 | 6275.3 | 6156.4 | 1.74 | 6.51 |
| 19 | 1000 | 1000 | 7013.3 | 6878.7 | 2.03 | 10.13 |
| 20 | 1000 | 1000 | 7795.1 | 7649.1 | 2.38 | 6.30 |
| 50 | 100 | 100 | 50,552.8 | 49,636.7 | 23.55 | 35.81 |
| 100 | 10 | 10 | 204,865.2 | 201,195.0 | 159.84 | 179.19 |
| 400 | 2 | 2 | 3,309,551.0 | 3,250,643.5 | 6,843.78 | 6,844.43 |

Notes:
- Move counts grow as ~20·N² (every center orbit costs ~12 actions of ~9 moves). This is the
  benchmark the network must beat (ARCHITECTURE §9 acceptance: mean cost ≤ 1.0× baseline).
- At N ≥ 50 almost all time is the two verification replays; planning N=400 (39,801 orbits)
  takes ~45 ms.
- Small N (2, 3) solutions are long compared with optimal (e.g. N=3 ≈ 150 moves): the core is
  solved by 3-cycles and orientation pairs, not by a 3×3 method. Improving the core is stretch
  goal 1 (ARCHITECTURE §16).

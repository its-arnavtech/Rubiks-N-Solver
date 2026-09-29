# Results

Measured numbers only. Each section says how to reproduce it.

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

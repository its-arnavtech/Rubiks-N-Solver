# Milestones & Task List

This is the ordered build plan. **Work top to bottom.** A milestone is done only when all its acceptance checks pass.

- Tick a task (`[x]`) in the same commit that completes it.
- Task ids (`M3.2`) are used in commits and in CHANGELOG.md.
- Tasks marked **(USER)** need the human, e.g. admin rights, a reboot, or a long GPU run they should watch. Prepare everything, then write the exact commands in CHANGELOG under "Needs user".

Design reference: [ARCHITECTURE.md](ARCHITECTURE.md) · Formats: [CONVENTIONS.md](CONVENTIONS.md) · Why: [DECISIONS.md](DECISIONS.md)

---

## M0 — Repo preparation
- [x] **M0.1** The git repo has **no commits** and a stale `.git/index.lock`. Remove the lock, then make an initial commit of the current tree, tagged `legacy-graph-theory`. Then commit these docs.
- [x] **M0.2** Old docs moved to `docs/legacy/`, new README, AGENTS/CLAUDE/CHANGELOG added. *(done during architecture; see CHANGELOG)*
- [x] **M0.3** Add workspace members `crates/nx-sim`, `nx-macro`, `nx-solve`, `nx-cli`, `nx-wasm`, `nx-py` (empty skeletons that build).
  - Exclude `nx-py` from `default-members`, because PyO3's `extension-module` feature is only enabled via maturin.
  - `nx-py` declares its own `[lints]` (PyO3 needs unsafe).
- [x] **M0.4** `python/` uv project, package `nxnn`, Python 3.12:
  - deps: torch (CUDA wheels via `[tool.uv.sources]`, see SETUP.md), numpy, pydantic, pyyaml, typer, tensorboard, fastapi, uvicorn, httpx, pytest, hypothesis, maturin
  - add `configs/smoke.yaml` and `configs/default.yaml`
- [x] **M0.5** `justfile` with `set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]`. Recipes: `setup`, `check`, `test`, `fmt`, `discover`, `verify-library`, `baseline`, `py-build`, `train`, `eval`, `solve`, `serve`, `web`, `bench`.
  - Update `.gitignore`: `artifacts/checkpoints/`, `runs/`, `python/.venv*`, `target-wsl/`, `__pycache__/`.
- [x] **M0.6** `.github/workflows/ci.yml` (Linux, CPU): fmt, clippy `-D warnings`, cargo test, pytest `-m "not gpu"`, wasm build, `pnpm check && pnpm build`.
- [ ] **M0.7 (USER)** Environment per SETUP.md: WSL2 attempt, time-boxed; otherwise native Windows. Record the outcome in CHANGELOG.

**Accept:** `just check` is green on Windows. CI is green.

## M1 — Simulator (`nx-sim`)
- [x] **M1.1** Geometry and facelet layout: port `rg-cube::geometry::sticker_position`. `Axis`, `Move {axis, layer: u32, turns}`, wire encode/decode, display notation (CONVENTIONS §1–2).
- [x] **M1.2** Fast `apply`:
  - index-arithmetic strips for the 4 side faces of a layer, plus face rotation for layer 0
  - no per-move full permutation
  - `Cube` (u8 colors) and `LabeledCube` (u32 home index)
- [x] **M1.3** Tests:
  - `rg-cube` oracle: every allowed move for N = 2..7 matches (rg-cube is a dev-dependency)
  - property tests on random N ∈ [2, 64]: inverse, order 4, same-axis commute, DLB fixed
- [x] **M1.4** `scramble(n, len, seed)` over allowed moves, with `ChaCha8Rng`.
- [x] **M1.5** criterion benches: inner move at N=400 ≤ 10 µs; face move ≤ 1 ms; 1M-move replay at N=100 ≤ 2 s.
- [x] **M1.6** `nx-wasm` compiles: `apply_moves(n, facelets, moves)`. `parallel` feature off for wasm.

**Accept:** all tests pass, budgets met, wasm builds.

## M2 — Orbits, identity, parity, validation
- [x] **M2.1** Orbit computation (union-find over move permutations at piece level), classification, ids (CONVENTIONS §3).
- [x] **M2.2** Canonical slot maps (CONVENTIONS §4): `extract(cube, orbit) -> [u8; 24]` and `insert`. Per-orbit solved check.
- [x] **M2.3** Identity from colors (CONVENTIONS §5), parity functions, FixedCenter frame state.
- [x] **M2.4** `validate(cube)` (color counts, orientation sums, 3×3 law, wing ids) and `random_state(n, seed)` (CONVENTIONS §6).
- [x] **M2.5** `nx orbits <N>` prints a table of orbit types and counts.

**Tests:**
- counts equal ARCHITECTURE §5 formulas for N = 2..40
- every sticker is in exactly one orbit
- every move maps each orbit to itself
- slot maps are bijective
- wing identity from colors equals labeled identity
- `random_state` passes `validate`

## M3 — Macros & action library (`nx-macro`)  ← hard gate before any training
- [x] **M3.1** `SymMove`, `LayerRef`, binding and instantiation, inverse, and a move-cancellation utility (shared with M4).
- [x] **M3.2** Discovery search per type (ARCHITECTURE §6.2): configurable limits, rayon, probe N = 12.
- [x] **M3.3** Action expansion by setups of length ≤ 2. Slot `perm`/`ori_delta`/`cost`.
- [x] **M3.4** `nx verify-library`: purity for N = min..18 over all instances plus spot checks N ∈ {31, 64, 101}, invariance, coverage (ARCHITECTURE §6.3).
- [ ] **M3.5** Write `artifacts/macros/library.json` (canonical JSON + sha256). Determinism test. Commit the file.

**Accept:** every type passes purity, invariance and coverage. Record action counts per type in CHANGELOG. Verification takes ≤ 10 min.

## M4 — Baseline solver (no neural net)  ← proves the whole pipeline
- [ ] **M4.1** Phase 0 wing parity. Phase 1a FixedCenter BFS. Phase 1b corner-parity quarter turn.
- [ ] **M4.2** Orbit-space simulation in Rust from library perms. Baseline orbit solver (cycle-sort; color-type assignment + parity trick).
- [ ] **M4.3** Emit (instantiate actions), cancel, verify. `SolveResult` with serde, matching CONVENTIONS §8.
- [ ] **M4.4** `nx solve --baseline --n <N> --seed <S> [--random-state]`.
- [ ] **M4.5** Test: orbit space ≡ real cube for random N ∈ [4, 30].

**Accept:** 100% verified solves: 1,000 random states for each N = 2..20, 100 at N=50, 10 at N=100, 2 at N=400. Record baseline move counts and timings in `docs/nn/RESULTS.md`.

## M5 — Python bridge & training environments
- [ ] **M5.1** `nx-py` via maturin. `just py-build` produces the `nxsim` module on Windows. Smoke test `import nxsim`.
- [ ] **M5.2** `nxnn.library` (load + sha256 check) and `nxnn.envs` (per-type GPU tensors, apply, solved check, scramble, uniform random states in torch). **No `nxsim` import in envs/model/train.**
- [ ] **M5.3** Test: envs ≡ real cube (Python, via `nxsim`).
- [ ] **M5.4** `nxnn.baseline` (orbit-level, torch/numpy) for eval metrics. Test: it solves 10k random states per type.

## M6 — Network & training
- [ ] **M6.1** `nxnn.config` (pydantic) + YAML configs: smoke (CPU, tiny) and default (4060).
- [ ] **M6.2** `nxnn.model` (ARCHITECTURE §8). Tests: shapes, `Q ≥ cost`, action masking.
- [ ] **M6.3** `nxnn.train`: Q-iteration, target net, curriculum, bf16, resume, checkpoints with `library_sha256` + git commit, TensorBoard.
- [ ] **M6.4** `nxnn.evaluate`: greedy/beam solve rate, mean cost, baseline ratio, latency per type.
- [ ] **M6.5** Smoke test: 200 steps on CPU < 2 min, no NaN, loss decreases (runs in CI).
- [ ] **M6.6 (USER watches)** Full training run with `just train default`. Watch TensorBoard. On acceptance, write `artifacts/checkpoints/CURRENT`.

**Accept per type:** greedy ≥ 99.5%; beam-8 has 0 failures on 100k; mean cost ≤ 1.0× baseline (goal 0.85×). Record results in RESULTS.md.

## M7 — Neural solver pipeline
- [ ] **M7.1** `nxnn.solve`: phases via `nxsim`, batched rounds per type, beam option, step cap, revisit guard, baseline fallback, emit/cancel/verify via `nxsim`, segment recording.
- [ ] **M7.2** CLI: `python -m nxnn.solve --n N --seed S --solver nn|baseline --beam W`.
- [ ] **M7.3** Benchmark N ∈ {2, 3, 4, 5, 7, 10, 20, 50, 100}, NN vs baseline: total time, raw/cancelled moves, fallback count → RESULTS.md.

**Accept:** 100% verified. N=100 end-to-end measured (goal < 5 s).

## M8 — Local web UI
- [ ] **M8.1** `nxnn.server` (FastAPI, CONVENTIONS §8) + pytest/httpx contract tests.
- [ ] **M8.2** `nx-wasm`: replay, orbit map per N, snapshots. `cargo xtask wasm` builds `nx-wasm` instead of `rg-wasm`.
- [ ] **M8.3** Remove the legacy UI (old solver worker, graph views). New Zustand store and API client.
- [ ] **M8.4** Cube rendering: `InstancedMesh` 3D (turn animation for N ≤ 10) + 2D net view (default for N > 20).
- [ ] **M8.5** Orbit overlay, per-type progress panel, phase indicator, round timeline, playback modes (move/action/round), speed control.
- [ ] **M8.6** Orbit inspector: click a sticker to see its orbit, its 24-slot diagram, and the action history with Q-values.
- [ ] **M8.7** `just serve` starts the server + web. README "See it run" section.

**Accept:** in the browser, for N ∈ {3, 4, 7, 20, 50, 100}, a random state is solved by the NN with the orbit progression visible, and the final state shows solved. `pnpm check && pnpm build` passes.

## M9 — Stretch (only when the user asks)
Move-level core network · ONNX + `ort` · multi-orbit macros · learned macro proposal.

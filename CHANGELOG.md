# CHANGELOG

Handoff log for all agents. See AGENTS.md §4 for how to update it.
- **"Current state"** is overwritten every session.
- **"Log"** is append-only, newest first.

---

## Current state

- **Project:** NxN neural cube solver (see `docs/nn/ARCHITECTURE.md`)
- **Active milestone:** M5 — Python bridge & training environments. M1–M4 accepted. M0 still waits on M0.7 (recorded?) and a first CI run.
- **Last completed task:** M4 — baseline solver, acceptance 100% verified (`docs/nn/RESULTS.md`)
- **In progress:** none
- **Next task:** M5.1 (`nx-py` via maturin: expose the ARCHITECTURE §11 API as `nxsim`).
- **Environment note:** `python/.venv` already has torch 2.14.0+cu126 with CUDA available on the RTX 4060 (driver 610.74), Python 3.12.13, so native Windows works. The user still needs to confirm M0.7 (WSL2 tried or not).
- **Blockers:** none
- **Needs user:** M0.7. Try WSL2 per `docs/nn/SETUP.md` §2, time-boxed; otherwise use native Windows. Report which one. M0's "CI is green" also needs the user to push.
- **How to verify:** `just check` (needs `just`: `winget install Casey.Just` or `cargo install just`). `cargo test -p nx-sim` for the simulator alone; `just bench` for the M1.5 budgets; `cargo build -p nx-wasm --target wasm32-unknown-unknown`. Plain `cargo build` skips `nx-py`. Python: `cd python; uv sync --no-install-package torch; uv run --no-sync pytest`.
- **Last updated:** 2026-09-28 by Claude Code (Opus 5.5)

---

## Log

### Entry template
```
### YYYY-MM-DD — <agent name/model> — <task ids>
**Done:** what was completed (with task ids)
**Files:** main files added/changed
**Tests:** what was run and the result (e.g. `just check` green; 1000/1000 verified at N=2..20)
**Decisions:** ADRs added/changed, conventions changed (and library_version bump if any)
**Problems / open questions:** anything unresolved
**Next:** the exact next step
```

### 2026-09-28 — Claude Code (Opus 5.5) — M4.1–M4.5
**Done:** M4. `nx-solve`: `phases.rs` (phase 0: one `x` slice quarter turn at layer p per odd wing orbit; 1a: BFS over the 24 frame states with MID turns on a 3×3, bound to MID; 1b: `U` if corner parity is odd), `orbit_solver.rs` (`SolverLib`/`KindLib`: cheapest action per directed 3-cycle and per orientation pair; cycle-sort; a missing 3-cycle is split as `(x→y→w)(w→z→x)`; orientation fixed with pairs; color orbits get a target assignment that keeps home pieces, pairs up 2-cycles, and fixes parity by swapping two same-colored targets), `solve.rs` (validate → phases → corners → middle edges (applied to the work cube, since core actions disturb non-core orbits) → all other orbits in parallel → emit with segments → cancel → verify raw and cancelled lists in parallel → `SolveResult`). `nx solve --baseline --n N --seed S [--random-state] [--len L] [--count C] [--json F]`. nx-sim `apply_all` now keeps outer face turns as pending per-face rotations (derived "lazy" strips per face rotation), so outer moves cost O(N): N=100 solve went from 1.45 s to 0.16 s.
**Files:** `crates/nx-solve/src/{phases,orbit_solver,solve}.rs`, `tests/baseline.rs`, `crates/nx-sim/src/cube.rs`, `crates/nx-cli/src/main.rs`, `docs/nn/RESULTS.md`, CONVENTIONS §8.
**Tests:** `cargo test` green; clippy clean. nx-solve: phases (frame BFS from all 24 rotations, parity phases), assignment (even, color-preserving), random states N=2..14 × 12 seeds solve and verify, segments tile the raw list, scrambles and solved cubes, invalid input rejected, JSON round trip, M4.5 orbit space ≡ real cube (30 random cases N ∈ [4, 30]). nx-sim: lazy `apply_all` = move-by-move for N=2..20 including layer N−1. **Acceptance:** 1,000/1,000 verified for every N=2..20, 100/100 at N=50, 10/10 at N=100, 2/2 at N=400 (numbers in `docs/nn/RESULTS.md`; N=100 mean 160 ms, N=400 6.8 s).
**Decisions:** `SolveResult` gains `cancelled_moves_b64` (CONVENTIONS §8); `moves_b64` stays the raw list that segments index. Phase 1b segments use phase `parity`.
**Problems / open questions:** Baseline move counts are ~20·N² and the core is solved inefficiently (N=3 ≈ 150 moves). Fine as a fallback/benchmark.
**Next:** M5.1.

### 2026-09-28 — Claude Code (Opus 5.5) — M3.1–M3.5
**Done:** M3. `nx-macro`: `sym.rs` (`LayerRef`, `SymMove` `"x:A_BAR:3"`, `Binding`, instantiate/invert/conjugate); `nx_sim::cancel` (generic same-axis-run cancellation, shared with M4). `effect.rs` (slot `Effect` compose/inverse/apply/classify, `OrbitProbe` reads effects from labeled cubes). `discover.rs`: commutators over canonical generator sequences on a labeled probe, evaluated with early exit (bail on >9 moved stickers or any sticker outside the target orbit), `[Y,X]` taken as the inverse of `[X,Y]`; rayon. `library.rs`: expansion `S·M·S⁻¹` for all canonical setups ≤ 2, cheapest per effect; canonical JSON + sha256; load/save. `verify.rs`: purity + invariance at every instance N=min..18 and 10 sampled instances at N=31/64/101 (per-thread cube, apply + undo), plus coverage. CLI: `nx discover [--check]`, `nx verify-library`.
**Action counts** (library `88e142fb01614dfa59ccc2f37b23ce195189e88c5314050ab15946b9fe36b729`):

| Type | macros found | macros used | actions | mean cost | max cost |
|---|---|---|---|---|---|
| Corner | 404 | 124 | 672 (630 3-cycles + 42 twist pairs: complete) | 12.32 | 19 |
| MidEdge | 1,684 | 376 | 1,826 (1,760 + 66 flip pairs: complete) | 8.36 | 13 |
| Wing | 696 | 344 | 4,024 of 4,048 | 9.71 | 12 |
| XCenter | 1,620 | 712 | 4,048 | 8.91 | 12 |
| PlusCenter | 2,064 | 817 | 4,048 | 8.67 | 10 |
| ObliqueA | 2,040 | 799 | 4,048 | 8.72 | 12 |
| ObliqueB | 2,040 | 799 | 4,048 | 8.72 | 12 |

**Files:** `crates/nx-macro/src/{sym,effect,discover,library,verify}.rs`, `tests/library_file.rs`, `crates/nx-sim/src/moves.rs` (`cancel`, `Turn`), `crates/nx-cli/src/main.rs`, `artifacts/macros/library.json` (4.7 MB, one line), `.gitattributes`.
**Tests:** `nx discover` 5 s; `just verify-library` all 7 types OK in **3.7 s** (budget 10 min): 2.3M action×instance checks. `discover --check` = committed bytes. nx-macro tests: verifier catches a wrong declared perm, an impure macro and coverage gaps; regenerated library = committed; orbit space ≡ real cube through *color* extraction for 40 random states N ∈ [4, 30].
**Decisions:** (1) Corner generators are `OUTER` only (ARCHITECTURE §6.1): macros then exist at every N. (2) No pure corner twist pair is a short R/U/F commutator, so discovery also keeps products of two pure 3-cycle macros that form an orientation pair (ARCHITECTURE §6.2). (3) Core types use wider limits (up to `(6,1)`, `(4,2)`, `(3,3)`), cheap because they are evaluated on core stickers only; probes: core and odd-only types at N=13, others at N=12 with `a=2` (`b=4` obliques). (4) `sha256` excludes `generator.git_commit` (CONVENTIONS §7). (5) The "~2,024 actions" estimate was the undirected count; directed 3-cycles on 24 slots are 4,048. `library_version` stays 1 (first library).
**Problems / open questions:** 24 Wing 3-cycles are not reachable with setups ≤ 2 (coverage still holds; the network just can't use those directly). Coverage for orientation is shown by generation (a pure pair exists, or a 3-cycle whose cube is a pure twist), not by listing all twist states.
**Next:** M4.1.

### 2026-09-28 — Claude Code (Opus 5.5) — M2.1–M2.5
**Done:** M2. `orbits.rs`: union-find over the quarter-turn 4-cycles of every layer (incl. N−1, so the fixed DLB corner joins the corner orbit) plus piece joins, then classification per CONVENTIONS §4 and id order per §3. `slots.rs`: `SlotMap` (stickers per canonical slot for every kind), `extract_labeled`, `solved_content`/`is_orbit_solved`. `identity.rs`: `SlotMap::extract` from colors (corners/middle edges via Kociemba face order, wings via a lookup table built by exhaustive placement at N=4, centers = color), `SlotMap::insert`, permutation/piece parity, the 24 fixed-center frame rotations (BFS over MID turns) with `frame_index`/`frame_parity`. `validate.rs`: `validate` and `random_state` (RNG order: frame, corners, middle edges, then the other orbits by id). `nx orbits <N> [--list]`.
**Files:** `crates/nx-sim/src/{orbits,slots,identity,validate}.rs`, `cube.rs` (`for_each_quarter_cycle`, `set_stickers`), `crates/nx-cli/src/main.rs`, `docs/nn/CONVENTIONS.md` §4.
**Tests:** `cargo test -p nx-sim` 38 tests green; workspace clippy clean. Counts = ARCHITECTURE §5 formulas for N=2..40; each sticker in exactly one orbit (N ≤ 24); every allowed move maps each orbit to itself (N ≤ 13); slot maps are bijections onto the union-find orbits (N ≤ 20); color identity = labeled identity for every orbit (N=2..20, scrambles); insert∘extract rebuilds scrambled cubes; corner-twist and flip sums and the 3×3 parity law hold on scrambles; `random_state` and scrambles pass `validate` for N=2..30; `validate` rejects a twisted corner, a flipped middle edge, two swapped middle edges (parity law), a moved DLB, a duplicated wing, a non-piece, wrong color counts; it accepts a wing swap and an even-N corner swap. `nx orbits 100`: 2,451 orbits (49 wing, 49 x-center, 1,176 + 1,176 oblique) in 5.5 ms; N=400: 39,801 orbits in 62 ms.
**Decisions:** CONVENTIONS §4 wing slot bit `s` changed: `s = 0` iff `(n_f1 × n_f2) · cubie > 0` (handedness). The first draft ("s = 0 at t = p") is not chirality-consistent on every edge, and the table build proved colors + s then do not determine identity. No library exists yet, so `library_version` stays 1.
**Problems / open questions:** `validate` requires the DLB corner solved and untwisted (our frame). Inputs in another whole-cube orientation are rejected, not re-oriented; the server/web only send frame states, so this is fine for now.
**Next:** M3.1.

### 2026-09-28 — Claude Code (Opus 5.5) — M1.1–M1.6
**Done:** M1 (simulator). M1.1 `geometry.rs` (port of `rg-cube` `sticker_position` + arithmetic inverse `sticker_at`, reference `move_permutation`), `moves.rs` (`Axis`, `Move {axis, layer: u32, turns}`, wire `u32` encode/decode, notation `R`/`2R'`/`13U2` both ways, `allowed_moves`). M1.2 `cube.rs`: `Layout` derives, per N, four affine side strips per axis from the 3D geometry; a move is a 4-cycle over N strip stickers plus an in-place face rotation for layer 0 (and N−1, which only tests use). `CubeState<T>` with `Cube = <u8>` and `LabeledCube = <u32>`. M1.3 oracle and property tests. M1.4 `scramble_moves`/`Cube::scramble` (uniform over allowed moves, re-draws a move on the same axis+layer as the previous one; stream pinned by a test). `rng.rs`: `ChaCha8Rng` from a u64 seed with our own rejection sampling (`below`, `between`, `shuffle`), so streams don't depend on `rand` versions. M1.5 criterion benches. M1.6 `nx-wasm` `applyMoves(n, facelets, moves: u32[])` + `solvedFacelets`; nx-sim `parallel` feature (optional rayon, off by default).
**Files:** `crates/nx-sim/{Cargo.toml, src/{lib,geometry,moves,cube,rng,scramble}.rs, tests/moves.rs, benches/moves.rs}`, `crates/nx-wasm/src/lib.rs`, root `Cargo.toml` (workspace deps `rand_chacha` 0.9 no-default-features, `rayon`).
**Tests:** `just check` green. nx-sim: fast path = geometry permutation for every layer (incl. N−1) N=2..12; = `rg-cube` for every move N=2..7 and for 200-move color sequences; properties (inverse, order 4, same-axis commute, DLB fixed) on 60 random N ∈ [2, 64]; round trips at N=100/257/400. Benches (release, this laptop): inner move N=400 **1.76 µs** (budget 10 µs), face move N=400 **151 µs** (1 ms), 1M-move replay N=100 **654 ms** (2 s). wasm32 build OK.
**Decisions:** none. Moves on layer N−1 are supported by `apply` (for the oracle) but are not "allowed"; `nx-wasm` rejects them.
**Problems / open questions:** the M1.2 commit accidentally re-encoded MILESTONES.md (BOM); M1.3 restored it. Lesson: don't round-trip docs through Windows PowerShell 5.1 `Get-Content`/`Set-Content` (it reads UTF-8 as ANSI). Use the editor tools.
**Next:** M2.1.

### 2026-09-28 — Claude Code (Sonnet 5.5) — M0.6
**Done:** M0.6. `.github/workflows/ci.yml` with three Linux jobs. `rust`: fmt --check, clippy `-D warnings` (whole workspace incl. `nx-py`, with Python 3.12), `cargo test`. `python`: `uv sync` without torch, then the CPU torch wheel over the CUDA lock, maturin build of `nxsim`, `pytest -m "not gpu"`. `web`: matching `wasm-bindgen` installed from the `Cargo.lock` version, `nx-wasm` wasm32 build, `cargo xtask wasm`, `pnpm check`, `pnpm build`.
**Files:** `.github/workflows/ci.yml`.
**Tests:** the workflow has NOT run on GitHub (no remote; nothing is pushed unless the user asks). I checked that the YAML parses and ran each command locally on Windows: `cargo xtask wasm`, `pnpm --dir web check`, `pnpm --dir web build`, the wasm-bindgen version extraction (0.2.128), `just check`, `just py-build`. Not exercised locally: the Linux-only steps (`uv pip install torch --index-url .../cpu`, `taiki-e/install-action` for wasm-bindgen, action versions `@v4/@v5/@v2`).
**Decisions:** none.
**Problems / open questions:** `xtask wasm` still builds `rg-wasm` (changes at M8.2). `xtask ci` uses `cargo test --workspace`, which would include `nx-py`; it isn't used by CI or `just`, so it was left alone. M0's acceptance line "CI is green" needs a push and a first run; expect possible small fixes to the workflow then.
**Next:** M0.7 (USER). Meanwhile M1.1 can start.

### 2026-09-28 — Claude Code (Sonnet 5.5) — M0.5
**Done:** M0.5. `justfile` with `windows-shell` set to PowerShell and all 14 recipes: setup, check, test, fmt, discover, verify-library, baseline, py-build, train, eval, solve, serve, web, bench (plus `pytest` and a default list). Recipes for commands that don't exist yet (discover, verify-library, baseline, train, eval, solve, serve, bench) are wired to their planned CLIs and fail until those milestones land. Python recipes use `[working-directory: 'python']` so config paths like `../artifacts/...` resolve. `.gitignore` gained `artifacts/checkpoints/`, `runs/`, `python/.venv*`, `target-wsl/`, `__pycache__/`, `*.pyc`, `.pytest_cache/`, `.hypothesis/`. Removed two `.pyc` files that M0.4 committed by mistake.
**Files:** `justfile`, `.gitignore`.
**Tests:** installed `just` 1.58.0 via `cargo install just --locked` (user-level, in `~/.cargo/bin`; not on the Git Bash PATH by default). `just check` passes end to end (fmt, clippy -D warnings, cargo test, pytest -m "not gpu", biome). `just py-build` also works: maturin builds `nx-py` and `import nxsim; nxsim.version()` works in `python/.venv`, which was a bonus ahead of M5.1.
**Decisions:** none.
**Problems / open questions:** `just serve` only starts the API server for now; M8.7 must add starting the web dev server too (cross-platform, in Python or xtask). `cargo test` also runs the legacy `rg-*` tests, which are slow (~30 s for `rg-solve`).
**Next:** M0.6.

### 2026-09-28 — Claude Code (Sonnet 5.5) — M0.4
**Done:** M0.4. `python/pyproject.toml` (hatchling build, package `nxnn`, `requires-python >=3.12,<3.13`, all deps from the milestone; `maturin` in the `dev` group with pytest and hypothesis). Torch is pinned to the cu126 index via `[tool.uv.sources]`. `uv lock` resolved torch 2.14.0+cu126. Added `configs/smoke.yaml` (CPU, tiny model, 200 steps) and `configs/default.yaml` (ARCHITECTURE §8–9 sizes) plus a `gpu` pytest marker and one import test.
**Files:** `python/pyproject.toml`, `python/uv.lock`, `python/nxnn/__init__.py`, `python/tests/test_package.py`, `python/configs/*.yaml`.
**Tests:** `uv sync --no-install-package torch` then `pytest` → 1 passed. Both YAML files load. The full torch install and CUDA check were NOT done (M0.7, user's machine setup).
**Decisions:** none. The config key layout (model/train/curriculum/eval/checkpoint) is a first draft; M6.1's pydantic schema is the authority and may rename keys.
**Problems / open questions:** the cu126 index is applied on every platform, as SETUP.md says. Linux CI will download the CUDA wheel unless M0.6 overrides it (M0.6 decides).
**Next:** M0.5.

### 2026-09-28 — Claude Code (Sonnet 5.5) — M0.3
**Done:** M0.3. Added `nx-sim`, `nx-macro`, `nx-solve`, `nx-cli` (binary `nx`), `nx-wasm`, `nx-py` (PyO3 module `nxsim` with a `version()` function). Root `Cargo.toml` has `default-members` without `nx-py`. `nx-py` has its own `[lints]` (unsafe allowed). Added workspace deps `nx-sim`, `nx-macro`, `nx-solve`, `pyo3 = "0.29"`. `extension-module` is not enabled in Cargo.toml; maturin will enable it in M0.4/M5.1.
**Files:** `Cargo.toml`, `crates/nx-*/`; `crates/rg-solve/src/nxn5.rs` reformatted by `cargo fmt` (whitespace only, needed for the fmt gate).
**Tests:** `cargo build --workspace` OK; `cargo clippy --workspace --all-targets -- -D warnings` OK; `cargo fmt --check` OK; `nx-wasm` builds for `wasm32-unknown-unknown`; `cargo run -p nx-cli` prints the skeleton banner.
**Decisions:** none.
**Problems / open questions:** `just` is not installed on this machine yet (M0.5 writes the justfile, but `just check` can't be run until `winget install Casey.Just`). Local Python is 3.10; the `python/` project pins 3.12 through uv.
**Next:** M0.4.

### 2026-09-28 — Claude Code (Sonnet 5.5) — M0.1
**Done:** M0.1. `.git/index.lock` was empty and no git process was running, so it was removed. Committed the legacy tree (rg-* crates, web, xtask, docs/legacy) as `cc2bf05`, tagged `legacy-graph-theory`. The new project docs (AGENTS/CLAUDE/CHANGELOG/README, docs/nn) are in the following commit.
**Files:** none changed; git history only.
**Tests:** none (no code changes).
**Decisions:** none.
**Problems / open questions:** `core.autocrlf=true` on this machine prints LF→CRLF warnings on add; harmless.
**Next:** M0.3.

### 2026-09-28 — Claude (Cowork) — M0.2
**Done:** Designed the full architecture with the user. Wrote the project docs.
**Files:**
- added `AGENTS.md`, `CLAUDE.md`, `CHANGELOG.md`
- added `docs/nn/ARCHITECTURE.md`, `CONVENTIONS.md`, `MILESTONES.md`, `DECISIONS.md`, `SETUP.md`
- replaced `README.md`
- moved the old `README.md` and `docs/01..12-*.md` into `docs/legacy/`

**Tests:** none. No code was written.
**Decisions:** ADR-001 … ADR-013 (orbit decomposition, fixed-corner frame, nx-sim with rg-cube oracle, searched macros, Q-value iteration, one shared network, correctness by code, training decoupled from Rust, local UI, same repo, library hash pins checkpoints, no hidden info, WSL2 time-boxed).
**Problems / open questions:**
- Repo state as found: no commits, and a stale `.git/index.lock` that could not be removed from the architecture session.
- The existing `web/` app (React 19, Vite, three.js, Zustand, Tailwind, pnpm) and `xtask` (wasm build) will be reused. The legacy UI will be replaced in M8.

**Next:** M0.1, then M0.3–M0.6.

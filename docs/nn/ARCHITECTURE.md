# NxN Neural Solver — Architecture

> **Status:** design approved, nothing implemented yet. This is the source of truth for *what* we are building and *why*.
> Exact formats and numbering live in [CONVENTIONS.md](CONVENTIONS.md). The ordered task list lives in [MILESTONES.md](MILESTONES.md).
> Decisions and their reasons live in [DECISIONS.md](DECISIONS.md). Progress lives in [/CHANGELOG.md](../../CHANGELOG.md).

---

## 1. What we are building (one paragraph)

A program that solves a Rubik's cube of **any size N** (2×2 up to 400×400 and beyond; the UI targets N ≤ 100) using **one small, self-trained neural network**. Every NxN cube is made of small groups of pieces called **orbits**: pieces that can only ever swap among themselves. There are only **7 kinds** of orbit, and each one is a puzzle of **24 slots** that looks identical on every cube size. Plain code breaks the cube into orbits and hands them to the network. The network, which never sees N, solves each orbit using short move **tricks (macros)** that the system discovered on its own. Solving is done for all orbits **in parallel on the GPU**, then the full move list is replayed on an exact simulator to prove the cube is solved. A local web page visualizes the orbits being solved.

**What makes it work on any N:** generalization is *built into the design*, not hoped for. The network only ever sees 24-slot puzzles of 7 fixed kinds. N only changes how many orbits there are and which layer numbers a macro binds to.

**Non-goals:** optimal (shortest) solutions. Big cubes need O(N²) moves no matter what. We optimize wall-clock time first and move count second.

---

## 2. Glossary

| Term | Meaning |
|---|---|
| **Facelet / sticker** | One colored square. A cube has 6·N² of them. |
| **Piece (cubie)** | A physical piece: corner (3 stickers), edge (2), center (1). |
| **Orbit** | The set of positions a piece can ever reach under legal moves. Computed from move permutations, never hand-coded. |
| **Orbit type** | One of 7 kinds: `Corner`, `MidEdge`, `Wing`, `XCenter`, `PlusCenter`, `ObliqueA`, `ObliqueB` (+ the special `FixedCenter` frame on odd N, handled by code). |
| **Slot** | One of the 24 canonical positions inside an orbit (canonical order defined in CONVENTIONS §4). |
| **Orbit indices** | The layer numbers that identify an orbit instance, e.g. a wing orbit at depth `p`, an oblique center orbit at `(a, b)`. |
| **Local generators** | The moves that matter for one orbit type: the outer layer plus the slices at that orbit's indices (and their mirrors). |
| **Symbolic move** | A move written with a layer *reference* (`OUTER`, `MID`, `A`, `A_BAR`, `B`, `B_BAR`) instead of a number. Bound to real layers per orbit. |
| **Macro** | A symbolic move sequence whose net effect is a small permutation (3-cycle, twist pair, flip pair) inside one orbit type, verified to touch nothing it shouldn't. |
| **Action** | What the network picks: a conjugated macro `S · M · S⁻¹` (setup, macro, undo setup), identified by `(type, action_id)`, with a fixed primitive-move cost. |
| **Action library** | `artifacts/macros/library.json`: every action per type, its symbolic moves, cost and slot effect. The network's output space. |
| **Core** | Corners (+ middle edges and fixed centers on odd N). Solved before everything else. |
| **Baseline solver** | Deterministic, no neural net. Used as fallback **and** as the benchmark the network must beat. |
| **Verified solve** | The full move list, replayed on `nx-sim` from the scramble, ends in the solved state. The only thing we ever report as a solve. |

---

## 3. System overview

```
                    ┌───────────────────────────── Rust (exact, fast) ─────────────────────────────┐
 scramble ─────────▶│ nx-sim: NxN simulator (any N) ─▶ orbits: enumerate/classify/extract/parity  │
                    │ nx-macro: symbolic moves, macro discovery + verification ─▶ library.json    │
                    │ nx-solve: phase 0 parity, core frame, baseline solver, emit, cancel, verify │
                    └──────────────┬──────────────────────────────────────▲────────────────────────┘
                                   │ PyO3 (nx-py → python module `nxsim`)  │
                    ┌──────────────▼───────────── Python (neural) ────────┴────────────────────────┐
                    │ nxnn.envs: GPU orbit puzzles (tensor gathers from library.json, no Rust)     │
                    │ nxnn.model: shared transformer, Q-head over actions                          │
                    │ nxnn.train: self-play Q-value iteration + curriculum, TensorBoard            │
                    │ nxnn.solve: batched parallel orbit solving + fallback                        │
                    │ nxnn.server: FastAPI on localhost                                            │
                    └──────────────┬───────────────────────────────────────────────────────────────┘
                                   │ HTTP JSON (localhost)
                    ┌──────────────▼───────────── Web (local) ─────────────────────────────────────┐
                    │ React + three.js viewer, nx-wasm replays moves exactly in the browser        │
                    │ orbit overlay, per-type progress, round timeline, orbit inspector            │
                    └──────────────────────────────────────────────────────────────────────────────┘
```

**Key separation (ADR-008):** training (`nxnn.envs`, `nxnn.model`, `nxnn.train`) depends only on PyTorch and `library.json`. It never imports `nxsim`. So training runs anywhere PyTorch + CUDA runs (WSL2 or Windows) without building Rust.

---

## 4. Cube model (`crates/nx-sim`)

**Requirements**
- Any N from 2 up to at least 1024. Sticker indices are `u32`. There is no layer bitmask type limit: a move names **one** layer.
- Facelet layout, axes and turn direction are **identical to `rg-cube`** (CONVENTIONS §1–2), so `rg-cube` is a direct oracle for N ≤ 7.
- **Fixed-corner frame (ADR-002):** the corner at D-L-B never moves. Layer `N−1` on every axis (the L/D/B side) is never turned. Allowed moves are layers `0..=N−2` on axes x, y, z, with 1, 2 or 3 quarter turns. This removes whole-cube-rotation duplicates and makes "solved" unique (up to swaps of identical-colored centers).
- Applying a move costs O(stickers moved): about 4N for an inner slice, plus N² for an outer face. Precomputed permutations are never materialized per move for large N. Moves are computed from index arithmetic (derived from and tested against the 3D geometry).
- Two state modes:
  - `Colors`: `Vec<u8>`, what the solver may see.
  - `Labeled`: `Vec<u32>`, each sticker's home index. Used only for macro verification, parity checks and tests, **never** as input to the solver or network (ADR-012: no hidden information).
- Compiles to `wasm32-unknown-unknown`. Rayon parallelism sits behind a `parallel` feature that is off for wasm.

**API sketch**
```rust
pub struct Cube { n: u32, facelets: Vec<u8> }            // Colors mode
pub struct LabeledCube { n: u32, facelets: Vec<u32> }    // Labeled mode
pub struct Move { axis: Axis, layer: u32, turns: u8 }    // layer in 0..=n-2
impl Cube {
    fn solved(n: u32) -> Self;  fn apply(&mut self, m: Move);  fn apply_all(&mut self, ms: &[Move]);
    fn is_solved(&self) -> bool;  fn scramble(n: u32, len: usize, seed: u64) -> (Self, Vec<Move>);
    fn random_state(n: u32, seed: u64) -> Self;   // uniform over reachable states, see CONVENTIONS §6
}
```

**Random scrambles:** random move sequences are a weak scramble for big N. `random_state` builds a uniformly random *reachable* state directly, orbit by orbit, respecting parity laws. It is used for evaluation.

---

## 5. Orbits (`crates/nx-sim::orbits`)

Orbits are **computed**: union-find over all allowed move permutations at a given N, at the piece level. Then each orbit is classified by its indices. Counts for N ≥ 4, with `q = ⌊(N−2)/2⌋`:

| Type | Pieces | Exists when | Orbits per cube | Slot content |
|---|---|---|---|---|
| `Corner` | 8 (×3 orientations = 24 stickers) | always | 1 | piece id 0..7 + orientation 0..2 |
| `MidEdge` | 12 (×2 = 24 stickers) | N odd | 1 | piece id 0..11 + orientation 0..1 |
| `Wing` | 24 | N ≥ 4 | q | piece id 0..23 (identity recoverable from colors, CONVENTIONS §5) |
| `XCenter` | 24 (4 per face) | N ≥ 4 | q | color 0..5 |
| `PlusCenter` | 24 | N odd ≥ 5 | q | color 0..5 |
| `ObliqueA` (a<b) | 24 | N ≥ 6 | q(q−1)/2 | color 0..5 |
| `ObliqueB` (a>b) | 24 | N ≥ 6 | q(q−1)/2 | color 0..5 |
| `FixedCenter` | 6 | N odd | 1 | handled by code (24-state frame) |

For N = 100: 1 corner orbit, 49 wing orbits, 2,401 center orbits. For N = 400: about 40,000 orbits.

**Canonical slots:** every orbit instance maps its 24 positions to slots 0..23 in a canonical order (CONVENTIONS §4). The mapping is chosen so that **a macro's slot effect is identical for every instance of that type at every N**. A test enforces this, and it is the property that makes the network N-independent.

**Solved check per orbit:** distinguishable types compare against slot identity. Color types check that each slot's color equals the home color of that slot's face.

**Parity:** computed exactly by code (ADR-007).
- `Wing`: permutation parity of the 24 wing ids.
- `Corner` / `MidEdge`: standard.
- Center types: none needed. Identical colors mean any color arrangement can be reached with an even permutation.

---

## 6. Macros and the action library (`crates/nx-macro`)

### 6.1 Symbolic moves
`SymMove { axis, layer: LayerRef, turns }` with `LayerRef ∈ {OUTER(=0), MID, A, A_BAR, B, B_BAR}` where `A_BAR = N−1−a`. Binding an orbit instance's indices produces concrete `Move`s. The local generator set per type:

| Type | Layer refs used |
|---|---|
| `Corner` | `OUTER` (+ `MID` for odd N when the core includes middle edges) |
| `MidEdge` | `OUTER`, `MID` |
| `Wing` (depth p) | `OUTER`, `A=p`, `A_BAR` |
| `XCenter` (a,a) | `OUTER`, `A`, `A_BAR` |
| `PlusCenter` (a,mid) | `OUTER`, `A`, `A_BAR`, `MID` |
| `ObliqueA/B` (a,b) | `OUTER`, `A`, `A_BAR`, `B`, `B_BAR` |

Each ref × 3 axes × 3 turn amounts gives at most 45 generators.

### 6.2 Discovery (automated search, no human algorithms — ADR-004)
1. **Candidates:** commutators `[X, Y] = X·Y·X⁻¹·Y⁻¹` over the type's generators, with `(|X|,|Y|) ∈ {(1,1..3), (2,1..2), (3,1)}` in both orders. Search limits are config values; widen them if coverage fails.
2. **Evaluate** on a `LabeledCube` at a probe size (e.g. N = 12, indices chosen generic), composing precomputed move permutations, in parallel with rayon.
3. **Keep** candidates whose net effect is exactly one of:
   - a **3-cycle** of pieces inside the target orbit (all types)
   - a **corner twist pair** (`Corner`)
   - an **edge flip pair** (`MidEdge`)

   Everything else on the whole cube must be untouched. For `Corner`/`MidEdge` (the core, solved first) "everything else" means **other core pieces only**, because non-core orbits are unsolved at that point.
4. **Dedupe** by slot effect and keep the shortest. Cancel adjacent moves inside the sequence.
5. **Expand to actions:** conjugate every macro by every setup `S` of length ≤ 2 over the same generators → `S·M·S⁻¹`. Dedupe by slot effect, keep the lowest primitive-move cost. Expect at most about 2,024 actions per type (the number of directed 3-cycles on 24 slots).

A macro is useful only if it passes **verification (§6.3)**. Math guarantees existence: bounded-length 3-cycles exist for every center/edge cluster type (Demaine et al. 2011, arXiv:1106.5736). If discovery doesn't find enough, the search budget is the problem, not the approach.

### 6.3 Verification (the robustness guarantee)
For every action of every type:
- **Purity:** instantiate at **every** orbit instance of that type for every N from the type's minimum to 18, plus spot checks at N ∈ {31, 64, 101}. On a `LabeledCube`, the net effect must equal the declared slot effect and change nothing else (§6.2 step 3 rules).
- **Invariance:** the slot effect is identical across all instances and sizes.
- **Coverage:** for each type, the actions' 3-cycles must connect all 24 slots (a connected hypergraph of 3-cycles generates the whole alternating group). `Corner` also needs twist coverage and `MidEdge` needs flip coverage. This guarantees every legal orbit state is solvable with the library.

*Why finite checks suffice:* a macro only turns slices at its own orbit's indices plus the outer layer. How it acts on any other orbit depends only on how that orbit's indices relate to the macro's (equal, mirrored, or unrelated). N = 18 contains every such relationship class. The spot checks guard against mistakes in that argument.

### 6.4 Library file
`artifacts/macros/library.json` is committed and deterministic (same config ⇒ byte-identical file). It carries `library_version` and a `sha256` content hash. **Every checkpoint records the hash it was trained against, and loading a checkpoint with a different hash is an error (ADR-011).** Schema: CONVENTIONS §7.

---

## 7. Solving pipeline

```
input: N, facelet colors
  0. validate (legal colors, counts, reachable — parity laws)
  1. enumerate orbits for N
  2. PHASE 0 (parity, code): for each Wing orbit with odd parity → one quarter turn of slice layer p
  3. PHASE 1 (core, code + network):
       a. odd N: bring FixedCenter frame home — BFS over its 24 states using MID slices
       b. if corner permutation parity is odd → one OUTER quarter turn (even N: also fixes nothing else;
          odd N: flips MidEdge parity too, keeping the 3×3 parity law satisfied)
       c. solve Corner orbit, then MidEdge orbit (odd N), with network actions (fallback: baseline)
  4. PHASE 2 (all remaining orbits, network, parallel):
       extract every Wing/XCenter/PlusCenter/Oblique orbit as a 24-slot state → group by type →
       rounds: one batched forward pass per type per round, pick action per unsolved orbit,
       apply in orbit space, until all solved or per-orbit step cap → stragglers go to baseline
  5. EMIT: instantiate each orbit's actions to concrete moves (S, M, S⁻¹); concatenate
  6. CANCEL: merge adjacent same-(axis, layer) turns mod 4, also inside runs of same-axis moves
     (different layers on one axis commute). Report raw and cancelled lengths.
  7. VERIFY: replay moves from the input state on nx-sim; must be solved. Otherwise error. (ADR-007)
output: SolveResult (CONVENTIONS §8): moves, segments (phase/orbit/round/action → move range), stats
```

**Why the phase order is safe (tested, not assumed):**
- Every macro and action is a commutator or conjugate, so it is an **even** permutation on every orbit and never changes wing parity.
- Outer quarter turns act on wings as two 4-cycles per orbit (even). MID turns don't touch wings. So Phase 1 keeps Phase 0's parity fix intact.
- Phase 2 actions are **pure**, so they preserve the core and every other orbit, and they commute across orbits. That is why every orbit can be planned simultaneously.

**Orbit space ≡ real cube** is a mandatory test. For random states, applying an action in `nxnn.envs` and applying its instantiated moves on `nx-sim` then re-extracting must give identical slot states.

**Baseline solver (`nx-solve::baseline`, Rust; mirrored in `nxnn.baseline` for training eval):**
- Distinguishable types: decompose the permutation into cycles and reduce with the cheapest 3-cycle actions (twist/flip actions for orientation).
- Color types: first choose a target assignment of pieces to slots (match colors, prefer pieces already home), fix assignment parity by swapping two identical-color pieces, then cycle-sort.

It always succeeds when the library passes coverage. It is the fallback and the benchmark.

---

## 8. The neural network (`python/nxnn/model.py`)

**One shared network for all 7 types (ADR-006).** The input is always ≤ 24 tokens + 1, so it is tiny and fast.

**Input encoding** for an orbit of type `t` with 24 slots:
- Token `i` = `E_slot[t, i] + E_content[t, content_i]`, where content is piece id × orientation, or color. The vocabulary is ≤ 72 per type.
- Prepend a `CLS` token = `E_type[t]`.
- No positional encoding beyond `E_slot`. Slot identity *is* the position.

**Encoder:** Transformer, pre-LayerNorm, `d_model=256`, 6 layers, 8 heads, FFN 1024, GELU, no dropout. About 5M parameters. Bf16 autocast on CUDA.

**Q-head (ADR-005):** `h = MLP(CLS_out) ∈ R^256`. For each action `a` of type `t`:

```
Q(s, a) = cost(a) + softplus( h · E_action[t, a] + bias[t, a] )
```

This is the estimated total primitive moves to solve if we take `a` now. The built-in `cost(a)` term gives a correct lower-bound shape. Actions not in type `t` are masked. Value `V(s) = min_a Q(s,a)`, with `V(solved) = 0` by definition (never predicted). Policy = `argmin_a Q`.

*Why Q instead of expanding every child:* with about 2,000 actions per state, evaluating every child for a value target is ~2,000× more forward passes. A Q-head scores all actions in one pass (DeepCubeAQ, Agostinelli et al., arXiv:2102.04518).

All sizes are config values. Start small and grow only if eval shows a capacity problem.

---

## 9. Training (`python/nxnn/train.py`)

**Environment (`nxnn.envs`)** — pure PyTorch on GPU. Per type, the library's actions become tensors:
- `perm[A_t, 24]` (slot gather indices)
- `ori_delta[A_t, 24]` (orientation change mod 3/2; zeros for other types)
- `cost[A_t]`

Applying an action to a batch is one `gather` plus an add. The solved check is a comparison to a solved tensor. Millions of steps per second, with no Rust and no big cube.

**Algorithm:** self-play Q-value iteration (no teacher, no external solver data; ADR-005). Each step:
1. Sample a type per batch element (weights from config, upweighting types with worse eval).
2. Generate states: with probability `1−p_uniform`, a scramble of `k ~ U(1, K_t)` random actions from solved; otherwise a uniform random legal state of that type. Drop states that happen to be solved.
3. For each state pick `K_a` actions (e.g. 8): half the current online-greedy best, half random.
4. `s' = T(s, a)`. Target `y = cost(a)` if `s'` is solved, else `cost(a) + min_a' Q_target(s', a')`.
5. Loss = Huber(`Q_online(s,a) − y`). AdamW (lr 3e-4, wd 0.01, cosine decay, warmup 1k steps), grad clip 1.0.
6. `Q_target ← Q_online` every `target_sync` steps (e.g. 2,000), or earlier if loss < threshold (DeepCubeA rule).

**Curriculum:** start with `K_t = 2`, `p_uniform = 0`. When greedy solve rate on `K_t`-scrambles ≥ 95% within a cap of `4·K_t` steps, set `K_t += 1`. When `K_t` reaches `K_uniform` (config, ~20), set `p_uniform = 0.5`.

**Eval** (every `eval_every` steps, per type) on 4,096 uniform random states:
- greedy solve rate (step cap 64)
- beam-8 solve rate
- mean primitive cost
- ratio to baseline mean cost
- per-decision latency

Everything is logged to TensorBoard (`runs/<run_id>`).

**Checkpoints:** `artifacts/checkpoints/<run_id>/step_<n>.pt` contains weights, optimizer, config, `library_sha256`, git commit and eval summary. `artifacts/checkpoints/CURRENT` holds the path of the checkpoint the solver/server uses. Training can resume.

**Acceptance per type:**
- greedy ≥ 99.5% on uniform random states
- beam-8: 0 failures on 100k states
- mean cost ≤ 1.0× baseline (goal ≤ 0.85×)

If the network cannot beat the baseline, we report that honestly. The baseline remains the fallback either way.

**Budget expectation (not a promise):** hours per type on an RTX 4060 laptop, not days. A smoke config must train on CPU in < 2 minutes for CI.

---

## 10. Inference (`python/nxnn/solve.py`)

- Group Phase 2 orbits by type into tensors `[n_orbits_t, 24]`. Each round runs one forward pass per type (chunked if large), takes argmin-Q, and applies the action in orbit space.
- Optional beam width `W` per orbit (batch becomes `n × W`).
- Per-orbit guards: a step cap (default 64) and revisit detection (hash of slot state). If either triggers, that orbit goes to the baseline.
- Rust does the emission, cancellation and verification via `nxsim`.
- Record per step: `(orbit_id, round, action_id, q_value)` for the visualizer.

**Target timings (goals to measure, on RTX 4060 laptop):** N=10 < 200 ms, N=100 < 5 s end-to-end including verification.

---

## 11. Bindings

- **`crates/nx-py`** (PyO3 + maturin + rust-numpy) builds the Python module `nxsim`. It exposes: `Cube`, `scramble`, `random_state`, `orbits(n)`, `extract(cube)` → per-type arrays, `phase0`, `core_frame`, `baseline_solve_orbit`, `emit(actions)`, `cancel(moves)`, `verify(start, moves)`, `load_library(path)`.

  PyO3 needs `unsafe` internally, so this crate does **not** inherit the workspace `unsafe_code = "forbid"` lint. It declares its own `[lints]` table.
- **`crates/nx-wasm`** (wasm-bindgen): simulator apply/replay, orbit map per N (orbit id + type per sticker), snapshots. It is built by `cargo xtask wasm` into `web/src/engine/pkg`.

---

## 12. Local server and web UI

**Server:** `python -m nxnn.server` runs FastAPI + uvicorn on `127.0.0.1:8000`. It loads `CURRENT` checkpoint and the library, and verifies the hash. Endpoints and payloads: CONVENTIONS §8. It runs on **native Windows** (where `nxsim` is built), even if training ran in WSL2. Checkpoints live on `C:` and are visible to both.

**Web** (reuse the existing Vite + React + TypeScript + Zustand + Tailwind + three.js app; replace the legacy UI):
- **Controls:** N (2–100), scramble seed / random state, solver (`nn` | `baseline`), beam width, Solve.
- **3D cube:** one `THREE.InstancedMesh` for all stickers (60,000 at N=100) with per-instance color.
  - Per-move turn animation only for N ≤ 10.
  - Larger N applies state changes instantly per step.
- **Orbit overlay:**
  - solved orbits show true colors
  - unsolved orbits are dimmed
  - the orbit(s) being acted on in the current step are highlighted
  - a mode that colors stickers by orbit type
- **2D net view** (unfolded cube) as the default for N > 20, since the 3D view hides back faces.
- **Progress panel:** current phase (Parity → Core → Orbits), per-type bars (solved/total orbits), round counter, moves so far, NN vs baseline stats.
- **Timeline:** scrub by round; playback modes: per move (small N), per action, per round (all orbits in parallel). Adjustable speed.
- **Orbit inspector:** click a sticker to see its orbit type and indices, its 24 slots as a small diagram, and the action history with Q-values.
- **Exact replay:** the server returns moves and segments. The browser replays with `nx-wasm` (never re-implements move logic in TypeScript) and caches snapshots every K segments for fast scrubbing.

---

## 13. Repository layout (target)

```
rubiks-graph/
├─ AGENTS.md  CLAUDE.md  CHANGELOG.md  README.md  justfile
├─ Cargo.toml                    # workspace: legacy rg-* + new nx-* + xtask
├─ crates/
│  ├─ rg-cube/                   # LEGACY — kept as test oracle (dev-dependency of nx-sim)
│  ├─ rg-graph/ rg-solve/ rg-wasm/ rg-cli/   # LEGACY — frozen, not extended (ADR-010)
│  ├─ nx-sim/                    # NxN simulator + orbits + parity
│  ├─ nx-macro/                  # symbolic moves, discovery, verification, library IO
│  ├─ nx-solve/                  # phases, baseline, emit, cancel, verify
│  ├─ nx-cli/                    # `nx` binary: discover | verify-library | solve | bench | orbits
│  ├─ nx-py/                     # PyO3 → python module `nxsim`
│  └─ nx-wasm/                   # wasm-bindgen for the web
├─ python/                       # uv project, package `nxnn`
│  ├─ pyproject.toml
│  ├─ nxnn/{library,envs,model,train,evaluate,baseline,solve,server,config}.py
│  ├─ configs/{smoke,default}.yaml
│  └─ tests/
├─ artifacts/
│  ├─ macros/library.json        # committed
│  └─ checkpoints/               # git-ignored (CURRENT pointer file too)
├─ runs/                         # TensorBoard logs, git-ignored
├─ web/                          # Vite app (legacy UI replaced in M8)
├─ xtask/                        # wasm build + dev orchestration (extended for nx-wasm)
└─ docs/
   ├─ nn/                        # THIS project's docs
   └─ legacy/                    # the old graph-theory docs, superseded
```

---

## 14. Testing strategy

| Layer | Must-have tests |
|---|---|
| nx-sim | Property tests on random N ∈ [2, 64]: move∘inverse = id, 4 quarter turns = id, same-axis moves commute. Oracle: every move equals `rg-cube`'s for N = 2..7 (convert layer indexing: nx layer = rg single-layer mask bit). Allowed layers never move the DLB corner. Perf budgets (below). |
| orbits | Counts match §5 formulas for N = 2..40. Every sticker is in exactly one orbit. Every move maps each orbit to itself. Canonical slot maps are bijections. Wing identity from colors = labeled identity. |
| nx-macro | Purity, invariance, coverage (§6.3). Library is deterministic (regenerate ⇒ same hash). |
| envs | Orbit space ≡ real cube (§7) for random N ∈ [4, 30], random orbits and actions. |
| baseline | Verified solves: 1,000 random states for each N = 2..20, 100 at N = 50, 10 at N = 100, 2 at N = 400. |
| model/train | Shapes. Q ≥ cost. Smoke config trains 200 steps on CPU, no NaN, loss decreases. Checkpoint round-trip. Hash mismatch raises. |
| solve | Every returned solve is verified. Fallback path exercised by an untrained model. Results identical for the same seed. |
| server/web | API contract tests (pytest + httpx). Vitest for move decoding and segment playback. `pnpm build` passes. |

**Performance budgets (release, native):**
- Inner slice move at N = 400 ≤ 10 µs.
- Outer face move at N = 400 ≤ 1 ms.
- Replay of 1M moves at N = 100 ≤ 2 s.
- Verification of all macros ≤ 10 min on the laptop.

Tracked with criterion (`nx bench`).

**CI (GitHub Actions, Linux, CPU only):** `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `pytest -m "not gpu"`, wasm build, `pnpm check && pnpm build`.

---

## 15. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Discovery misses macros for some type | Existence is proven. Widen search limits. Coverage test fails loudly before any training. |
| Canonical slot mapping wrong ⇒ network sees inconsistent puzzles | Invariance test across all instances/sizes is a hard gate (M3). |
| Network doesn't beat baseline | Honest reporting. The baseline is the fallback. Try larger action cost weight, beam search, more capacity. |
| 40,000-orbit solves compound tiny failure rates | Per-orbit fallback and final verification mean no false solve is ever reported. |
| Library changes invalidate checkpoints | Hash in checkpoint. Load fails. Bump `library_version`. |
| WSL2 setup stalls | Time-boxed. Native Windows fallback is fully supported (SETUP.md). |
| Browser can't animate 250k+ moves | Per-round/per-action playback. Instant state application for large N. 2D net view. |
| Move count at large N | Accepted (O(N²)). Stretch: multi-orbit macros (Demaine's N²/log N idea). |

---

## 16. Stretch goals (after M8, only if the user asks)
1. Move-level core network (2×2/3×3 core solved move-by-move instead of by macros) for shorter core solutions.
2. ONNX export + `ort` to run inference from Rust without Python.
3. Macros that fix several orbits at once (shorter solutions at large N).
4. A network that proposes new macros itself.

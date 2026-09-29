# 11 · Testing & Performance

## 1. Strategy

Solver bugs are usually **silent**: a subtly wrong move table still produces a "solution"; it just doesn't solve the cube, or it produces longer solutions than it should. The defence is **independent oracles at every layer**:

```text
               ┌───────────────────────────────┐
               │  E2E (Playwright): app works  │   few
               ├───────────────────────────────┤
               │  Solver fuzz: random states,  │
               │  every solution re-verified   │
               ├───────────────────────────────┤
               │  Graph fingerprints: vertex   │
               │  counts, layer histograms,    │
               │  diameters vs published data  │
               ├───────────────────────────────┤
               │  Property tests: move algebra,│
               │  coordinate round-trips and   │
               │  congruence                   │
               ├───────────────────────────────┤
               │  Unit tests                   │   many
               └───────────────────────────────┘
```

**Rule:** every solution produced by any solver, in any test, is re-applied on the **facelet model**. The facelet model is the simplest code in the project and is independent of the cubie and coordinate models.

---

## 2. Oracles (known values the code must reproduce)

| Area | Oracle | Source of truth |
|---|---|---|
| Move algebra | m⁴ = id; m m⁻¹ = id; `R U` has order 105; `R U R' U'` has order 6 | group theory / well known |
| Geometry | the three hand-checked sticker mappings in [05 §3](05-cube-model.md#3-moves-as-axis-layer-set-turns) | hand derivation |
| Piece orbits | classification counts per N (8/12/24/24/24/6) equal union-find orbits | [05 §5](05-cube-model.md#5-pieces-and-orbits-derived-automatically) |
| Invariants | derived laws per N | [05 §8](05-cube-model.md#8-validation-is-this-state-solvable) |
| 2×2 graph | 3,674,160 vertices; HTM layer histogram; diameter 11 | [07 §0](07-solving-3x3.md#0-warm-up-the-whole-2x2-graph) |
| 3×3 BFS | layers 1, 18, 243, 3,240, 43,239, 574,908, 7,618,438, 100,803,036 | [02 §3](02-graph-theory-foundations.md#3-bfs-and-the-wall-it-hits) |
| Thistlethwaite | coset sizes 2,048 / 1,082,565 / 29,400 / 663,552; max depths 7 / 10 / 13 / 15 | [07 §3](07-solving-3x3.md#3-thistlethwaite-four-coset-graphs) |
| Kociemba | coordinate sizes; symmetry classes 64,430 (flip-slice) and 2,768 (corner perm) | [07 §4](07-solving-3x3.md#4-kociemba-two-phase) |
| Korf | superflip = 20 HTM; corner PDB max 11 | [07 §5](07-solving-3x3.md#5-korf-optimal-solver-stretch-goal-m7) |
| Big cubes | C(24,8) = 735,471 etc. as BFS vertex counts of the phase spaces; chain gates V1–V6 | [08 §2](08-solving-4x4-5x5.md#2-the-phase-contract) |
| Engine | IDA\* optimal on the 8-puzzle vs BFS; toy-graph closed forms | [10 · M2](10-roadmap.md#m2--graph-engine-core) |

---

## 3. Property tests (proptest)

| Property | Applies to |
|---|---|
| `unrank(rank(x)) == x` and `rank(unrank(i)) == i` | every ranking function |
| **Congruence:** `π(a) == π(b) ⇒ π(a·m) == π(b·m)` for random a, b with equal π | every coordinate |
| `move_table[c][m] == encode(apply(decode(c), m))` | every move table |
| facelet ↔ cubie round-trip; applying a move commutes with conversion | `rg-cube` |
| `parse(format(alg)) == alg` (modulo simplification) | notation |
| `apply(state, solve(state)) == solved` | every solver |
| `h(v) ≤ true distance` (checked on the 2×2 exactly, and on 3×3 positions within BFS reach) | every heuristic |
| `|h(v) − h(v·m)| ≤ 1` (consistency) | every PDB |
| stepping with random budget slices == stepping with one big budget | every resumable search |
| sequential table build == parallel table build (byte-identical) | BFS builder |
| sym-reduced lookup == raw lookup on sampled entries | symmetry tables |

---

## 4. Performance budgets

These are "fail CI" thresholds. Targets appear in [01 §5](01-vision-and-scope.md#5-scope-and-success-criteria).

| Item | Budget (browser unless noted) |
|---|---|
| App first load (without optional tables) | < 2 MB JS+wasm gzipped; interactive < 1.5 s |
| Light wasm instance init | < 50 ms |
| Turn animation | 60 fps on 5×5 with the neighbourhood view open |
| 2×2 table generation | < 2 s |
| Thistlethwaite tables (generated in browser) | < 2 s; solve < 10 ms |
| Kociemba small tables (download + verify) | < 1 s from local server |
| Kociemba first solution | < 100 ms (p95) |
| IDA\* throughput (3×3 phase 1, small tables) | native ≥ 20 M nodes/s/core; wasm ≥ 50% of native (to be calibrated in M6) |
| Worker slice length | 8–16 ms (UI never blocked > 32 ms) |
| Telemetry overhead with `Full` level | < 15% of throughput |
| 4×4 solve | p95 < 10 s |
| 5×5 solve | p95 < 30 s |
| Memory per worker | see [04 §7](04-system-architecture.md#7-memory-model) |

---

## 5. Benchmarks

- **criterion** micro-benchmarks: move application (facelet, cubie, coordinate), ranking functions, PDB lookup, IDA\* node expansion, BFS layer expansion.
- **`rgraph bench`** macro-benchmarks: N random states (seeded) per algorithm, reporting mean/p50/p95/max of length, time and nodes. Results go to JSON and are compared with the previous run to flag regressions over 10%.
- **Browser benchmarks:** `performance.mark/measure` around solve phases, and a hidden `/bench` route that runs the same seeded suite in wasm, so native and browser numbers can be compared.
- Results are summarized in `docs/benchmarks.md` at each milestone.

---

## 6. Determinism and native/wasm parity

- All randomness is seeded (`u64` seeds on every scramble, sample and test).
- Move orderings and tie-breaks are fixed, and no hash-map iteration order leaks into results (FxHash is deterministic; ordered iteration is used where output depends on it).
- The **parity test** runs a seeded suite natively and in wasm (via Node + the wasm build in CI) and asserts identical solutions for identical budgets.

---

## 7. CI pipeline

| Trigger | Jobs |
|---|---|
| Every push / PR | `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test` (fast suite), wasm build, `biome check`, `vitest`, `vite build`, Playwright smoke (load app → random state → solve → animation completes → screenshot diff) |
| Nightly | heavy ignored tests (3×3 BFS to depth 7, 10⁶-state fuzz for Thistlethwaite and Kociemba), full `rgraph bench`, native/wasm parity, Phase Lab reports for 4×4/5×5 |
| Release | `wasm-opt` build, table generation, static bundle artifact |

Local equivalent: `cargo xtask ci`.

---

## 8. Debugging aids

| Tool | Purpose |
|---|---|
| `rgraph explain --size 3 "<facelets>"` | Prints every coordinate, every heuristic value, validity invariants, and which phase the state is in |
| `rgraph verify --size N --file sols.jsonl` | Re-applies stored solutions on the facelet model |
| `rgraph tables list / check` | Lists tables with definition text, hash, histogram; verifies checksums |
| Telemetry recording | `?record=1` in the app downloads the telemetry stream of a solve for offline replay in the views |
| Seeded repro | Every failing fuzz case prints its seed and facelet string, so it can be replayed in both CLI and browser |

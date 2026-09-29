# 12 · Decision Log (ADRs)

Short architecture decision records. Each one gives the context, the decision, and its consequences. When implementation deviates from these docs, add a new ADR that supersedes the old one; don't rewrite history.

---

### ADR-001 · Rust engine compiled to native and WebAssembly
- **Context:** search inner loops dominate runtime; tables are hundreds of MB natively; the UI must run in a browser.
- **Decision:** all cube semantics and graph algorithms in Rust. Native builds generate tables and run benchmarks; wasm builds solve in the browser.
- **Consequences:** a single implementation with native/wasm parity tests. It needs the `wasm32` target and `wasm-bindgen-cli` (pinned). The TypeScript UI contains no move logic.

### ADR-002 · "Only graph theory" is enforced as rules R1–R6
- **Context:** the phrase is ambiguous.
- **Decision:** adopt the rules in [01 §2](01-vision-and-scope.md#2-what-only-graph-theory-means). In short: paths come from graph search, heuristics are BFS distances in quotient graphs, there is no human method knowledge, and parity is resolved by connectivity analysis.
- **Consequences:** no hard-coded algorithms anywhere. The big-cube phases must be designed so that search finds everything, which makes the Phase Lab (ADR-009) a necessity.

### ADR-003 · Moves generated from 3D geometry
- **Context:** hand-typed permutation tables are the classic bug source, and they don't scale to 4×4 and 5×5.
- **Decision:** moves are (axis, layer mask, turns), and their permutations are computed from integer sticker coordinates and rotation maps.
- **Consequences:** one code path for all N; wide, slice and rotation moves come for free; correctness rests on three hand-checked rotation facts plus property tests.

### ADR-004 · Resumable (explicit-stack) search instead of browser threads
- **Context:** long wasm calls freeze a worker; SharedArrayBuffer requires cross-origin isolation, and wasm threads need nightly Rust.
- **Decision:** every search exposes `step(budget)`; the worker time-slices.
- **Consequences:** pause, resume, single-step, clean cancellation and deterministic results. It is slightly more complex than recursive IDA\*. Parallelism in the browser comes from multiple workers, not threads.

### ADR-005 · Tables generated natively, shipped as verified static files
- **Context:** large BFS tables take far longer single-threaded in wasm than multi-threaded natively.
- **Decision:** `rgraph gen-tables` writes `.rgt` files with a definition hash and a checksum. The browser fetches and verifies them and caches them in IndexedDB. Small tables can also be generated in-browser (with a live visualization).
- **Consequences:** fast first solve and automatic invalidation when definitions change. Production builds must include a table-generation step.

### ADR-006 · Two coordinate execution styles
- **Context:** move tables are ideal for small coordinates but explode for C(24,8) × 27 moves (≈ 79 MB as u32).
- **Decision:** move tables for coordinates below about 2¹⁸; bitmask/piece state with rank-on-demand for larger ones.
- **Consequences:** the 4×4/5×5 phases stay within the browser memory budget. Two code paths share one `Coordinate` trait and one congruence test harness.

### ADR-007 · Frame handling per N
- **Context:** even cubes have no fixed centres.
- **Decision:** the 2×2 fixes the DBL corner (9 moves). The 3×3 and 5×5 use fixed centres. The 4×4 uses 27 moves with an **orientation-free** goal: phases pick colour pairs dynamically, and the 3×3 stage recolours.
- **Consequences:** no wasted "rotation" moves on the 4×4. Phase-1 heuristics are a min over 3 colour pairs, and the phase-3 goal set is multi-source.

### ADR-008 · Solvability via automatically derived invariants
- **Context:** validation rules differ per N, and hand-written checks are error-prone.
- **Decision:** derive the reachable invariant subspace (parities and orientation sums) from the generators by linear algebra over GF(2)/Z₃.
- **Consequences:** the same mechanism powers parity lifting between phases; tests pin the derived laws.

### ADR-009 · Big-cube phases are data, validated by gates and the Phase Lab
- **Context:** the exact 4×4/5×5 phase boundaries can't be known in advance with certainty; parity traps are easy to miss.
- **Decision:** `PhaseSpec` in RON. Gates V1–V6 are checked automatically, and the Phase Lab reports orbits, sizes, components, invariants and sampled performance.
- **Consequences:** design iterations are cheap and measurable, and the docs' phase tables are hypotheses until the gates pass.

### ADR-010 · Visualization stack: three.js + sigma.js/graphology + d3
- **Context:** we need a 3D cube, graphs up to about 30k nodes interactive (plus a 3.67M-point cloud), and custom charts and trees.
- **Decision:** three.js (imperative) for 3D and the point cloud, sigma.js v3 with graphology for networks, and d3 modules for charts, icicles and sunbursts.
- **Consequences:** each library does what it's best at; the React wrappers stay thin. React-three-fiber and cosmos.gl remain optional future upgrades.

### ADR-011 · Engine is the single source of truth; animation is cosmetic
- **Context:** renderer and engine state drift is a common visual bug.
- **Decision:** the renderer only draws the engine's facelet arrays. Turn animations are interpolations followed by a full re-sync.
- **Consequences:** no drift, instant scrubbing via precomputed states, and a simpler renderer.

### ADR-012 · pnpm + Vite + React + Zustand + Tailwind + Biome; `cargo xtask` for orchestration
- **Context:** tools already present on the machine (Node 24, pnpm 11); Windows-first development.
- **Decision:** as stated in the title. Build scripting lives in a Rust `xtask` crate rather than shell scripts.
- **Consequences:** the same commands work in PowerShell, Git Bash and CI, with a small number of well-known tools.

### ADR-013 · Interim: solves run to completion inside the worker (2026-09-19)
- **Context:** the Solve button shipped before the search-tree visualizer, which is what needs resumable search (ADR-004).
- **Decision:** for now `Solver::solve` runs as one call in the solver worker, bounded by a time budget (target 20 moves, 1 s). The UI thread stays responsive because the search runs off-thread. The first milestone to visualize IDA\* live (M6 search-tree view) converts `ida::search_exact` to the explicit-stack `step(budget)` form.
- **Consequences:** no pause or step yet; cancellation happens by discarding the result (the controller ignores results for a cube that has changed).

### ADR-014 · Cube state never waits on the render loop (2026-09-19)
- **Context:** hidden or occluded pages get no `requestAnimationFrame` callbacks, which stalled queued turns (observed in the embedded preview pane).
- **Decision:** each turn animation has a watchdog timer (duration + 250 ms) that commits the move if the animation hasn't finished. Hidden pages commit immediately.
- **Consequences:** the engine state always advances; visuals simply catch up when frames resume.

### ADR-015 · 4×4 shipped; 5×5 phase 3 still experimental (2026-09-21)
- **Context:** the 4×4 chain from docs/08 §5 works: 3 reduction phases plus Kociemba, about 67 moves (OBTM) on average, 2–6 s per solve natively after a 10 s one-time table build (15 s in wasm). Getting it there took three connectivity fixes, all found by measurement: (1) phase 2 must land in the component of the centre graph that phase 3 can solve; (2) the wing permutation must be even ("OLL parity"); (3) the corner-vs-dedge parity target depends on which rotated colour scheme the centres end in ("PLL parity" plus recolouring). For the 5×5, phases 1, 2a and 2b work in about 0.1 s each. Phase 2 had to be split so that the "centres solvable by phase 3" condition became a BFS distance table instead of a filter.
- **Problem:** the 5×5 phase 3 (solve both centre orbits and attach 24 wings to 12 midges, keeping earlier work) is correct but some stages take 50–110 s natively. The stages need 9–12 moves over 20–26 generators, and a max of independent tables leaves a 5–7 move gap to the true depth. Tried without success: every stage ordering (centres first or last, x then t, class-0 first), weighted IDA\*, adaptive unit selection with time slices, single-tredge units, inner-slice generators, and joint (x, t) per-axis and two-tredge/both-class tables.
- **Decision:** ship the 4×4 in the app. Keep the 5×5 chain in `rg-solve` (reachable from the CLI, slow tests `#[ignore]`d) and do not offer it in the UI until phase 3 is fast.
- **Options for the 5×5:** (a) much larger precomputed pairing tables (four tredges per table is about 860M entries, so native only); (b) macro-operators discovered by search (Korf 1985): fast and reliable, but longer solutions; (c) a different phase-3 decomposition.

### ADR-016 · 5×5 pairing by macro operators found by search (2026-09-28)
- **Context:** ADR-015 left the 5×5 phase 3 correct but unusably slow (50–110 s per pairing stage), and listed three ways out. The choice was (b), Korf's macro-operators, accepting longer solutions for speed and reliability.
- **Decision:** discover macro operators by search at table-build time — sequences of up to 8 phase-3 moves that restore both centre orbits, return every midge sticker to its place, and move at most 6 wings — and pair the tredges by walking over those operators and their conjugates by centre-fixing setup turns. The search finds 268 operators; 1,522 setups give them reach. Rules R1–R6 hold: the operators are paths found by search, never written down, and the filter is a property of the resulting permutation.
- **Consequences:** the 5×5 is offered in the app. Pairing takes about 40 ms instead of minutes, and a whole solve is about 150 OBTM in roughly 5 s of search after an 18 s one-time table build in the browser (7 s natively). Solutions are noticeably longer than the ~100 OBTM the original chain aimed at, which is the accepted trade-off. Phases 1 and 2b now use a weighted (1.5×) heuristic with any-depth IDA\*, which cut their tails from tens of seconds to a few seconds at a cost of 1–2 moves each. This supersedes the 5×5 half of ADR-015; the 4×4 half stands.
- **Found along the way:** the inner-slice half turns were not being tracked on the centre state (bit-permutations were built only for the phase-2b move set), and `expand_obtm` mangled wide block turns into a turn plus its own undo. Both are now pinned by tests.

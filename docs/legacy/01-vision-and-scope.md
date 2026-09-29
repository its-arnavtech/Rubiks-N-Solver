# 01 · Vision & Scope

## 1. The pitch

Build a program that solves the 2×2, 3×3, 4×4 and 5×5 Rubik's cubes by treating each puzzle as a graph. States are vertices, moves are edges, and a solution is a path to the solved vertex. Every solving technique must be a graph technique, and every one of them is shown on screen while it runs: the frontier of a BFS, the tree of an IDA\* search, the small quotient graph whose distances drive the heuristic, and the phase-by-phase path through a chain of coset graphs.

The finished product is a **visual workbench**. You can scramble or enter a cube, choose an algorithm, and watch the graph search find a path. The 3D cube then plays that path back, and the graph views show why each move was chosen.

---

## 2. What "only graph theory" means

"Using only graph theory" can be read several ways, so the project follows six explicit rules. Every design decision in these documents is checked against them.

| # | Rule | In practice |
|---|---|---|
| **R1** | **Every solution is a path found by a graph algorithm.** | Allowed: BFS, bidirectional BFS, IDA\*, A\*, greedy descent along an exact distance table, connected-component analysis. |
| **R2** | **Every heuristic is a shortest-path distance in a smaller graph.** | Each lower bound is a BFS distance in a *quotient graph* (a Schreier coset graph) of the cube's graph. These are "pattern databases" / "pruning tables". |
| **R3** | **No human solving knowledge.** | No beginner method, CFOP, Roux or Petrus steps. No algorithm libraries (OLL/PLL/parity algorithms). No hard-coded move sequences anywhere in solver code. The only puzzle knowledge supplied is **geometry**: what a physical turn does to the stickers. |
| **R4** | **Group theory is the construction kit, not the solver.** | Subgroups and cosets are used to *define* graphs (which positions to merge into one vertex). Purely algebraic solvers such as Schreier–Sims stabilizer-chain factorization are not used to produce solutions. They may appear in tests to cross-check group orders. |
| **R5** | **Parity is connectivity.** | "Parity" cases on the 4×4 and 5×5 are handled by computing connected components and invariants of graphs, then steering earlier searches away from the wrong component. No memorized parity algorithms. |
| **R6** | **Everything is observable.** | Every algorithm emits telemetry (nodes, layers, frontiers, heuristic values, phase boundaries) that the visualizer can render. |

> If you intended something stricter, such as "materialize the entire graph and run Dijkstra on it", the next section explains why that is physically impossible above the 2×2. We do run that literal approach on the 2×2, where it fits in memory, as the purest demonstration and as a test oracle.

---

## 3. Why we can't just build the graph

| Puzzle | Vertices (positions) | Degree (move set) | Storage at **1 bit** per vertex | Materialize? |
|---|---|---|---|---|
| 2×2 (one corner fixed) | 3,674,160 | 9 | ≈ 459 KB | **Yes**: full BFS in well under a second |
| 3×3 | 43,252,003,274,489,856,000 ≈ 4.3 × 10¹⁹ | 18 | ≈ 5.4 exabytes | No |
| 4×4 | ≈ 7.40 × 10⁴⁵ | 27 | ≈ 10⁴⁵ bytes | No |
| 5×5 | ≈ 2.83 × 10⁷⁴ | 36 | ≈ 10⁷³ bytes | No |

This leads to the project's central design: graphs are **implicit**. A vertex's neighbours are computed on demand and never stored. Search is guided by distances in **small quotient graphs** that are stored. For the big cubes, the problem is broken into a **chain of smaller graph problems** (phases).

---

## 4. Deliverables

| ID | Deliverable | Description |
|---|---|---|
| D1 | **Engine library** (`rg-cube`, `rg-graph`) | Geometry-derived NxN cube model (N = 2…5) and a puzzle-agnostic graph-search engine |
| D2 | **Solvers** (`rg-solve`) | 2×2 exact; 3×3: bidirectional BFS, Thistlethwaite, Kociemba, Korf (stretch); 4×4 and 5×5 reduction chains |
| D3 | **Visual workbench** (`web/`) | Browser app with 3D cube, 2D net and facelet editor, graph views, search visualizations, playback |
| D4 | **CLI** (`rgraph`) | Native table generation, batch solving, benchmarks, Phase Lab, graph export |
| D5 | **Documentation** | This document set, kept current as the code evolves |

---

## 5. Scope and success criteria

Move counts use **HTM** (half-turn metric: any face turn counts 1) for 2×2 and 3×3, and **OBTM** (outer block turn metric: any turn of one or more layers from one face counts 1) for 4×4 and 5×5. Times are measured in a current desktop browser on this machine class after tables are loaded, unless marked native.

| Puzzle | Algorithm | Solution quality target | Time target |
|---|---|---|---|
| 2×2 | Exact distance table (full BFS) | **Optimal** (≤ 11 HTM) | < 1 ms per solve |
| 3×3 | Bidirectional BFS | Optimal, for positions ≤ 11 moves from solved | < 5 s |
| 3×3 | Thistlethwaite | ≤ 45 HTM guaranteed (typically about 30) | < 10 ms |
| 3×3 | Kociemba two-phase | First solution ≤ 24 in < 100 ms; improves to ≤ 21 within 1 s (typical) | anytime |
| 3×3 | Korf IDA\* (stretch) | **Optimal** | native: seconds to minutes per random state; browser opt-in |
| 4×4 | Reduction chain + Kociemba | ≤ 65 OBTM (aim about 55) | < 10 s |
| 5×5 | Reduction chain + Kociemba | ≤ 130 OBTM (aim about 100–110) | < 30 s |

"Done" for each puzzle also requires that:
1. 100% of a large random-state sample is solved (10,000 in CI, 1,000,000 in the nightly native run for 3×3).
2. Every solution, when applied, provably yields the solved state (checked by the independent facelet model).
3. The algorithm's graph behaviour is visualized (see [09 · Visualization](09-visualization.md)).

---

## 6. Non-goals

- **Human methods or tutorials.** No beginner steps, OLL/PLL, or commutator catalogues (rule R3).
- **Camera or colour recognition** of a physical cube. State entry is by a click-to-paint facelet editor.
- **Cubes larger than 5×5.** The model and engine are generic in N (6×6 would add *oblique* centre orbits), but phase chains are designed only up to 5×5.
- **Supercube centre orientation**, non-cube puzzles, multiplayer, or accounts.
- **Server backend.** Everything runs locally: a static web app plus a native CLI.
- **Mobile-first UI.** It should work at phone width, but the target is a desktop browser.

---

## 7. Guiding principles

1. **Correct before clever.** Every graph has a checkable fingerprint (vertex count, layer histogram, diameter) that is compared against published values before optimization starts. See [11 · Testing](11-testing-and-performance.md).
2. **One source of truth for cube semantics.** Move logic lives only in Rust. The TypeScript UI never re-implements a turn; it asks the engine.
3. **Visualization is a first-class consumer.** Every search algorithm is written against an *observer* interface from day one. It costs nothing when disabled and streams events when enabled.
4. **Honest labelling.** The UI always says what a number is: *lower bound*, *exact distance*, *optimal solution*, or *suboptimal solution*.
5. **Data-driven phases.** Phase chains for 4×4 and 5×5 are declared as data (generators, coordinates, goals). Their tables, invariants and validity checks are derived by tooling, not by hand.
6. **Performance budgets are requirements.** Each table and search has a memory and time budget (see [11](11-testing-and-performance.md#4-performance-budgets)), and CI benchmarks catch regressions.

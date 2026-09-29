# rubiks-graph

> Solving 2×2 → 5×5 Rubik's cubes with nothing but graph theory, and visualizing every step.

Every cube position is a **vertex**. Every turn is an **edge**. Solving means **finding a path** to the solved vertex.

The catch is size. The 3×3 graph has 43,252,003,274,489,856,000 vertices, and the 5×5 graph has about 2.8 × 10⁷⁴. You can't store those graphs, let alone search them naively. So this project is about the tools graph theory offers for searching graphs too large to store: implicit graphs, quotient (Schreier) graphs, distance bounds computed by BFS on small graphs, meet-in-the-middle search, and heuristic search. Each of those ideas also gets a live visualization.

**Status:** early implementation. Working today:
- An interactive 3D viewer for 2×2–5×5, where every move is derived from 3D geometry.
- A **Solve** button for the **2×2**: BFS over the whole 3,674,160-vertex graph, so solutions are optimal.
- A **Solve** button for the **3×3**: Kociemba two-phase (IDA\* over coset graphs), typically about 20 moves in under a second in the browser.
- A **Solve** button for the **4×4**: a 3-phase reduction chain plus Kociemba. It averages about 67 moves and takes a few seconds per solve, after a one-time table build of about 15 s in the browser.
- A **Solve** button for the **5×5**: a 4-phase reduction chain plus Kociemba, where the edges are paired by chaining **macro operators discovered by search** (ADR-016). It averages about 150 moves and a few seconds per solve, after a one-time table build of about 18 s in the browser.

**Run it:**
```bash
cargo xtask dev
```
This builds the wasm engine and starts the app at http://localhost:5173. The first run needs the `wasm32-unknown-unknown` target and `wasm-bindgen-cli`; `cargo xtask doctor` checks for both.

---

## Documentation map

| # | Document | Contents |
|---|---|---|
| 01 | [Vision & scope](docs/01-vision-and-scope.md) | Goals, what "only graph theory" means as a rule set, success criteria, non-goals |
| 02 | [Graph-theory foundations](docs/02-graph-theory-foundations.md) | Cayley graphs, quotient/Schreier graphs, why BFS distances in small graphs are admissible heuristics, subgroup chains, parity as connectivity, symmetry as automorphisms, glossary |
| 03 | [Tech stack](docs/03-tech-stack.md) | Each technology choice with its rationale and the alternatives rejected; environment setup for this machine |
| 04 | [System architecture](docs/04-system-architecture.md) | Processes, crates, data flow, resumable search, telemetry, table pipeline, public APIs |
| 05 | [Cube model](docs/05-cube-model.md) | NxN model derived from 3D geometry, move representation, notation, pieces, input validation |
| 06 | [Graph engine](docs/06-graph-engine.md) | Coordinates, move tables, BFS pruning tables, bidirectional BFS, IDA\*, symmetry, phase chains, table file format |
| 07 | [Solving the 3×3](docs/07-solving-3x3.md) | 2×2 warm-up (whole graph), raw BFS, Thistlethwaite, Kociemba two-phase, Korf optimal |
| 08 | [Solving 4×4 & 5×5](docs/08-solving-4x4-5x5.md) | Reduction as phase chains, parity lifting, the Phase Lab, risks |
| 09 | [Visualization](docs/09-visualization.md) | Each view: 3D cube, projection lens, neighbourhood graph, coset explorer, search tree, and more |
| 10 | [Roadmap](docs/10-roadmap.md) | Milestones M0–M10, from an empty repo to the finished program, with acceptance criteria |
| 11 | [Testing & performance](docs/11-testing-and-performance.md) | Oracles, property tests, benchmarks, performance budgets, CI |
| 12 | [Decision log](docs/12-decision-log.md) | Architecture decision records for the key choices |

**Suggested reading order:** 01 → 02 → 04 → 07 → 08 → 09 → 10. Read 03, 05, 06, 11 and 12 as reference when you need them.

---

## At a glance

| Layer | Choice |
|---|---|
| Engine (cube model, graph algorithms, solvers) | **Rust**, compiled natively (CLI, table generation, benchmarks) and to **WebAssembly** (browser) |
| Web app | **TypeScript**, **React**, **Vite**, **Zustand**, **Tailwind CSS** |
| 3D cube | **three.js** |
| Graph views | **sigma.js** + **graphology** (WebGL network rendering) |
| Charts / trees | **d3** modules |
| Threading | Solver runs in a **Web Worker**, and search is resumable so it can pause, step and cancel |
| Precomputed tables | Generated natively by the `rgraph` CLI, served as static `.rgt` files, cached in IndexedDB |

| Puzzle | Algorithms (all pure graph search) |
|---|---|
| 2×2 | The whole graph (3,674,160 vertices) is BFS'd; optimal solutions by gradient descent |
| 3×3 | Bidirectional BFS (short scrambles), Thistlethwaite (4 coset graphs), Kociemba two-phase (IDA\*), Korf optimal (stretch goal) |
| 4×4 | 3-phase reduction chain + 3×3 stage, with parity resolved by connectivity analysis |
| 5×5 | 4-phase reduction chain (edges paired by search-discovered macro operators) + 3×3 stage |

---

## Planned repository layout

```
rubiks-graph/
├─ Cargo.toml               # Rust workspace
├─ crates/
│  ├─ rg-cube/              # NxN cube model: geometry, facelets, pieces, moves, notation, validation
│  ├─ rg-graph/             # puzzle-agnostic graph engine: BFS, bidi-BFS, IDA*, coordinates, tables, symmetry
│  ├─ rg-solve/             # solvers: 2x2 exact, 3x3 (BFS, Thistlethwaite, Kociemba, Korf), 4x4/5x5 reduction
│  ├─ rg-wasm/              # wasm-bindgen API for the browser
│  └─ rg-cli/               # `rgraph` native CLI: gen-tables, solve, bench, phase-lab, export-graph
├─ xtask/                   # cross-platform build orchestration (`cargo xtask ...`)
├─ web/                     # Vite + React + TypeScript app
│  ├─ src/{app,engine,cube3d,net,graphs,charts,ui}/
│  └─ public/tables/        # generated .rgt tables (git-ignored)
└─ docs/                    # this documentation
```

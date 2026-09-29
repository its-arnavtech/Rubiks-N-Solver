# 09 · Visualization

**Principle:** each graph idea in [02](02-graph-theory-foundations.md) gets a view that makes it visible. Views are driven by the store (state, solution, telemetry), never by solver internals directly, and each has an explicit scale limit so it can't freeze the browser.

---

## 1. Layout

```text
┌──────────────────────────────────────────────────────────────────────────────────┐
│ rubiks-graph   [2×2][3×3][4×4][5×5]   Algorithm [Kociemba ▾]  Tables ✔  ◐ theme   │
├───────────────┬──────────────────────────────────────┬───────────────────────────┤
│ STATE         │                                      │ GRAPH VIEWS               │
│ ▸ Random state│                                      │ [Neighbourhood] [Coset]   │
│ ▸ Scramble…   │           3D CUBE STAGE              │ [Search tree] [Distances] │
│ ▸ Edit (net)  │                                      │ [Pipeline] [Compare]      │
│ ▸ Notation ▢  │     drag = orbit · keys = turn       │                           │
│               │                                      │                           │
│ SOLVE         │  Lens: (•) Colours ( ) Phase view    │   sigma.js / d3 canvas    │
│ ▶ Solve  ⏸ ⏭  │                                      │                           │
│ budget 2 s    │  ┌ 2D net (mini) ┐                   │                           │
│ target ≤ 20   │  └───────────────┘                   │                           │
│               │                                      │                           │
│ LIVE STATS    │                                      │                           │
│ nodes 12.4M   │                                      │                           │
│ 9.1M nodes/s  │                                      │                           │
│ depth 17 / 18 │                                      │                           │
├───────────────┴──────────────────────────────────────┴───────────────────────────┤
│ TIMELINE  |◀ ◀ ▶ ▶|  ×1.0  R U2 F' … ‖ phase 2 ‖ D2 R2 …    h-profile ▁▃▂▂▁▁▁     │
└──────────────────────────────────────────────────────────────────────────────────┘
```

- Panels are resizable. The right-hand panel can be popped out full-screen.
- Keyboard: standard notation keys turn the cube (`U`, `Shift+U` = U', `Alt+U` = Uw, and so on), `Space` plays/pauses, `←/→` step, `S` solves.
- Themes: light and dark. A colour-blind-safe sticker palette plus optional letters on stickers. Reduced-motion mode shortens animations.

---

## 2. V1 · Cube stage (3D)

**Purpose:** the ground-truth picture of the current position, and playback of solutions.

| Aspect | Design |
|---|---|
| Geometry | N³ − (N−2)³ cubies (8 / 26 / 56 / 98) as one **InstancedMesh** of rounded boxes; 6N² stickers (24 / 54 / 96 / 150) as a second InstancedMesh with a per-instance colour attribute |
| State sync | Stickers are coloured from the engine's facelet array (**single source of truth**, [04 §3](04-system-architecture.md#3-the-single-source-of-truth)) |
| Turn animation | The cubies of the turning layers are parented to a pivot and rotated with easing. On completion the pivot is reset, all transforms go back home, and colours are re-synced from the engine's new facelet array. **No drift**, because the animation is purely cosmetic. |
| Playback | Every intermediate facelet state along the solution is precomputed, so scrubbing is instant (set the colours; no animation needed) |
| Controls | OrbitControls; keyboard turns; drag on a sticker to turn its layer (raycast gives axis, layer and direction) |
| Highlights | Glow outline on the pieces a phase is working on; pulse on pieces moved by the current move |
| Performance | 60 fps on integrated GPUs; at most 2 draw calls for the cube |

---

## 3. V2 · 2D net and facelet editor

- The standard cross-shaped net ([05 §2](05-cube-model.md#2-sticker-geometry-doubled-integer-coordinates)), kept in sync with V1.
- **Editor mode:** pick a colour and click stickers. There are live colour counters, and "Validate" runs the invariant checker ([05 §8](05-cube-model.md#8-validation-is-this-state-solvable)).
- Errors are explained in graph language, for example: *"This cube is in a different connected component from solved: the corner-twist invariant is 1 (mod 3). One corner is twisted; the candidates are highlighted."*
- Import/export: facelet string, plus a shareable URL hash (`#n=3&f=UUUU…`). This encodes the cube state only.

---

## 4. V3 · Projection lens

**Purpose:** show a **graph homomorphism physically**. The lens repaints the cube so that only what a coordinate "sees" remains visible. Two positions that look identical under the lens are the same vertex of the quotient graph.

| Lens | Colouring |
|---|---|
| Edge orientation (T1, Kociemba phase 1) | Edge stickers show **good** / **bad**; everything else grey |
| Corner orientation | Corner reference stickers show twist 0 / 1 / 2 |
| Slice membership | Edges coloured by the slice they belong to (M/E/S) |
| Tetrads (T3) | Corners coloured by tetrad |
| 4×4 phase 4-1 / 4-2 | Centres coloured by colour *pair* (axis); wings coloured by class A/B |
| 4×4 phase 4-3 / 5×5 5-5 | Each dedge/tredge coloured by paired / unpaired |
| Custom | Any registered coordinate |

Implementation: the engine computes `lens(lensId) → Uint8Array` of per-sticker palette indices, and the renderer swaps the palette. During playback the lens follows the active phase automatically.

---

## 5. V4 · Neighbourhood graph (local Cayley graph)

**Purpose:** show that the cube *is* a graph. The current position sits in the centre, surrounded by its neighbours.

- The engine builds the **radius-r ball** around the current position: r = 1 gives 1 + 18 vertices on the 3×3, and r = 2 gives 262 (canonical), including edges *between* discovered vertices. That shows the short cycles (`U D = D U`, the 4-cycle `U U U U`).
- **Layout:** radial by distance from the centre; angle from the first move.
- **Node colour:** the heuristic value h (any chosen PDB, or the max), or the **exact distance** where known (2×2, or a coset graph for its own coordinate).
- **Edges downhill in h** are drawn brighter. You can literally see the gradient IDA\* follows, and where it is flat (a weak heuristic).
- Click a node to apply its move path, with animation. Hover a node for a mini-net preview.
- The solution path is overlaid when it passes through the displayed ball.
- **Scale limits:** r ≤ 2 on 3×3/4×4, r ≤ 1 on 5×5 (1 + 36), and at most 1,500 nodes.

---

## 6. V5 · Coset-graph explorer (quotient graphs in full)

**Purpose:** show the small graphs that power the heuristics and Thistlethwaite's phases, **completely**.

| Graph | Vertices | Rendering |
|---|---|---|
| Edge orientation (T1) | 2,048 | force layout or onion; all edges |
| Corner orientation | 2,187 | same |
| Thistlethwaite T3 | 29,400 | WebGL, onion layout; edges on hover/zoom |
| 4×4 phase-4-2 centres | 12,870 | onion |
| Anything larger (T2 1.08M, T4 663k, C(24,8) 735k) | – | layer histogram + **sampled** neighbourhood around the current coset; no full render |

- **Onion layout:** concentric rings by BFS distance from the goal. Angles are assigned by a BFS tree sweep, so each child sits near its parent. This is deterministic and instant.
- The **current coset** pulses, and the **descent path** to the goal animates ring by ring as the solution plays.
- Colour by distance (sequential palette) or by component (categorical), with the component view doubling as the **parity explainer** ([08 §9](08-solving-4x4-5x5.md#9-what-gets-visualized-big-cubes)).
- Data comes from `quotientGraph(id)` as CSR typed arrays ([06 §5.3](06-graph-engine.md#53-graph-export-for-visualization)), loaded into graphology with no JSON round-trip.

---

## 7. V6 · Distance distributions

- Bar charts (linear and log scale) of BFS layer sizes for any table: 2×2 full graph, each Thistlethwaite phase, each PDB, and the raw 3×3 BFS layers.
- Overlays show the published values where they are known (the 2×2 distribution and the 3×3 first layers); a mismatch renders red, so correctness is visible.
- A "Where is my cube?" marker shows which bar the current position (or its coset) falls in.

---

## 8. V7 · Search-tree view (IDA\*)

**Purpose:** watch heuristic search think.

1. **Icicle / sunburst** of the search tree (d3-hierarchy on Canvas). Rows are depths, cells are move prefixes (up to depth K = 4), and width is proportional to nodes expanded beneath. Colour is the fraction pruned. It updates live from `TreeAggregate` events.
2. **Iteration chart:** x = IDA\* threshold, y = nodes expanded (log). The classic exponential staircase makes it obvious how much a better heuristic saves.
3. **f-distribution strip:** a histogram of f = g + h of sampled nodes against the current threshold.
4. **Step mode:** with the session paused, "step 1 node / 100 / 10k" advances the resumable search ([04 §5](04-system-architecture.md#5-resumable-search-why-and-how)). The icicle highlights the current DFS path and the cube stage can mirror the node under the cursor.
- **Scale:** the aggregate is bounded (≈ 40k cells); samples use a 4k-element reservoir.

---

## 9. V8 · Phase pipeline

```text
 ┌──────────────┐   ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
 │ T1  2,048    │ ─►│ T2 1,082,565 │ ─►│ T3  29,400   │ ─►│ T4  663,552  │
 │ gens 18      │   │ gens 14      │   │ gens 10      │   │ gens 6       │
 │ 6 moves  ✔   │   │ 9 moves  ✔   │   │ 11 moves ▶   │   │ …            │
 │ 0.02 ms      │   │ 0.05 ms      │   │              │   │              │
 └──────────────┘   └──────────────┘   └──────────────┘   └──────────────┘
```

- One card per phase: goal subgroup, generator count, coordinate sizes, table sizes, moves found, nodes expanded, time.
- Clicking a card switches the lens to that phase's projection, focuses the coset explorer on its graph (if enumerable), and filters the search tree to that phase.
- The same component serves Thistlethwaite, Kociemba (2 cards), 4×4 (4 cards) and 5×5 (6 or more cards).

---

## 10. V9 · Bidirectional BFS: meet in the middle

- Mirrored bar chart: forward layers grow from the left, backward layers from the right, with memory per side.
- When the frontiers intersect, a handshake animation plays and the meeting vertex's two half-paths join into the solution.
- A side-by-side counter compares the nodes a one-sided BFS *would* have needed, which shows the b^d vs 2·b^(d/2) difference.

---

## 11. V10 · Whole 2x2 graph galaxy

- All **3,674,160 vertices** rendered as a GPU point cloud (three.js `Points`, custom shader).
- **Algebraic 3D layout** (no physics): radius = BFS distance from solved (shells 0–11); position on the shell from a low-discrepancy mapping of (permutation rank, twist rank), so structurally similar positions sit near each other.
- Colour by distance. The current position and its optimal path are drawn as a bright polyline through the shells.
- Buffer ≈ 44 MB of positions plus 3.7 MB of colours. It is loaded on demand, with a sampled mode (10%) for weaker GPUs.

---

## 12. V11 · Table generation, live

- Whenever a table is generated in the browser (or regenerated after a hash mismatch), its BFS appears live: a layer histogram growing bar by bar, the fill percentage, and the forward/backward mode switch marked on the timeline.
- Uses `TableProgress` events. For native-generated tables, the stored histogram is replayed as an animation.

---

## 13. V12 · Algorithm comparison dashboard

- Run the same scramble through several solvers in parallel workers.
- The table shows length (per metric), optimality label, nodes, time, and tables used. The charts show **length vs time** (anytime curves, one line per algorithm) and nodes per algorithm (log scale).
- A batch mode (N random states) produces distributions: a histogram of solution lengths per algorithm.

---

## 14. V13 · Heuristic profile along the solution

- A line chart under the timeline. For every step of the solution it shows each heuristic's value (phase coordinates, PDBs) and the exact remaining distance where known.
- Phase boundaries are shaded bands. Each phase's heuristic falls to zero at its boundary, which is a direct picture of "descending through the subgroup chain".
- The playhead is synced with the cube stage.

---

## 15. Frontend module structure

```text
web/src/
├─ app/        App shell, layout, routes, Zustand store (cube, solve, telemetry, ui slices)
├─ engine/     wasm loading (light + worker), Comlink API, table loader + IndexedDB cache
├─ cube3d/     CubeStage (three.js), animation queue, lens palettes, sticker picking
├─ net/        2D net renderer + facelet editor + validation messages
├─ graphs/     NeighborhoodView, CosetExplorer (sigma.js), GalaxyView (three.js Points), layouts
├─ charts/     Histogram, IterationChart, AnytimeChart, HeuristicProfile, SearchIcicle (d3)
├─ pipeline/   PhasePipeline cards
└─ ui/         panels, controls, keyboard map, theme tokens
```

**Performance rules**
- Telemetry updates coalesce into one store write per animation frame.
- Heavy views are imperative classes (three.js, sigma.js) behind thin React wrappers that pass props via refs and never re-render the canvas component.
- Every view declares a node/element cap and degrades to sampling beyond it.

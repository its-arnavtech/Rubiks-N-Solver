# 07 · Solving the 3×3 (and the 2×2 warm-up)

The 3×3 work is a **ladder**. Each rung fixes the failure of the one below it, and the visualizer lets you compare them on the same scramble:

| Rung | Algorithm | Graph idea | Weakness that motivates the next rung |
|---|---|---|---|
| 0 | 2×2 exact (bonus) | Store the **entire** graph's distances | Only works when the graph fits in memory |
| 1 | BFS / bidirectional BFS | Raw search on the full Cayley graph | Exponential: stalls around 11 moves |
| 2 | Thistlethwaite | Chain of 4 **coset graphs**, each fully BFS'd | Long solutions (≤ 45, typically about 30) |
| 3 | Kociemba two-phase | **IDA\*** with quotient-graph heuristics over 2 big coset graphs | Near-optimal, but not provably optimal |
| 4 | Korf (stretch) | IDA\* on the full graph with large PDBs | Optimal but heavy (tables ≈ 86 MB, seconds to minutes) |

---

## 0. Warm-up: the whole 2x2 graph

The 2×2 is the one Rubik's-type graph we can **materialize completely**, which makes it the purest demonstration of R1 and a perfect test oracle.

- **Vertices:** fix the DBL corner. The other 7 corners have 7! = 5,040 permutations × 3⁶ = 729 orientations (the 7th is implied) = **3,674,160**.
- **Edges:** moves U, R, F × {1, 2, 3} = 9 per vertex.
- **Index:** `perm_rank(7 corners) · 729 + twist_rank(6 corners)`, one byte per vertex = 3.7 MB (nibble: 1.8 MB).
- **BFS from solved** gives the exact distance of every position. The layer histogram must equal the known HTM distribution:

| d | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| count | 1 | 9 | 54 | 321 | 1,847 | 9,992 | 50,136 | 227,536 | 870,072 | 1,887,748 | 623,800 | 2,644 |

The total is 3,674,160, and the diameter (God's number for the 2×2 in HTM) is **11**.

- **Solving = greedy descent:** from the current vertex, take any edge to a vertex with distance one less. This is always optimal.
- **Visuals:** the full-graph galaxy (3.67M points, [09 §11](09-visualization.md#11-v10--whole-2x2-graph-galaxy)), the distance histogram, and the neighbourhood explorer showing exact distances on every node.
- The 2×2 is also the **corner projection of the 3×3** (up to the frame), which introduces the idea of a quotient before the 3×3 uses it.

---

## 1. Representation on the 3×3

- Full state: `Cube3 { cp, co, ep, eo }` (cubie arrays), packed into a `u128` for hashing.
- 18 moves (HTM), canonical filter: no same face twice; opposite faces in a fixed order ([06 §6.3](06-graph-engine.md#63-canonical-move-filter)).
- All coordinates below are ranks of piece projections, generated and tested as in [06 §4](06-graph-engine.md#4-coordinates-and-move-tables).

---

## 2. Raw graph search on the full Cayley graph

### 2.1 BFS (demonstration)

BFS from solved, layer by layer, until a memory cap. It reproduces the table in [02 §3](02-graph-theory-foundations.md#3-bfs-and-the-wall-it-hits) (layers 0–5 in the browser, up to 7 natively), and the UI shows the frontier exploding. It solves positions whose depth is within the explored ball, and it is a **test oracle** for move correctness: any bug in the move tables changes these counts.

### 2.2 Bidirectional BFS (a real solver for short positions)

| Depth stored per side | States stored per side (cumulative) | Approx. memory per side | Solves positions up to (with a streamed extra layer) |
|---|---|---|---|
| 4 | 46,741 | ≈ 2 MB | 9 |
| 5 | 621,649 | ≈ 25 MB | **11** (browser default) |
| 6 | 8,240,087 | ≈ 330 MB | 13 (native default) |

- Stored depth 5 per side, plus streaming the next layer against the other side's map, reaches distance 11 in the browser.
- The result is **optimal**, because the first meeting at the smallest combined depth is a shortest path.
- **Visual:** two growing frontiers (bar pairs per layer) and a "handshake" animation when they meet ([09 §10](09-visualization.md#10-v9--bidirectional-bfs-meet-in-the-middle)).

---

## 3. Thistlethwaite: four coset graphs

The chain ([02 §6](02-graph-theory-foundations.md#6-subgroup-chains-solving-as-a-sequence-of-small-graph-problems)):

```text
G₀ = ⟨L, R, F, B, U, D⟩
G₁ = ⟨L, R, F, B, U2, D2⟩
G₂ = ⟨L, R, F2, B2, U2, D2⟩
G₃ = ⟨L2, R2, F2, B2, U2, D2⟩
G₄ = {solved}
```

| Phase | Moves available | Coset graph = what the coordinate captures | Vertices (index) | Max depth (HTM) |
|---|---|---|---|---|
| **T1** G₀→G₁ | all 18 | **Edge orientation.** 12 edges, each good/bad relative to the "no U/D quarter turns" subgroup; the last is implied | 2¹¹ = **2,048** | 7 |
| **T2** G₁→G₂ | L, R, F, B (×3) + U2, D2 = 14 | **Corner orientation** relative to the L/R axis (3⁷ = 2,187) × **positions of the 4 M-slice edges** UF, UB, DF, DB (C(12,4) = 495) | **1,082,565** | 10 |
| **T3** G₂→G₃ | L, R (×3) + F2, B2, U2, D2 = 10 | **Which 4 of the remaining 8 edge slots hold E-slice edges** (C(8,4) = 70) × **corner permutation modulo G₃'s corner group** (8!/96 = 420 = 70 tetrad splits × 6) | 70 × 420 = **29,400** | 13 |
| **T4** G₃→{e} | L2, R2, F2, B2, U2, D2 = 6 | Full state inside the squares group G₃ | **663,552** | 15 |

The guaranteed maximum is 7 + 10 + 13 + 15 = **45 HTM**.

**The T3 coordinate, done generically.** The corner part is "the corner permutation up to right-multiplication by the 96 corner permutations that G₃ can produce". The engine computes those 96 permutations by BFS over G₃'s corner action. It then takes, as the canonical label, the minimum-rank permutation in the coset of 96, and indexes the 420 labels. Two checks guard this construction:
1. **Congruence test:** equal labels must map to equal labels under every T3 move.
2. **Count test:** BFS from the goal over the resulting graph must find exactly 29,400 vertices with maximum depth 13.

If the left/right composition convention were wrong, check 1 would fail immediately.

**T4 indexing:** the corner-permutation index among the 96 (lookup) × the M, E and S slice edge permutations (24³). That gives 1,327,104 slots, of which exactly 663,552 are reachable; BFS marks the rest as unreachable. That costs 1.3 MB at one byte per slot.

**Solving:** every table is **exact**, so each phase is **greedy descent**: pick any allowed move that lowers the phase distance. It never searches, and runs in microseconds. Optional improvement: among equally good moves, prefer one that also lowers the *next* phase's distance, or one that cancels with the previous move.

**Tables:** 2,048 + 1,082,565 + 29,400 + 1,327,104 bytes ≈ 2.4 MB. They generate in the browser in about a second, so they need no download.

**Visuals:** the phase pipeline shows G₀ ⊃ G₁ ⊃ G₂ ⊃ G₃ ⊃ {e} with a live "you are here". The coset graphs for T1 (2,048 vertices) and T3 (29,400 vertices) are rendered **in full** with the current coset highlighted and the descent path animated. The projection lens repaints the cube to show only what the current phase sees ([09 §4](09-visualization.md#4-v3--projection-lens)).

---

## 4. Kociemba two-phase

```text
G ⊃ G₁ = ⟨U, D, R2, L2, F2, B2⟩ ⊃ {solved}
```

### 4.1 Coordinates

| Phase | Coordinate | Size | Moves |
|---|---|---|---|
| 1 | twist (corner orientation) | 3⁷ = 2,187 | 18 |
| 1 | flip (edge orientation) | 2¹¹ = 2,048 | 18 |
| 1 | slice (positions of the 4 E-slice edges, unordered) | C(12,4) = 495 | 18 |
| 2 | corner permutation | 8! = 40,320 | 10 |
| 2 | U/D-layer edge permutation (8 edges) | 8! = 40,320 | 10 |
| 2 | E-slice edge permutation | 4! = 24 | 10 |

The phase-2 moves are U, U2, U', D, D2, D', R2, L2, F2, B2.

The phase-1 coset graph has 2,187 × 2,048 × 495 = **2,217,093,120** vertices (max depth 12). The phase-2 graph has 40,320 × 40,320 × 24 / 2 = **19,508,428,800** vertices (max depth 18). Neither is stored; both are searched with IDA\*.

### 4.2 Heuristic tables, two tiers

| Tier | Phase 1 PDBs | Phase 2 PDBs | Total size | When |
|---|---|---|---|---|
| **Small** (default first) | twist × slice (1,082,565); flip × slice (1,013,760); h = max | corner perm × slice perm (967,680); U/D-edge perm × slice perm (967,680); h = max | ≈ 2 MB nibbles + ≈ 3 MB move tables | M6 baseline |
| **Large** (symmetry-reduced) | flipslice classes (64,430 under 16 syms) × twist (2,187) = 140,908,410 at 2 bits ≈ 35 MB | corner-perm classes (2,768) × U/D-edge perm (40,320) = 111,605,760 ≈ 28 MB | ≈ 65 MB | optional download; about 10× fewer nodes |

### 4.3 Search (anytime)

```text
for each phase-1 solution p1 in increasing length (IDA* enumeration):
    skip if last move of p1 ∈ phase-2 moves              // boundary rule
    s1 = start · p1                                      // now in G₁
    bound = best_len − |p1| − 1
    p2 = IDA*_phase2(s1, max_depth = bound)              // may fail → continue
    if found: best = p1 + p2; emit SolutionImproved
    stop when best_len ≤ target_length or time budget exhausted
```

Optional: run **three axis-conjugated searches** (U/D, R/L and F/B as the "phase-1 axis") and **the inverse position**, interleaved in time slices, and keep the global best. This is automorphism-based and pure graph symmetry.

**Expectations (to be confirmed by benchmark):** small tables give a first solution in ≤ 24 moves within tens of milliseconds and ≤ 21 within about 1 s in the browser. Large tables reach ≤ 20 on most positions within about 1 s.

**Visuals:** the IDA\* search-tree icicle (live), the iteration growth chart, the anytime "length vs time" chart, and the heuristic profile along the final path, where the phase-1 heuristics fall to 0 and hand over to phase-2 heuristics.

---

## 5. Korf optimal solver (stretch goal, M7)

- IDA\* on the full 3×3 graph with h = max of three PDBs:

| PDB | Coordinate | Entries | Size (4-bit) |
|---|---|---|---|
| Corners | all 8 corners (perm × twist) | 8! × 3⁷ = 88,179,840 | 44 MB |
| Edges A | 6 edges (positions × flips) | 12!/6! × 2⁶ = 42,577,920 | 21 MB |
| Edges B | the other 6 edges | 42,577,920 | 21 MB |

- The corner PDB's maximum is 11: the corners behave like a 2×2 (in HTM).
- Enhancements: symmetry and inverse lookups, parallel root split natively, optional 7-edge PDBs (510,935,040 entries each) natively.
- **Honest expectations:** random positions are mostly 17–18 moves from solved, and IDA\* with these PDBs takes seconds to minutes per position natively. It is **native-first**; the browser gets an opt-in with a depth cap and a clear warning.
- Oracles: "superflip" is exactly 20 (HTM), and optimal lengths of random-move scrambles of length ≤ 12 can be cross-checked against bidirectional BFS.

---

## 6. Comparison summary (targets, to be measured)

| | Bidi BFS | Thistlethwaite | Kociemba (small) | Kociemba (large) | Korf |
|---|---|---|---|---|---|
| Optimal? | yes (short only) | no | no | no | **yes** |
| Typical length (HTM) | = distance | ≈ 30 | ≈ 21–22 | ≈ 19–20 | ≈ 17–18 |
| Worst case | depth-capped | 45 | bounded by time budget | bounded by time budget | 20 |
| Browser time | ≤ 5 s | < 10 ms | ≤ 1 s | ≤ 1 s | opt-in |
| Tables | none | ≈ 2.4 MB | ≈ 5 MB | ≈ 65 MB | ≈ 86 MB |
| Signature visual | meeting frontiers | coset graphs in full | search-tree icicle | same, far fewer nodes | same, deep iterations |

These become rows in the in-app comparison dashboard ([09 §13](09-visualization.md#13-v12--algorithm-comparison-dashboard)), with real measured values.

---

## 7. Validation checklist (3×3)

- [ ] BFS layer counts 0–5 (CI) and 0–7 (native, ignored-by-default test) match [02 §3](02-graph-theory-foundations.md#3-bfs-and-the-wall-it-hits).
- [ ] Thistlethwaite coset-graph sizes 2,048 / 1,082,565 / 29,400 / 663,552 and max depths 7 / 10 / 13 / 15.
- [ ] Kociemba coordinate sizes; symmetry class counts 64,430 and 2,768 (large tier).
- [ ] Every solution replayed on the **facelet** model reaches solved (independent of the cubie model).
- [ ] 10,000 random states per algorithm in CI; 1,000,000 nightly (native) for Thistlethwaite and Kociemba.
- [ ] Native and wasm produce identical solutions for identical inputs and budgets.

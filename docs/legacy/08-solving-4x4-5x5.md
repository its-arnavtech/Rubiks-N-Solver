# 08 · Solving the 4×4 and 5×5

## 1. Scale, and why the strategy changes

| | 3×3 | 4×4 | 5×5 |
|---|---|---|---|
| Positions | 4.3 × 10¹⁹ | 7,401,196,841,564,901,869,874,093,974,498,574,336,000,000,000 ≈ **7.4 × 10⁴⁵** | ≈ **2.83 × 10⁷⁴** |
| Move set (ours) | 18 | 27 (OBTM) | 36 (OBTM) |
| Diameter | 20 (known) | unknown | unknown |
| Piece orbits | corners, edges | corners (8), wings (24), centres (24) | corners (8), midges (12), wings (24), x-centres (24), t-centres (24) |

Optimal solving is out of reach, so we use **reduction as a subgroup/phase chain**: a sequence of graph problems, each small enough to BFS or to search with IDA\* and quotient-graph heuristics. The chain's end state is a cube that behaves exactly like a 3×3, which the 3×3 solver finishes.

The **structure** of this document is firm. The **exact phase boundaries are hypotheses**, grounded in how published multi-phase solvers (TPR 4×4×4, Walton's NxNxN solver, Norskog's multi-stage solvers) divide the work. They are **validated and tuned with the Phase Lab (§4)** before implementation locks them in. Every size quoted below is exact combinatorics; the depths are estimates to be measured.

---

## 2. The phase contract

A phase *i* is a `PhaseSpec` ([06 §11](06-graph-engine.md#11-phase-chain-driver)): a generator subset **Sᵢ**, coordinates **πᵢ**, a goal set **Tᵢ**, heuristics, and goal filters. A chain is **valid** only if it passes six automated gates:

| Gate | Check | How |
|---|---|---|
| **V1 Preservation** | Every generator in S_{i+1} preserves all goals of phases ≤ i, or the later phase explicitly re-includes those goals in its own goal set ("goal inclusion") | apply every generator to goal representatives |
| **V2 Congruence** | Each coordinate is compatible with Sᵢ | property test ([06 §4](06-graph-engine.md#4-coordinates-and-move-tables)) |
| **V3 Coverage** | Every coordinate value that phase *i* can be handed lies in the same connected component (of phase *i*'s Schreier graph) as some goal value | BFS from Tᵢ over Sᵢ; sample 10⁵ chain runs and check all start values are covered |
| **V4 Lifting** | The invariants and components that later phases can't change are fixed by earlier phases (§3) | GF(2)/Z_k nullspace + component labels |
| **V5 Budget** | Tables within the size budget (§7); phase p95 search time within budget | Phase Lab benchmark |
| **V6 End-to-end** | 100% of 10⁴ random states solve, and every solution verifies on the facelet model | CI |

---

## 3. Parity lifting: connectivity requirements flow backwards

"Parity" is phase *i+1*'s restricted graph falling apart into components ([02 §8](02-graph-theory-foundations.md#8-parity-is-connectivity)). If phase *i* hands over a state in the wrong component, phase *i+1* can never finish. The fix is made at design time, backwards along the chain:

```text
for i = last down to 1:
    I  = invariants of ⟨S_i⟩        // Z₂ parities / Z_k sums that every generator in S_i preserves
    C  = component labels of phase i's coordinate graph (when enumerable)
    phase (i−1) goal  +=  "I(state) = I(goal)"   and   "C(π_i(state)) = C(goal)"
```

Two mechanisms carry the requirement into the earlier phase:
1. **Promoted coordinate.** A parity that is a homomorphism on the *earlier* phase's group (for example the corner-permutation sign) is a genuine 2-vertex quotient graph. It is added to that phase's coordinates, so its heuristic accounts for it.
2. **Goal filter.** A condition that is only defined at the goal (for example "wing permutation parity within the A-class", which is meaningful only once all wings are in their classes) is checked when IDA\* reaches h = 0. A 1-bit filter rejects about half of the goal hits, so it costs roughly 2× in that phase and never makes it incorrect.

The classic 4×4 parities come out of this analysis mechanically. The final 3×3 stage's graph (outer turns only) has 4 components over reduced states, so the reduction phases are required to land in the component of *solved*. No parity algorithm is ever stored.

---

## 4. The Phase Lab

`rgraph phase-lab --size 4 --spec phases/4x4.ron --phase 2` produces a report (JSON plus human-readable text):

- piece-slot **orbits under Sᵢ** (union-find), which reveals for example the wing "high/low" classes;
- **coordinate sizes** and **Schreier graph statistics**: vertex count, BFS layer histogram, diameter, number of components;
- **invariants** of ⟨Sᵢ⟩ (the nullspace) and the lifting requirements they impose on phase *i−1*;
- **table sizes** in each encoding, and whether they fit the browser and native budgets;
- **sampled chain runs**: phase lengths and node counts (mean/p95/max), coverage failures (gate V3) with example states;
- an optional **Graphviz/CSR dump** of small phase graphs for the visualizer.

Phase specs are **data** (a RON file, loaded at build time by `rg-solve`), so trying a different phase boundary is a spec edit and a rerun, not a code change. The Phase Lab also drives the in-app "Pipeline" view ([09 §9](09-visualization.md#9-v8--phase-pipeline)).

---

## 5. 4×4 phase chain

### 5.1 Frame: orientation-free goals

The 4×4 has no fixed centres. Our move set is {U, D, L, R, F, B, Uw, Rw, Fw} × 3 = **27**, and **"solved" means solved in any of the 24 orientations**. The phases choose the orientation as they go:
- Phase 1 accepts **any** of the 3 opposite colour pairs landing on the R/L axis. The heuristic is the min over pairs of the same table, since the table depends only on which 8 slots are occupied.
- Phase 2 accepts either of the remaining 2 pairs on U/D.
- Phase 3 accepts the arrangements whose colours form a **proper rotation** of the colour scheme (4 of the 8 sign choices; mirror images are excluded). This is a multi-goal BFS.
- The 3×3 stage receives the reduced cube **recoloured** so that its centres read as standard, and solves it with face turns. Face turns are colour-independent, so no move translation is needed.

### 5.2 The chain

| Phase | Goal | Generators Sᵢ | Coordinates (size) | Heuristic | Est. depth |
|---|---|---|---|---|---|
| **4-1** Axis 1 | the centres of some opposite colour pair occupy the R and L faces | all 27 | which 8 of 24 centre slots hold that pair: C(24,8) = **735,471** | exact table (u8), min over 3 pairs | ~7–9 |
| **4-2** Axis 2 + wing orientation | a second pair's centres occupy U/D (so the third pair is on F/B); every wing in its home class under S₃; lifted parities | G_A: U, D, F, B, R, L (18) + Rw (3) + Uw2, Fw2 = **23** | centres: C(16,8) = **12,870**; wing classes: which 12 of 24 wing slots hold class-A wings, C(24,12) = **2,704,156** | max of the two exact tables; optional joint symmetry-reduced table natively | ~10–12 |
| **4-3** Reduce | centres solved (proper orientation); all 12 dedges paired; 3×3 parity invariants correct | G_B: U, D, R, L (12) + F2, B2 + Uw2, Rw2, Fw2 = **17** | centres: 3 axes × C(8,4) = 70³ = **343,000** (reachable subset computed); wing pairing (see below) | max of centre table and dedge-pairing PDBs | ~13–17 |
| **4-4** 3×3 stage | solved | U, D, L, R, F, B (18) | Kociemba coordinates ([07 §4](07-solving-3x3.md#4-kociemba-two-phase)) | Kociemba tables | ~18–20 |

Every generator of G_A preserves "pair on R/L": R/L face turns rotate those centres within their face, Rw's inner slice moves only U/F/D/B centres, and Uw2/Fw2 swap R↔L. Every generator of G_B preserves both axis goals and, by construction, the wing classes (they are defined as G_B's orbits). These are gate V1 checks.

**Wing classes (phase 4-2).** The 24 wing slots split into orbits under G_B. The expectation, and the classical "high/low edge" picture, is **2 orbits of 12**: quarter turns of inner slices, and the F/B outer quarter turns excluded from G_B, are what move a wing between classes. The Phase Lab computes the orbits instead of trusting this, and the coordinate is "which 12 slots contain class-A wings" (the goal is the class-A slot set).

**Dedge pairing (phase 4-3).** Because wings never change class in G_B, a dedge is paired when its class-A wing and class-B wing sit in the same edge slot. The heuristics are **multi-goal PDBs over 3 dedges at a time**: positions of their 3 A-wings (12·11·10 = 1,320) × their 3 B-wings (1,320) = **1,742,400** entries, with goals "all 3 paired, in any slots". Four disjoint PDBs cover all 12 dedges; h = max(centres, PDB₁…PDB₄). Natively, 4-dedge PDBs (11,880² = 141,134,400 entries, about 70 MB each) are an option if the 3-dedge ones prove too weak.

**Parity, concretely.** Lifting analysis (§3) is expected to produce:
- *Edge parity* (the 4×4 "OLL parity"): wing-permutation parity is invariant under G_B, because it has no inner quarter turns. So phase 4-2 (which has Rw quarter turns) must deliver the right value. This is a filter or promoted coordinate on 4-2.
- *PLL parity*: the 3×3 stage needs dedge-permutation parity = corner-permutation parity. Corner parity is a genuine coordinate (sign homomorphism) that G_B's quarter turns change, so it becomes part of phase 4-3's goal. The Phase Lab confirms whether phase 4-3 can always fix it, or whether it must be lifted further back.

**Expected total:** about 8 + 11 + 15 + 19 ≈ **50–55 OBTM**, against a target of ≤ 65.

### 5.3 Fallbacks if the Phase Lab rejects a boundary

| Problem | Fallback |
|---|---|
| 4-2 too slow (weak heuristic over 3.5 × 10¹⁰ vertices) | Split it into 4-2a (centres, exact) and 4-2b (wing classes, with generators that preserve the centre axes but still include moves that change wing class). Or build a joint symmetry-reduced table natively. |
| 4-3 too slow | Insert a Thistlethwaite-style intermediate phase that first moves wings into their target slices, shrinking the pairing graph. Or use larger (4-dedge) PDBs natively. |
| Coverage failure (V3) | Promote the offending invariant to the previous phase (§3). |

---

## 6. 5×5 phase chain

### 6.1 Frame

Fixed centres define the frame. The move set is the 6 faces + 6 two-layer wide turns, × 3 = **36**. None of these moves ever moves a fixed centre. The 5×5 has **two** centre orbits (x-centres and t-centres) and **three** edge orbits (the midges are real 3×3 edges and carry orientation; the two wing classes are like 4×4 wings).

### 6.2 The chain

| Phase | Goal | Generators | Coordinates (size) | Heuristic | Est. depth |
|---|---|---|---|---|---|
| **5-1** R/L centres to axis | all 8 R/L x-centres and 8 R/L t-centres on the R/L faces | all 36 | x: C(24,8) = **735,471**; t: C(24,8) = **735,471** | max of two exact tables; optional joint table natively | ~10–12 |
| **5-2** U/D centres to axis | U/D x- and t-centres on U/D (so F/B are on F/B) | G_A5: 18 face turns + Rw, Lw (6) + Uw2, Dw2, Fw2, Bw2 = **28** | x: C(16,8) = 12,870; t: 12,870; joint = **165,636,900** | native: exact joint table (≈ 83 MB nibble); browser: max of two 12,870 tables | ~9–11 |
| **5-3** Edge orientation | every wing in its G_B5 class; all midges oriented; lifted parities | G_C5: 18 face turns + all 6 wide half turns = **24** | wing classes C(24,12) = **2,704,156**; midge EO 2¹¹ = **2,048** | max of two exact tables | ~8–10 |
| **5-4** Solve centres | x- and t-centres fully solved | G_B5: U, D, R, L (12) + F2, B2 + 6 wide half turns = **20** | x: ≤ 70³ = 343,000; t: ≤ 343,000 (reachable subsets computed) | max of two exact tables | ~12–15 |
| **5-5** Pair tredges (staged a/b/c: 4 tredges at a time) | centres stay solved (goal inclusion) + next 4 tredges paired + all previous tredges still paired | G_B5 (20) | per stage: pairing PDBs over tredge subsets + centre tables | max | ~3 × (10–14) |
| **5-6** 3×3 stage | solved | 18 face turns | Kociemba (EO already solved, so its phase 1 is short) | Kociemba tables | ~16–19 |

**Why centres come before edges:** once the centres are separated onto their axes, the axis-preserving generator sets shrink step by step (all 36 → 28 → 24 → 20), and each shrink makes the next phase's quotient graph smaller.

**Why stage 5-5 uses goal inclusion:** no single generator in G_B5 pairs edges without disturbing centres (every inner-slice turn moves centres). The phase goal therefore includes "centres solved and earlier tredges paired", and IDA\* finds sequences that temporarily break and then restore them. The heuristic max(centre tables, pairing PDBs) keeps that search focused.

**Expected total:** about 11 + 10 + 9 + 13 + 36 + 18 ≈ **95–110 OBTM**, against a target of ≤ 130. *(What shipped is longer: see 6.3.)*

### 6.3 What shipped: macro operators for the pairing phase

Phases 5-1 to 5-4 shipped essentially as designed, with phase 5-2 split into 2a (U/D centres plus the wing-parity bit) and 2b (wing classes, midge orientation, and centres moved into an arrangement phase 3 can solve). Stage **5-5 did not work as designed**: pairing while holding the centres needs 9–12 moves over 20–26 generators, and the max of independent tables leaves a 5–7 move gap, so single stages took 50–110 s (ADR-015).

What shipped instead is Korf's **macro-operator** method (ADR-016), which stays inside rules R1–R6 because the macros are themselves found by search:

1. **Discovery** (`rg-solve/src/macros5.rs`, once at table-build time). Enumerate every canonical sequence of up to 4 phase-3 moves and group them by the centre state they produce. Two sequences that leave the centres in the *same* state compose, one forward and the other reversed, into a sequence of up to 8 moves that **restores the centres**. Of those, keep the ones that also return every midge sticker to where it was and move at most 6 wings. The search finds **268** distinct such operators, the shortest 7 moves moving 4 wings.
2. **Conjugation.** A macro conjugated by a sequence of centre-fixing turns (`setup⁻¹ · macro · setup`) is another macro with the same properties, its effect moved onto other wings. 1,522 setups of up to 3 turns give the few hundred discovered macros enough reach to touch every part of the puzzle, at no memory cost: the composition is done on the fly during the walk.
3. **The walk.** Because every operator restores the centres and leaves the midges alone, nothing it does can undo the centre stage or detach a wing outside its own support. Pairing is therefore a plain walk: at each step take the operator that attaches the most wings, shortest first, and when none does, take the shortest operator reaching an arrangement not seen before.

**Cost:** the walk finishes in about 40 ms and 80–160 moves, against 50–110 s per stage before. The price is length, which is the documented trade-off: a 5×5 solve is about **150 OBTM** rather than the ~100 the original chain aimed at.

**Two bugs this work uncovered**, both now pinned by tests: the inner-slice half turns were not tracked on the centres at all (their bit-permutations were built only for the phase-2b move set, so phase 3 could believe centres were solved when they were not), and `expand_obtm` rewrote *wide block turns* as well as inner slices, corrupting the cube between phases.

### 6.4 5×5 risks and fallbacks

| Risk | Signal (Phase Lab) | Mitigation |
|---|---|---|
| **5-5 is the hard part**: pairing search with weak heuristics | p95 phase time > budget | (1) Thistlethwaite-style **slice separation** first: bring each tredge's wings into the slice its midge will occupy, so pairing works on a much smaller graph. (2) Bigger PDBs natively. (3) Pair 2 tredges per stage instead of 4. (4) Weighted IDA\* / beam search as a clearly labelled suboptimal mode (still pure graph search). |
| 5-3 can't reach every wing-class configuration with G_C5 | V3 coverage failure | Merge 5-3 into 5-2, where Rw/Lw quarter turns are available, or promote the invariant into 5-2. |
| 5-1 slow in the browser | p95 > 5 s | Ship the joint (symmetry-reduced) 5-1 table as an optional download. |

---

## 7. Table budget

Sizes at the listed encoding; move tables are excluded where coordinates use rank-on-demand ([06 §4](06-graph-engine.md#4-coordinates-and-move-tables)).

| Puzzle | Table | Entries | Browser | Native |
|---|---|---|---|---|
| 4×4 | 4-1 centres axis | 735,471 | 0.7 MB (u8) | same |
| 4×4 | 4-2 centres | 12,870 | 13 KB | same |
| 4×4 | 4-2 wing classes | 2,704,156 | 1.4 MB (nibble) | same |
| 4×4 | 4-3 centres | ≤ 343,000 | 0.3 MB | same |
| 4×4 | 4-3 dedge PDBs | 4 × 1,742,400 | 3.5 MB | or 3 × 141M ≈ 210 MB (4-dedge) |
| 4×4 | 3×3 stage | – | ≈ 5 MB (Kociemba small) | ≈ 65 MB (large) |
| **4×4 total** | | | **≈ 11 MB** | **≤ 300 MB** |
| 5×5 | 5-1 x / t | 2 × 735,471 | 1.5 MB | + joint table (optional) |
| 5×5 | 5-2 | 2 × 12,870 / joint 165.6M | 26 KB | 83 MB |
| 5×5 | 5-3 wings + midges | 2,704,156 + 2,048 | 1.4 MB | same |
| 5×5 | 5-4 x / t | 2 × ≤ 343,000 | 0.7 MB | same |
| 5×5 | 5-5 pairing PDBs | design-dependent | ≤ 64 MB | ≤ 512 MB |
| **5×5 total** | | | **≤ 80 MB** | **≤ 1 GB** |

---

## 8. Hand-off to the 3×3 solver

1. **Extract** a 3×3 facelet cube from the reduced NxN: corners as-is; each paired dedge or tredge becomes one edge; each solved centre block becomes one centre.
2. **Recolour** (4×4 only) so the centres read as standard ([§5.1](#51-frame-orientation-free-goals)).
3. **Validate** with the 3×3 validator. Thanks to parity lifting this cannot fail; if it ever does, it is a bug, and the test suite treats it as one.
4. **Solve** with Kociemba (anytime, shared time budget) and map each 3×3 face turn to the NxN outer-layer turn of the same name.
5. **Concatenate** the phase solutions, simplify across phase boundaries (merge or cancel adjacent same-layer turns), and report per-phase lengths.

---

## 9. What gets visualized (big cubes)

- **Pipeline view:** each phase as a card showing its generator set, coordinate sizes, table sizes, moves found, nodes and time. The current phase is highlighted during playback.
- **Projection lens per phase:** phase 4-1 greys everything except the R/L-pair centres; phase 4-2 shows wing classes as two colours; phase 4-3 shows pairing status per dedge; and so on.
- **Parity explainer:** for a chosen phase, its Schreier graph's components (for enumerable coordinates) with the "wrong" component highlighted, plus the invariant that separates them.
- **Search trees and iteration charts** per phase, exactly as for Kociemba.

Details are in [09 · Visualization](09-visualization.md).

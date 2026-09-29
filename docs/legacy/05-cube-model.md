# 05 · Cube Model (`rg-cube`)

**Goal:** one generic model for N = 2…5 in which every move is **derived from 3D geometry**, never typed in by hand. Hand-written move tables are the most common source of subtle cube-solver bugs, and generating them from rotation matrices removes that risk for every N at once.

The model has three levels:

| Level | Representation | Used for |
|---|---|---|
| **Facelet** | `Vec<u8>` of 6·N² colours | I/O, rendering, ground-truth verification of every solution |
| **Piece (cubie)** | per orbit: permutation + orientation arrays | coordinate encode/decode, validation, specialized fast states |
| **Coordinate** | `u32`/`u64` integers | the hot path of all searches (defined in `rg-solve`, built on `rg-graph`) |

---

## 1. Frame, faces, colours

- Axes: **x → R**, **y → U**, **z → F** (right-handed).
- Face order everywhere: **U, R, F, D, L, B** (Kociemba's convention, used by most solvers and facelet strings).
- Default colour scheme (Western): U white, R red, F green, D yellow, L orange, B blue. Colours are stored as the index of their home face (0–5), so the scheme is a display concern only.

---

## 2. Sticker geometry (doubled integer coordinates)

Integer coordinates avoid floating point: each sticker centre is a point on a lattice with spacing 2. For a face-local sticker at (row r, column c), with 0 ≤ r, c < N and the face viewed from outside in the standard net orientation:

| Face | Position (x, y, z) |
|---|---|
| U | (2c − (N−1),  N,  2r − (N−1)) |
| R | (N,  (N−1) − 2r,  (N−1) − 2c) |
| F | (2c − (N−1),  (N−1) − 2r,  N) |
| D | (2c − (N−1),  −N,  (N−1) − 2r) |
| L | (−N,  (N−1) − 2r,  2c − (N−1)) |
| B | ((N−1) − 2c,  (N−1) − 2r,  −N) |

The sticker's **cubie centre** is the same point with its ±N component replaced by ±(N−1). Stickers that share a cubie centre belong to the same piece.

**Facelet index** = `face·N² + r·N + c`. For N = 3 this is the standard 54-character facelet string:

```
             ┌──────────┐
             │ U0 U1 U2 │
             │ U3 U4 U5 │
             │ U6 U7 U8 │
  ┌──────────┼──────────┼──────────┬──────────┐
  │ L0 L1 L2 │ F0 F1 F2 │ R0 R1 R2 │ B0 B1 B2 │
  │ L3 L4 L5 │ F3 F4 F5 │ R3 R4 R5 │ B3 B4 B5 │
  │ L6 L7 L8 │ F6 F7 F8 │ R6 R7 R8 │ B6 B7 B8 │
  └──────────┼──────────┼──────────┴──────────┘
             │ D0 D1 D2 │
             │ D3 D4 D5 │
             │ D6 D7 D8 │
             └──────────┘
string order: U0..U8 R0..R8 F0..F8 D0..D8 L0..L8 B0..B8
```

---

## 3. Moves as (axis, layer set, turns)

Every turn on any N, whether face, wide, inner slice, middle slice or whole-cube rotation, is one struct:

```rust
pub struct Move {
    pub axis: Axis,        // X | Y | Z
    pub layers: u8,        // bitmask; bit ℓ = layer ℓ counted from the + face (U/R/F side)
    pub turns: u8,         // 1, 2, 3 quarter turns, clockwise as seen from the + face
}
```

A cubie centre with coordinate `a` along the axis lies in layer **ℓ = ((N−1) − a) / 2**. Layer 0 is the U/R/F face, and layer N−1 is the D/L/B face.

**Clockwise quarter-turn rotations** (as seen from the + face):

| Axis | Map |
|---|---|
| x (like R) | (x, y, z) ↦ (x,  z, −y) |
| y (like U) | (x, y, z) ↦ (−z, y,  x) |
| z (like F) | (x, y, z) ↦ (y, −x,  z) |

These were hand-checked on three stickers: under `R` the top-right sticker of F goes to the back-right of U; under `U` the top-left sticker of F goes to the top-left of L; under `F` the U sticker of the UFL corner lands on R at the URF corner. The unit tests pin all three.

**Deriving a move's permutation:** for every sticker i whose cubie centre lies in a selected layer, rotate its position `turns` times, look up the index j of the resulting position, and record `src[j] = i`. Moves are stored in **gather form**, `new[j] = old[src[j]]`, which is a branch-free loop that vectorizes well.

**Notation mapping** (WCA plus a SiGN subset):

| Notation | axis | layers | turns | Notes |
|---|---|---|---|---|
| `U` / `R` / `F` | y / x / z | {0} | 1 | |
| `D` / `L` / `B` | y / x / z | {N−1} | 3 | clockwise from D/L/B = counter-clockwise from U/R/F |
| `Uw` (= `2Uw`) | y | {0,1} | 1 | |
| `3Uw` | y | {0,1,2} | 1 | |
| `Dw` | y | {N−2, N−1} | 3 | |
| `2U` | y | {1} | 1 | single inner layer (WCA-style) |
| `u` | y | {0,1} | 1 | SiGN: lowercase = two-layer wide, same as `Uw` |
| `M` / `E` / `S` | x / y / z | {(N−1)/2} | 3 / 3 / 1 | odd N only; M follows L, E follows D, S follows F |
| `x` / `y` / `z` | x / y / z | all | 1 | whole-cube rotations |
| suffix `'` / `2` | | | 4 − t / 2 | |

---

## 4. Move sets per puzzle

| Puzzle | Generators (each × {1, 2, 3} turns) | Count | Frame handling |
|---|---|---|---|
| 2×2 | U, R, F | 9 | **DBL corner fixed**: no move touches it, so the solved state is unique. Standard for 2×2 graph work. |
| 3×3 | U, D, L, R, F, B | 18 | Fixed centres define the frame. |
| 4×4 | U, D, L, R, F, B, Uw, Rw, Fw | 27 | **Orientation-free goal**: "solved" = solved in any of the 24 orientations; phase goals are chosen per colour-pair ([08 §5](08-solving-4x4-5x5.md#5-44-phase-chain)). |
| 5×5 | U, D, L, R, F, B, Uw, Dw, Lw, Rw, Fw, Bw | 36 | Fixed centres define the frame (no move in this set moves them). |

Each phase in [07](07-solving-3x3.md)/[08](08-solving-4x4-5x5.md) uses a **subset** of its puzzle's move set, expressed as a `MoveSubset` bitmask over these indices.

**Metrics:** a solution's length is computed per metric. HTM counts every face turn as 1. OBTM counts every face or wide turn as 1, and an inner slice as 2. STM counts any single contiguous block as 1. QTM counts a half turn as 2.

---

## 5. Pieces and orbits (derived automatically)

Stickers are grouped by cubie centre. Each cubie is classified by its sticker count and the absolute values of its coordinates:

| Class | Sticker count | Rule | N=2 | N=3 | N=4 | N=5 |
|---|---|---|---|---|---|---|
| Corner | 3 | all |coords| = N−1 | 8 | 8 | 8 | 8 |
| Midge | 2 | edge coordinate = 0 (odd N) | – | 12 | – | 12 |
| Wing | 2 | edge coordinate ≠ 0; one orbit per |value| | – | – | 24 | 24 |
| Fixed centre | 1 | both in-face coords = 0 | – | 6 | – | 6 |
| X-centre | 1 | |a| = |b| ≠ 0 | – | – | 24 | 24 |
| T-centre | 1 | one coord 0, other ≠ 0 | – | – | – | 24 |
| Oblique | 1 | |a| ≠ |b|, both ≠ 0 (N ≥ 6) | – | – | – | – |

This classification is cross-checked by computing the **orbits of piece positions under the move set** (union-find over every move's position permutation). The two partitions must agree, which is a unit test for every N.

**Orientation model**

| Orbit | Orientation states | Reference |
|---|---|---|
| Corners | 3 | index of the U/D-coloured sticker within the slot's sticker order |
| Midges / 3×3 edges | 2 | whether the reference sticker (U/D, else F/B) sits on the slot's reference facelet |
| Wings | – | A wing fits each slot in **exactly one** orientation, so position fully describes it |
| Centres | – | Visually symmetric; identical colours are interchangeable |

**Deriving cubie-level moves:** for each move and orbit, record where each slot's piece goes, and how its reference sticker's index changes, in a `(perm, ori_delta)` pair. These tables are produced from the facelet permutations, so they can never disagree with them. A property test checks that facelet-level and cubie-level application commute with decode/encode on random states.

**Specialized fast state.** For the 3×3 hot paths that need full states (bidirectional BFS), `Cube3 { cp: [u8; 8], co: [u8; 8], ep: [u8; 12], eo: [u8; 12] }` packs into a `u128` key (cp 8×3 bits + co 8×2 + ep 12×4 + eo 12×1 = 100 bits).

---

## 6. Identifying pieces from facelets (input decoding)

| Orbit | Identification |
|---|---|
| Corners, midges | Unordered colour set identifies the piece; the position of its U/D (or F/B) colour gives the orientation. |
| Wings | Each colour pair belongs to **two** wings (for example both white-green wings on a 4×4). They are told apart by chirality: the 24 whole-cube rotations act **simply transitively** on the 24 wing slots, so for each (slot, ordered colours) there is exactly one wing that can appear that way. A lookup table built from the solved state by applying the 24 rotations gives `(slot, colours) → wing id`. |
| Centres | Colour only (identical pieces). |

Any impossible sticker combination (a corner showing two opposite colours, a wing whose ordered colours match no piece, wrong colour counts) is rejected with a precise error.

---

## 7. Notation parser and formatter

Grammar (EBNF):

```text
alg        = { ws } , [ item , { ws+ , item } ] , { ws } ;
item       = move | group ;
group      = "(" , alg , ")" , [ count ] ;
move       = [ layercount ] , face , [ "w" ] , [ amount ]
           | wide_lower , [ amount ]            (* SiGN: u d l r f b = two-layer wide *)
           | slice , [ amount ]                 (* M E S, odd N *)
           | rotation , [ amount ] ;
layercount = digit , { digit } ;                (* "3Uw" = 3 outer layers; "2U" = single 2nd layer *)
face       = "U" | "D" | "L" | "R" | "F" | "B" ;
wide_lower = "u" | "d" | "l" | "r" | "f" | "b" ;
slice      = "M" | "E" | "S" ;
rotation   = "x" | "y" | "z" ;
amount     = "2" | "'" | "2'" ;
count      = digit , { digit } ;
```

- Errors carry character spans, so the UI can underline the bad token.
- The **formatter** prints in a chosen style (WCA / SiGN), merges consecutive same-layer turns, and cancels identities (`R R'` disappears, `R R` becomes `R2`).
- `invert(alg)` and `simplify(alg)` are part of the public API.
- **Rotation elimination:** `strip_rotations(alg)` pushes rotations to the end by conjugation and drops them ([02 §9](02-graph-theory-foundations.md#9-symmetry--graph-automorphisms)).

---

## 8. Validation: "is this state solvable?"

**Principle:** a state is solvable if it lies in the connected component of *solved*. For these puzzles, the component is characterized exactly by a small set of **homomorphism invariants**, which are **derived automatically from the move set**:

1. For each orbit with distinguishable pieces, compute for every generator its **permutation parity** (Z₂) and its **orientation sum** (Z₃ for corners, Z₂ for midges).
2. The reachable invariant values are the span of the generator vectors: GF(2) for parities, and Z₃ / Z₂ for orientation sums.
3. The input is valid if and only if its invariant vector lies in that span, the colour counts are right, and every piece is identifiable.

The derivation reproduces the known laws, and a test asserts them:

| Puzzle | Derived laws |
|---|---|
| 3×3 | corner twist sum ≡ 0 (mod 3); edge flip sum ≡ 0 (mod 2); corner parity = edge parity |
| 2×2 | corner twist sum ≡ 0 (mod 3); any permutation |
| 4×4 | corner twist sum ≡ 0 (mod 3); wings: any permutation; centres: identical pieces, so parity is invisible |
| 5×5 | corner twist sum ≡ 0 (mod 3); midge flip sum ≡ 0 (mod 2); corner parity = midge parity; wings any; centres invisible |

Error messages name the failing invariant ("one corner is twisted", "two pieces are swapped", "an edge is flipped"), and the editor highlights candidate pieces.

---

## 9. Random states and scrambles

- **Random state (all N).** Sample every orbit uniformly (Fisher–Yates for permutations, uniform orientations), then repair the invariants: fix the last corner's twist, the last midge's flip, and swap two pieces if the parities disagree. Every legal state is equally likely.
- **Scramble string.** Solve the random state with the default solver and invert the solution. This is exactly how official random-state scramblers work.
- **Random-move scramble.** Available for the "watch it get harder" demos (depth-k scrambles show how search cost grows with distance).
- Seeds are `u64`, and every run can be reproduced from its seed.

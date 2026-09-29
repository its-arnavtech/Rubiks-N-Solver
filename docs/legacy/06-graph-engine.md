# 06 · Graph Engine (`rg-graph`)

The engine is **puzzle-agnostic**. It knows about graphs, integer coordinate spaces, distances and search, and nothing about cubes. Everything cube-specific is plugged in from `rg-solve`.

---

## 1. Module map

| Module | Responsibility |
|---|---|
| `combinatorics` | ranking/unranking: permutations, combinations, orientations, binomial tables |
| `space` | `ImplicitGraph`, `IndexedSpace`, `MoveTable`, product spaces |
| `coord` | `Coordinate` trait, representative-based move-table generation, congruence checks |
| `bfs` | layered BFS over indexed spaces → distance tables; component labelling; CSR export |
| `table` | `PruningTable` (u8 / nibble / 2-bit mod 3), `.rgt` I/O, definition hashing |
| `bidi` | bidirectional BFS over hashed full states |
| `ida` | resumable IDA\* with composite heuristics, canonical filter, solution enumeration |
| `sym` | symmetry groups acting on moves and coordinates; class-reduced tables |
| `phase` | multi-phase chain driver (Thistlethwaite-style, Kociemba-style, NxN reduction) |
| `invariants` | GF(2)/Z_k invariant derivation (parity lifting, validation support) |
| `observe` | `Observer` trait, `NoopObserver`, `RecordingObserver`, telemetry buffers |

---

## 2. Core abstractions

```rust
/// A graph whose vertices are never stored. Edges are labelled by move indices.
pub trait ImplicitGraph {
    type Node: Clone;
    fn degree(&self) -> usize;                                     // number of move labels
    fn step(&self, node: &Self::Node, mv: usize) -> Self::Node;    // follow edge `mv`
    fn canonical(&self) -> &CanonicalFilter;                       // allowed-next-move masks
}

/// A finite graph whose vertices are 0..size (a coordinate space / quotient graph).
pub trait IndexedSpace {
    fn size(&self) -> u64;
    fn degree(&self) -> usize;
    fn neighbor(&self, v: u64, mv: usize) -> u64;
}

/// Admissible lower bound on distance-to-goal.
pub trait Heuristic<N> {
    fn h(&self, node: &N) -> u8;
}

/// Receives search events; NoopObserver compiles to nothing.
pub trait Observer {
    fn expand(&mut self, depth: u8, mv: u16, h: u8) {}
    fn prune(&mut self, depth: u8, f: u8) {}
    fn iteration(&mut self, threshold: u8, nodes: u64) {}
    fn layer(&mut self, side: Side, depth: u8, size: u64) {}
    // ... see §9
}
```

**Why two graph traits?** `ImplicitGraph` is for the huge graphs we walk (full cube states, product coordinates). `IndexedSpace` is for the small graphs we fully enumerate with BFS (quotients). A product of coordinates is itself an `ImplicitGraph` whose node is a tuple of `u32`s, so IDA\* in phase 1 of Kociemba walks the node `(twist, flip, slice)` with three table lookups per edge.

---

## 3. Combinatorics (ranking)

Every coordinate is a bijection between a combinatorial object and `0..size`:

| Object | Size | Ranking method |
|---|---|---|
| Permutation of n (n ≤ 24) | n! | Lehmer code, O(n²) (n is small; an O(n log n) variant exists but isn't needed) |
| Partial permutation (k of n, ordered) | n!/(n−k)! | Lehmer code over the chosen pieces |
| Combination (k of n) | C(n,k) | combinatorial number system with a precomputed binomial table (`C(24,12)` fits easily in u32) |
| Orientation vector (n pieces, base b, last implied) | b^(n−1) | mixed radix |
| Parity bit | 2 | sign of a permutation |

Example sizes that recur: C(12,4) = 495, C(8,4) = 70, C(16,8) = 12,870, C(24,8) = 735,471, C(24,12) = 2,704,156, 3⁷ = 2,187, 2¹¹ = 2,048, 8! = 40,320.

Unit tests: `unrank(rank(x)) == x` exhaustively for sizes ≤ 10⁶ and by proptest above that.

---

## 4. Coordinates and move tables

```rust
pub trait Coordinate {
    type Rep;                                    // a representative full state (cubie arrays)
    fn size(&self) -> u64;
    fn encode(&self, rep: &Self::Rep) -> u64;    // state -> coordinate
    fn decode(&self, c: u64) -> Self::Rep;       // coordinate -> some state with that coordinate
}
```

**Move-table generation (generic):** for each c in `0..size` and each move m, `table[c][m] = encode(apply(decode(c), m))`. This is correct *only if the coordinate is compatible* ([02 §4.1](02-graph-theory-foundations.md#41-coordinates)), so every coordinate gets a **congruence test**: for random pairs of states with equal coordinates, applying the same move must give equal coordinates. An incompatible coordinate fails in milliseconds instead of silently producing wrong tables.

**Two execution styles, chosen per coordinate size:**

| Style | When | Cost per edge | Memory |
|---|---|---|---|
| **Move table** `Vec<u16/u32>` of size × moves | size ≲ 2¹⁸ | 1 load | size × moves × 2–4 B |
| **Compact piece state + rank on demand** | large coordinates (for example C(24,8) with 27 moves would need 79 MB as a u32 table) | a few byte-table permutations of a bitmask + one rank | tiny |

For subset coordinates ("which 8 of 24 slots hold R/L centres"), the compact state is a 24-bit mask. A move is a slot permutation applied to the mask via three 256-entry byte lookup tables per move, and ranking uses precomputed binomial tables. This keeps the 4×4 and 5×5 phases small in memory.

---

## 5. BFS: distance tables, components, exports

### 5.1 Pruning-table builder

```text
build_pdb(space, goals, encoding):
    dist[*] = UNKNOWN; for g in goals: dist[g] = 0
    depth = 0; filled = |goals|
    while filled < size and layer(depth) non-empty:
        mode = if filled < size/2 { Forward } else { Backward }   // classic switch
        parallel for chunk in 0..size:
            Forward : for v with dist[v] == depth:
                          for m: w = neighbor(v, m); if dist[w] == UNKNOWN: dist[w] = depth+1
            Backward: for v with dist[v] == UNKNOWN:
                          if any m: dist[neighbor(v, m)] == depth: dist[v] = depth+1
        depth += 1; record histogram[depth]; observer.layer(...)
```

- **Multi-source:** `goals` can be a set, for example "any of the 24 orientations of solved" or "these 3 dedges paired anywhere".
- **Forward/backward switching:** once the unknown vertices are the minority, checking unknowns is cheaper than expanding the frontier.
- **Parallelism (native):** chunks are processed with rayon. Writes are idempotent (any writer stores the same depth + 1), so a relaxed atomic compare-and-swap on the containing `u32` word is enough. The result is **deterministic**, because BFS distances are unique.
- **Encodings:** `u8` for small tables and exact tables used in greedy descent; **nibble** (4 bits, values 0–14, 15 = unknown) as the default; **2-bit mod 3** (Kociemba) for the largest. The exact value is recovered by stepping to a neighbour whose value is one less mod 3.
- **Histogram** of layer sizes is stored in the file header. It is a test oracle and a chart.

### 5.2 Components and invariants

`components(space)` labels connected components with a BFS or union-find sweep. It is used by the Phase Lab and by parity lifting ([08 §3](08-solving-4x4-5x5.md#3-parity-lifting-connectivity-requirements-flow-backwards)).

### 5.3 Graph export (for visualization)

`export_csr(space, max_nodes)` returns `offsets: Uint32Array, targets: Uint32Array, labels: Uint8Array, dist: Uint8Array`. Self-loops and parallel edges are dropped or merged. It is used for the coset-graph explorer on spaces up to about 50k vertices.

---

## 6. IDA\*

### 6.1 State machine

```rust
struct Frame<N> { node: N, g: u8, next_mv: u8, last_mv: u8 }

pub struct IdaStar<G, H, Goal, O> {
    graph: G, heur: H, goal: Goal, obs: O,
    stack: Vec<Frame<G::Node>>, threshold: u8, next_threshold: u8,
    root: G::Node, max_depth: u8, stats: Stats,
}

impl<...> IdaStar<...> {
    pub fn step(&mut self, mut budget: u64) -> IdaStep {
        while budget > 0 {
            let Some(top) = self.stack.last_mut() else {
                if self.next_threshold > self.max_depth { return IdaStep::Exhausted; }
                self.threshold = self.next_threshold; self.next_threshold = INF;
                self.obs.iteration(self.threshold, self.stats.nodes);
                self.push_root(); continue;
            };
            if top.next_mv as usize == self.graph.degree() { self.stack.pop(); continue; }
            let mv = top.next_mv; top.next_mv += 1;
            if !self.graph.canonical().allows(top.last_mv, mv) { continue; }
            let child = self.graph.step(&top.node, mv as usize);
            let (g, h) = (top.g + 1, self.heur.h(&child));
            budget -= 1; self.stats.nodes += 1;
            if g + h > self.threshold { self.next_threshold = self.next_threshold.min(g + h);
                                        self.obs.prune(g, g + h); continue; }
            self.obs.expand(g, mv as u16, h);
            if h == 0 && (self.goal)(&child) { return IdaStep::Found(self.path_with(mv)); }
            self.stack.push(Frame { node: child, g, next_mv: 0, last_mv: mv });
        }
        IdaStep::Progress
    }
}
```

After a `Found`, calling `step` again **continues the enumeration**: it finds the next solution of the same length, then longer ones. The multi-phase driver needs exactly this.

### 6.2 Heuristic composition

`MaxHeuristic<(A, B, C)>` evaluates tuple members and returns the max, with early exit once the max already exceeds the remaining budget (threshold − g). The most selective table is evaluated first.

### 6.3 Canonical move filter

`CanonicalFilter` is a `Vec<u64>` indexed by the previous move. Bit m is set if move m may follow. It is built from move metadata (axis, layer mask):

- same axis and same layer set → forbidden (they would merge);
- same axis, different layer sets → allowed only in increasing layer-mask order (same-axis turns commute);
- different axes → allowed.

This rule is generic for all N ([02 §10](02-graph-theory-foundations.md#10-canonical-sequences-removing-duplicate-paths)).

### 6.4 Enhancements (added in this order, each benchmarked)

1. **Move ordering by h:** try children with the smallest h first. This finds a solution sooner in the final iteration.
2. **Inverse and conjugate lookups:** h = max(h(g), h(g⁻¹), h(σgσ⁻¹)) where the coordinates support it (Korf; Kociemba's three axes).
3. **Phase-boundary rule (Kociemba):** a phase-1 solution whose last move already lies in the phase-2 generator set is skipped, because a shorter phase-1 solution reaching the same coset exists.
4. **Parallel root split (native only):** distribute depth-1 or depth-2 subtrees across threads for Korf.

---

## 7. Bidirectional BFS

- Two `FxHashMap<Key, u8 /*last move*/>` maps: forward from the start, backward from the goal.
- Each step **expands the smaller frontier** by one full layer, checking every new state against the opposite map.
- The **last layer can be streamed**: the deepest layer on one side is generated and checked on the fly without storing it, which buys one extra ply of reach for free.
- The path is reconstructed by walking stored last-moves back to each root.
- Hard caps: `max_nodes` and `max_bytes`, reporting `Exhausted { reached_depth }` rather than crashing.
- Resumable: the frontier cursor is saved between `step` calls.
- Emits `Frontier` events per layer (sizes, memory) for the meet-in-the-middle visualization.

---

## 8. Symmetry

```rust
pub struct SymGroup {
    pub order: usize,                 // 16 (U/D-preserving) or 48 (full O_h)
    pub move_conj: Vec<Vec<u8>>,      // move_conj[s][m] = index of σ_s m σ_s⁻¹
    pub inverse: Vec<u8>,
}
pub struct SymCoord {
    pub classes: u32,                 // number of equivalence classes
    pub raw_to_class: Vec<u32>,       // class index
    pub raw_to_sym: Vec<u8>,          // which σ maps raw → representative
    pub rep_of_class: Vec<u32>,
    pub self_sym: Vec<u64>,           // stabilizer mask per class (needed for correct BFS)
}
```

- Symmetry group elements are generated from two or three geometric generators (a rotation about an axis, an axis swap, a mirror), realized as **facelet permutations** from `rg-cube`, so they are geometry-derived too.
- Mirror symmetries map clockwise to counter-clockwise; this is handled in `move_conj`.
- A symmetry-reduced PDB is indexed by `class × other_coord`. Lookup is `table[class(c1) · |c2| + sym_apply(c2, sym(c1))]`.
- Tests: class counts must match known values (for example 64,430 flip-slice classes under the 16 U/D symmetries, and 2,768 corner-permutation classes).

---

## 9. Telemetry (Observer API)

```rust
pub trait Observer {
    fn session(&mut self, info: &SessionInfo) {}
    fn phase_start(&mut self, phase: u8, info: &PhaseInfo) {}
    fn phase_end(&mut self, phase: u8, report: &PhaseReport) {}
    fn iteration(&mut self, threshold: u8, nodes: u64) {}
    fn expand(&mut self, depth: u8, mv: u16, h: u8) {}
    fn prune(&mut self, depth: u8, f: u8) {}
    fn layer(&mut self, side: Side, depth: u8, size: u64) {}
    fn solution(&mut self, moves: &[u16]) {}
    fn table_progress(&mut self, table: TableId, depth: u8, filled: u64) {}
}
```

`RecordingObserver` keeps a bounded **tree aggregate** (a count per move-prefix up to depth K, deeper nodes folded into their ancestor), a **reservoir sample** of expanded nodes (depth, h, f), and plain counters. `drain()` serializes them into a `u32` buffer ([04 §6](04-system-architecture.md#6-telemetry-pipeline)). All observer calls are behind generics, so `NoopObserver` gives zero overhead, and a criterion benchmark verifies it.

---

## 10. Table file format (`.rgt`)

Little-endian, versioned, self-describing:

| Offset | Size | Field |
|---|---|---|
| 0 | 4 | magic `RGT1` |
| 4 | 2 | format version |
| 6 | 2 | kind: 0 = PDB u8, 1 = PDB nibble, 2 = PDB 2-bit mod 3, 3 = move table u16, 4 = move table u32, 5 = sym-coord index |
| 8 | 8 | entry count |
| 16 | 16 | **definition hash** (xxh3-128 of the canonical definition text) |
| 32 | 8 | payload checksum (xxh3-64 of the uncompressed payload) |
| 40 | 1 | max depth (PDBs) |
| 41 | 1 | moves per entry (move tables) |
| 42 | 22 | reserved |
| 64 | var | table id (u16 length + UTF-8), histogram (u64 × (max depth + 1)) |
| … | var | zlib-compressed payload |

The **canonical definition text** is a stable, human-readable string, for example:

```text
rgt/1 n=3 coord=twist*udslice gens=U,U2,U',D,D2,D',R,R2,R',L,L2,L',F,F2,F',B,B2,B' goals=[0] enc=nibble engine=1
```

It is printed by `rgraph tables list`, which makes table mismatches easy to debug.

---

## 11. Phase-chain driver

```rust
pub struct PhaseSpec {
    pub name: &'static str,
    pub gens: MoveSubset,             // edges available in this phase
    pub coords: Vec<CoordRef>,        // what the phase "sees"
    pub goal: GoalSpec,               // goal values (possibly many: multi-goal)
    pub heuristics: Vec<TableRef>,    // PDBs combined with max
    pub exact: bool,                  // exact full table → greedy descent, no search
    pub filters: Vec<FilterRef>,      // lifted invariants / component checks at goal time
}

pub struct ChainSearch { phases: Vec<PhaseSpec>, strategy: ChainStrategy, limits: Limits }
pub enum ChainStrategy {
    Greedy,                           // Thistlethwaite: first optimal path of each phase
    Anytime { max_candidates: u32 },  // Kociemba/NxN: enumerate phase-i paths, bound later phases
}
```

The **anytime strategy** is a depth-first search over *phase solutions*. It takes phase-1 solutions in increasing length; for each one it searches phase 2 with bound `best_total − len₁ − 1`, and so on down the chain. When the time budget ends, it returns the best found. Each improvement is emitted as `SolutionImproved`, and the UI plots length against time.

---

## 12. Invariant derivation (GF(2) / Z_k)

```text
for each generator s: vector v_s = (parity of orbit₁, parity of orbit₂, …, twist_sum mod 3, flip_sum mod 2, …)
Reachable invariant values  = span{v_s}
Invariant functionals        = nullspace (annihilator) of the generator matrix
```

This is used to validate input cubes ([05 §8](05-cube-model.md#8-validation-is-this-state-solvable)) and to **lift parity requirements** between phases ([08 §3](08-solving-4x4-5x5.md#3-parity-lifting-connectivity-requirements-flow-backwards)). The implementation is plain Gaussian elimination over GF(2) (bitsets) and over Z₃. The matrices are tiny, at most a few dozen columns.

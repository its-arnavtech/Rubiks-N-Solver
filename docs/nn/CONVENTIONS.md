# Conventions & Contracts

Exact numbering, encodings and file formats. **Every component must follow this file.** If code and this file disagree, it's a bug. Fix one of them *and* note the fix in CHANGELOG.md. Changing anything here that affects `library.json` or checkpoints requires bumping `library_version` (see §7).

---

## 1. Facelets (identical to `rg-cube`)

- Faces are numbered **U=0, R=1, F=2, D=3, L=4, B=5**.
- `sticker_index = face·N² + row·N + col`, where `row, col ∈ 0..N`, face-local, viewed from outside in the standard net orientation.
- The 3D positions follow `rg-cube/src/geometry.rs::sticker_position` exactly. Copy that function as the reference and cite it.
- Colors are `u8` in 0..5. In the solved state, `color(sticker) = face(sticker)`.
- Wire format for a facelet array: base64 of the raw `u8` bytes, in sticker-index order.

## 2. Axes, layers and moves

- Axes: **x → R, y → U, z → F** (same as `rg-cube`).
- Layer `ℓ ∈ 0..N` along an axis counts **from the positive face**:
  - `ℓ = 0` is the R/U/F face layer.
  - `ℓ = N−1` is the L/D/B face layer.
- **Fixed-corner frame:** layer `N−1` is **never** turned on any axis, so the D-L-B corner never moves. Allowed layers are `0..=N−2`.
- `turns ∈ {1, 2, 3}`: clockwise quarter turns as seen from the positive face.
- **Wire encoding** (`u32`): `(layer << 4) | (axis << 2) | turns`, with axis x=0, y=1, z=2. Move lists travel as base64 of little-endian `u32`s.
- `MID = (N−1)/2` exists for odd N only.
- **Display notation:**
  - layer 0 → `R`, `U`, `F`
  - layer ℓ ≥ 1 → `{ℓ+1}R`, `{ℓ+1}U`, `{ℓ+1}F` (so `2R` is the slice next to R)
  - suffixes: `''` (1 turn), `'2'` (2 turns), `"'"` (3 turns)
- **rg-cube oracle mapping:** an nx move `(axis, ℓ, t)` equals rg-cube `Move{axis, layers: 1<<ℓ, turns: t}` for N ≤ 7.

## 3. Orbit enumeration order and ids

Orbit ids are assigned deterministically, in this order:
1. `Corner`
2. `MidEdge` (odd N)
3. `FixedCenter` (odd N, not a 24-slot orbit)
4. `Wing` by `p` ascending
5. `XCenter` by `a`
6. `PlusCenter` by `a`
7. `ObliqueA` by `(a, b)` lexicographic
8. `ObliqueB` by `(a, b)` lexicographic

Type ids for the network: `Corner=0, MidEdge=1, Wing=2, XCenter=3, PlusCenter=4, ObliqueA=5, ObliqueB=6`.

## 4. Canonical slots (24 per orbit)

**Invariance requirement (hard gate):** for every action, the slot effect measured on any instance of its type, at any N, must be identical. The rules below are the initial definition. If the invariance test fails, the **mapping** is wrong, not the test. Fix it here and in code.

**Center orbits** (`XCenter`, `PlusCenter`, `ObliqueA`, `ObliqueB`):
- Center-grid coords are face-local `(r, c)` with `1 ≤ r, c ≤ N−2`. Let `q = ⌊(N−2)/2⌋`.
- Face rotation `rot(r, c) = (c, N−1−r)` (a clockwise quarter turn of the face as seen from outside). An orbit has exactly 4 positions on each face, related by `rot`.
- **Representative `(a, b)`:** the member with `1 ≤ r ≤ q` and `1 ≤ c ≤ q`. If no member qualifies (only `PlusCenter`), use the member `(r, MID)` with `r < MID`.
- **Classification:**
  - `a == b` → `XCenter`
  - `b == MID` → `PlusCenter`
  - `a < b` → `ObliqueA`
  - `a > b` → `ObliqueB`
- **Slot** = `face·4 + k`, where position = `rot^k(a, b)` on that face (k = 0..3).
- **Layer binding:** `A = a`, `A_BAR = N−1−a`, `B = b`, `B_BAR = N−1−b`.

**Wing orbits** (depth `p`, `1 ≤ p ≤ q`, members at edge positions `p` and `N−1−p`):
- Edge order `e = 0..11`: **UB, UR, UF, UL, FR, FL, BR, BL, DF, DR, DB, DL**. The first-named face is `f1`.
- Position `t ∈ 1..N−2` counts the edge's non-corner stickers on `f1` in increasing sticker index.
- The orbit's two members on edge `e` sit at `t = p` and `t = N−1−p`.
- **Slot** = `2e + s`, where `s` is the member's handedness: `s = 0` if `(n_f1 × n_f2) · c > 0`, else `s = 1`. Here `n_f` is the outward unit normal of face `f` and `c` the member's cubie centre (doubled coordinates, §1). *(Fixed in M2.3: the first draft used "s = 0 at t = p", which is not chirality-consistent on every edge, so colors could not determine identity. Rotations preserve the triple product, so this rule makes §5 hold.)*
- **Layer binding:** `A = p`, `A_BAR = N−1−p`.

**Corner orbit:**
- Slot order `URF, UFL, ULB, UBR, DFR, DLF, DBL, DRB` (index `i`).
- Slot content = `piece·3 + orientation`, using Kociemba orientation (twist of the U/D-colored sticker).
- `DBL` (slot 6) is the fixed corner and is always solved.

**MidEdge orbit (odd N):**
- Slot order `UR, UF, UL, UB, DR, DF, DL, DB, FR, FL, BL, BR`.
- Content = `piece·2 + orientation`, using Kociemba orientation.

**FixedCenter frame (odd N):** the 6 face-center stickers. Its state is one of 24 rotations. It is not given to the network.

## 5. Piece identity from colors

- **Corner / MidEdge:** identity is the standard lookup from the color set. Orientation comes from where the U/D color sits (edges: Kociemba rule).
- **Wing:** the ordered color pair `(color on f1-side sticker, color on f2-side sticker)` plus the slot's `s` bit uniquely determines the wing's home slot.
  - Build the lookup table by exhaustive placement on a `LabeledCube` at N = 4.
  - Test on random states for N up to 20: identity from colors must equal the labeled identity.
- **Centers:** identity is not recoverable (identical colors), and never needed. Content = color.

## 6. Random states (for evaluation and the web)

`random_state(n, seed)` must be uniform over reachable states. Generate it orbit by orbit:

- **FixedCenter (odd N):** uniform over its 24 frame rotations.
- **Corner:** a uniform permutation of the 7 movable corners (DBL is fixed), with uniform twists whose sum ≡ 0 mod 3.
- **MidEdge (odd N):** a uniform permutation with uniform flips whose sum is even. The permutation parity is forced so that `parity(corners) ⊕ parity(mid edges) ⊕ parity(frame) = 0`.
- **Wing:** a uniform permutation of 24. Wing parity is free.
- **Center types:** a uniform arrangement of 4 pieces of each color.

The RNG is `rand_chacha::ChaCha8Rng` seeded with `seed`, so results are identical across OSes.

**Tests:**
- every `random_state` passes `validate`
- every `random_state` is solved by the baseline
- `validate` rejects states with one corner twisted, and states with one wing swap made on the odd law (a 3×3 parity violation)

## 7. `artifacts/macros/library.json`

```json
{
  "library_version": 1,
  "sha256": "<hex of canonical JSON of this object with the sha256 field removed>",
  "generator": { "probe_n": 12, "search": { "...": "..." }, "git_commit": "..." },
  "types": {
    "Wing": {
      "type_id": 2,
      "content": "piece",
      "orientation_mod": 1,
      "layer_refs": ["OUTER", "A", "A_BAR"],
      "macros": [
        { "id": 0, "moves": ["x:A:1", "y:OUTER:3", "..."], "kind": "three_cycle", "cycle": [3, 17, 8] }
      ],
      "actions": [
        { "id": 0, "setup": ["y:OUTER:1"], "macro": 0, "cost": 10,
          "perm": [0, 1, 2, "... 24 ints"], "ori_delta": [0, 0, "... 24 ints"] }
      ]
    }
  }
}
```

- **Symbolic move string:** `"<axis>:<ref>:<turns>"`, where axis ∈ `x|y|z`, ref ∈ `OUTER|MID|A|A_BAR|B|B_BAR`, turns ∈ `1|2|3`.
- **Action semantics** (gather): `new[i] = old[perm[i]]`, then `orientation(new[i]) = (orientation(new[i]) + ori_delta[i]) mod orientation_mod`.
- `cost` = number of primitive moves in `setup + macro + inverse(setup)` after cancellation. It is N-independent.
- **Canonical JSON:** keys sorted, no whitespace, UTF-8. Regenerating with the same config must produce the same bytes.
- Any change to the slot conventions, action ordering or JSON semantics **bumps `library_version`**. Old checkpoints then refuse to load.

## 8. Local HTTP API (`127.0.0.1:8000`)

| Method & path | Request | Response |
|---|---|---|
| `GET /api/health` | — | `{status, checkpoint, library_sha256, device}` |
| `POST /api/scramble` | `{n, mode: "random_state" \| "moves", seed, length?}` | `{n, facelets_b64, scramble_moves_b64 \| null}` |
| `POST /api/solve` | `{n, facelets_b64, solver: "nn" \| "baseline", beam?: 1, max_steps?: 64}` | `SolveResult` |
| `GET /api/orbits/{n}` | — | `{orbits: [{id, type, indices}], sticker_orbit_b64}` (u32 orbit id per sticker) |

The server enforces `2 ≤ n ≤ 100`, configurable. The CLI has no cap.

**`SolveResult`:**
```json
{
  "n": 100, "solver": "nn", "verified": true,
  "moves_b64": "...", "raw_len": 251234, "cancelled_len": 238901,
  "segments": [
    { "phase": "parity|core_frame|core|orbits|fallback", "orbit_id": 57, "orbit_type": "XCenter",
      "round": 3, "action_id": 812, "q": 41.7, "start": 10432, "end": 10444 }
  ],
  "stats": { "orbits_total": 2451, "orbits_fallback": 0, "rounds": 14 },
  "timings_ms": { "extract": 0, "plan": 0, "emit": 0, "verify": 0, "total": 0 },
  "checkpoint": "artifacts/checkpoints/<run>/step_<n>.pt", "library_sha256": "..."
}
```

- `segments` index the **raw** move list (before cancellation). The web replays raw moves.
- `verified` is always `true`. An unverified result is an HTTP 500 with a message, never a success response.
- If a payload exceeds about 5 MB, `segments` may switch to a columnar form (`{phase: [...], orbit_id: [...], ...}`). Document the change here if it happens.

## 9. Files, seeds, naming

- **Run ids:** `YYYYMMDD-HHMM-<config-name>`.
- **Checkpoints:** `artifacts/checkpoints/<run_id>/step_<n>.pt`. The pointer file `artifacts/checkpoints/CURRENT` contains one relative path.
- **Seeds:** every CLI command and config takes `seed`. Default seed 0. Tests use fixed seeds.
- **Rust:** crates are named `nx-*`. Python package `nxnn`, native module `nxsim`.
- **Commits:** `M<milestone>.<task>: <summary>`, e.g. `M1.3: oracle tests vs rg-cube`.

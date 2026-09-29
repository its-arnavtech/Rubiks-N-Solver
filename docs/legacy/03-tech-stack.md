# 03 · Tech Stack

Every choice below is judged against four forces:
1. **Raw search speed.** IDA\* on the 4×4 and 5×5 expands 10⁷–10⁹ nodes, so the inner loop must be tight.
2. **Memory control.** Pruning tables of tens to hundreds of MB, packed at 4 bits per entry.
3. **Rich visualization in the browser.** WebGL for 3D and for large graphs.
4. **One codebase for native and browser.** Tables are generated natively (multi-threaded, fast); the same code solves in the browser.

---

## 1. Summary

| Layer | Choice | Version policy |
|---|---|---|
| Engine language | **Rust**, edition 2024 | stable toolchain; this machine has 1.96 |
| Browser target | **WebAssembly** (`wasm32-unknown-unknown`) + **wasm-bindgen** | `wasm-bindgen-cli` pinned to the same version as the crate in `Cargo.lock` |
| Native parallelism | **rayon** | latest 1.x |
| Web language | **TypeScript** (strict) | 5.x |
| Build/dev server | **Vite** | latest major |
| UI framework | **React** | 19.x |
| App state | **Zustand** | latest |
| Styling | **Tailwind CSS** | v4 |
| 3D | **three.js** (+ `OrbitControls` from its addons) | latest |
| Network graphs | **sigma.js** v3 + **graphology** (+ `graphology-layout-forceatlas2` worker) | latest |
| Charts / trees | **d3** modules: `d3-scale`, `d3-axis`, `d3-shape`, `d3-hierarchy`, `d3-zoom` | latest v3/v4 modules |
| Worker RPC | **Comlink** | latest |
| Browser cache | **IndexedDB** via `idb-keyval` | latest |
| Package manager | **pnpm** | this machine has 11.22 |
| Runtime | **Node.js** | this machine has 24.18 (LTS line) |
| Rust tests/bench | `cargo test`, **proptest**, **criterion** (optionally `cargo-nextest`) | |
| Web tests | **Vitest** (unit), **Playwright** (end-to-end smoke and screenshots) | |
| Lint/format | `rustfmt`, `clippy` (warnings are errors in CI), **Biome** (TS lint + format) | |
| Build orchestration | **`cargo xtask`** (a Rust crate that scripts builds cross-platform) | |
| CI | GitHub Actions (once the repo is pushed) | |

---

## 2. Engine: Rust, compiled natively and to WebAssembly

**Why Rust**
- **Speed.** Native code with no garbage collector, `#[inline]` hot paths, SIMD-friendly byte arrays. IDA\* with table lookups runs at tens of millions of nodes per second per core natively. Rust compiled to WebAssembly typically keeps a large fraction of that (roughly half to two-thirds is a reasonable planning figure), which is far above what idiomatic JS achieves on this workload.
- **Memory layout control.** Nibble-packed `Vec<u8>` tables, `[u8; 20]` cubie arrays and `u128` packed keys are all direct and cheap.
- **Same code, two targets.** The `rgraph` CLI generates tables with every core (rayon), and the identical crates compile to `wasm32` for the browser. That makes native and wasm results comparable bit for bit, which is itself a test ([11](11-testing-and-performance.md)).
- **Tooling.** Cargo workspaces, proptest, criterion, clippy.

**Alternatives considered**

| Alternative | Why not |
|---|---|
| TypeScript only | Several times slower in search inner loops; GC pauses; awkward bit-packing without `u64`. Fine for the UI, wrong for the engine. |
| Python (+ NumPy/Numba) | Great for prototyping, but about 100× slower in pure-Python IDA\*. Browser story needs a server or Pyodide. |
| C++ → Emscripten | Comparable speed, but heavier toolchain on Windows, no Cargo, weaker safety for a large codebase. |
| Go → wasm | Large wasm binaries, GC, slower numeric code in wasm. |
| Server backend + thin client | Adds deployment and latency; a static site with wasm is simpler and works offline. |

**WASM toolchain decision:** use `cargo build --target wasm32-unknown-unknown --release`, then `wasm-bindgen --target web`. `cargo xtask wasm` wraps both steps. `wasm-pack` would also work, but its development has slowed since the Rust-WASM working group's GitHub organization was archived, and it would only add a wrapper layer. Optional: `wasm-opt -O3` (Binaryen, installable as the `wasm-opt` crate) for about 10–20% smaller and faster output in release builds.

**WASM memory:** `wasm32` gives up to 4 GB of linear memory. We budget **≤ 1 GB** per worker (see [04 §7](04-system-architecture.md#7-memory-model)). Browser threads (SharedArrayBuffer) need cross-origin isolation headers and nightly Rust for `std::thread` in wasm, so we deliberately avoid them. Concurrency comes from **multiple workers**, and responsiveness comes from **resumable search** ([ADR-004](12-decision-log.md)).

### 2.1 Rust crates used

| Crate | Where | Purpose |
|---|---|---|
| `rayon` | rg-graph (feature `parallel`), rg-cli | parallel BFS table generation, parallel move-table builds |
| `rustc-hash` | rg-graph | FxHashMap/FxHashSet for bidirectional BFS visited sets |
| `serde`, `serde_json` | rg-solve, rg-wasm, rg-cli | request/response payloads, Phase Lab reports |
| `serde-wasm-bindgen` | rg-wasm | zero-copy-ish JS object conversion |
| `wasm-bindgen`, `js-sys` | rg-wasm | JS bindings, typed arrays |
| `console_error_panic_hook` | rg-wasm | readable panics in the browser console |
| `miniz_oxide` | rg-graph (table I/O) | zlib (de)compression of `.rgt` payloads, pure Rust, wasm-friendly |
| `xxhash-rust` (xxh3) | rg-graph | payload checksums and definition hashes |
| `rand`, `rand_xoshiro` | rg-cube | seeded, reproducible scrambles and random states |
| `thiserror` | libraries | typed errors |
| `anyhow`, `clap` | rg-cli, xtask | CLI ergonomics |
| `proptest` | dev | property tests (move algebra, coordinate round-trips) |
| `criterion` | dev | micro-benchmarks with regression detection |

---

## 3. Web application

| Choice | Why | Alternatives rejected |
|---|---|---|
| **Vite** | Instant dev server, native ES modules, first-class `new Worker(new URL(...), {type:'module'})` and `.wasm` asset handling | Webpack (slow config), Parcel |
| **React 19 + TypeScript** | Mature ecosystem, well-known; rendering-heavy views are imperative (three.js/sigma) and sit behind thin React wrappers | Svelte/Solid (lighter, but smaller ecosystem for graph and 3D components) |
| **Zustand** | Tiny global store; subscribes outside React, which the render loops need | Redux (boilerplate), Context (re-render storms) |
| **Tailwind CSS v4** | Fast, consistent UI styling without a component library | CSS-in-JS runtime cost; a UI kit would add weight |

### 3.1 Rendering libraries

| Need | Choice | Why | Scale limit we plan for |
|---|---|---|---|
| 3D cube (2×2 → 5×5), animated turns | **three.js** (plain, imperative) | Full control of the animation queue, instancing (98 cubies and 150 stickers on the 5×5), custom shaders for the *projection lens* | trivial |
| Whole 2×2 graph "galaxy" (3.67M points) | **three.js `Points`** + custom shader | GPU point cloud; no layout physics needed (algebraic layout) | about 4M points |
| Network graphs (neighbourhoods, coset graphs up to ~30k nodes / ~150k edges) | **sigma.js v3 + graphology** | WebGL renderer, standard graph data model, ForceAtlas2 in a web worker | ≈ 100k nodes interactive |
| Charts: layer histograms, IDA\* iteration growth, heuristic profiles | **d3** (scales, axes, shapes) into SVG | precise control, small bundle | trivial |
| Search-tree icicle / sunburst | **d3-hierarchy** into Canvas | tens of thousands of aggregated tree cells | ≈ 50k cells |

Rejected: **react-three-fiber** (convenient, but the move-animation queue and instanced sticker updates are simpler imperatively; it can be adopted later without changing the engine). **Cytoscape.js** (Canvas renderer slows past ~10k elements). **d3-force + SVG** for big graphs (fine below ~2k nodes, so it's used only for tiny insets). **cosmos.gl** (GPU force layout for millions of nodes, noted as a future option if we ever want a force layout of a 10⁶-vertex coset graph).

### 3.2 Worker model

- One **solver worker** holds the full wasm instance plus loaded tables. It is created via Vite's worker import and wrapped with **Comlink** for typed RPC.
- The **main thread** instantiates the same wasm module *without tables* for instant, synchronous state operations: apply move, parse notation, validate, render facelets.
- Optional **extra workers** run competing solvers side by side on the comparison dashboard.

---

## 4. Browser requirements

WebAssembly (with bulk memory), WebGL2, module Web Workers and IndexedDB. Any current Chrome, Edge, Firefox or Safari (17+) qualifies. No SharedArrayBuffer, so no special server headers are required.

---

## 5. Environment on this machine

Detected on 2026-09-19:

| Tool | Status |
|---|---|
| Windows 11 Home (x64), 22 logical CPUs, 23 GB RAM | ✔ plenty for native table generation (largest planned native table set ≈ 1 GB) |
| Rust 1.96.0, `stable-x86_64-pc-windows-msvc`, rustup 1.29 | ✔ |
| `wasm32-unknown-unknown` target | ✘ not installed |
| `wasm-bindgen-cli` | ✘ not installed |
| Node.js 24.18.0, npm 11.16, pnpm 11.22 | ✔ |
| git 2.54 | ✔ (project folder is not yet a git repo) |
| Python 3.10 | ✔ not required; only useful for optional analysis notebooks |

### 5.1 One-time setup (planned M0 steps)

Add the WebAssembly compile target:

```bash
rustup target add wasm32-unknown-unknown
```

Install the wasm-bindgen CLI. The version must equal the `wasm-bindgen` crate version in `Cargo.lock`, and `cargo xtask doctor` will check and print the exact command:

```bash
cargo install wasm-bindgen-cli --version <version-from-Cargo.lock>
```

Optional, for smaller and faster release wasm:

```bash
cargo install wasm-opt
```

Install web dependencies (run in `web/`):

```bash
pnpm install
```

### 5.2 Everyday commands (planned)

| Command | Does |
|---|---|
| `cargo xtask doctor` | Verifies toolchain, target, wasm-bindgen version match, Node/pnpm |
| `cargo xtask wasm [--watch]` | Builds `rg-wasm` in release, runs wasm-bindgen into `web/src/engine/pkg/` |
| `cargo xtask tables [--size 3] [--set browser\|native\|all]` | Runs `rgraph gen-tables` natively into `web/public/tables/` |
| `cargo xtask dev` | `wasm --watch` + `pnpm dev` together |
| `cargo xtask ci` | fmt check, clippy, tests, wasm build, web lint/test/build |
| `rgraph solve --size 3 --algo kociemba "R U R' U' ..."` | Native solve from the terminal |
| `rgraph phase-lab --size 4 --phase 2` | Phase design report ([08 §4](08-solving-4x4-5x5.md#4-the-phase-lab)) |
| `rgraph bench --size 3 --algo all --samples 1000` | Benchmark suite |

The xtask pattern keeps build scripting in Rust, so the same commands work in PowerShell, Git Bash, and CI.

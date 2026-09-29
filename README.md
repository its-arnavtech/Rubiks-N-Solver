# rubiks-graph — NxN neural cube solver

> One small, self-trained neural network that solves Rubik's cubes of **any size**: 2×2, 10×10, 100×100, 400×400. There's a local web page to watch it work.

**How it works (short):** every NxN cube is made of small groups of pieces called **orbits** that only ever swap among themselves. There are only 7 kinds, and each is a 24-slot puzzle that looks the same on every cube size. The system:
1. discovers its own move tricks (macros) that shuffle 3 pieces of one orbit and leave everything else untouched
2. trains one network, by self-play, to solve any orbit with those tricks
3. solves all orbits of a cube in parallel on the GPU
4. replays the whole solution on an exact simulator to prove the cube is solved

Bigger cubes just mean more orbits.

**Status:** simulator, orbits, macro library, baseline solver, Python bridge, training pipeline, neural solver pipeline, local server and web UI are built and tested. The full training run (M6.6) is next. See [CHANGELOG.md](CHANGELOG.md) and [docs/nn/RESULTS.md](docs/nn/RESULTS.md).

## See it run

One-time setup (Windows; see [docs/nn/SETUP.md](docs/nn/SETUP.md)): Rust, `just`, `uv`, `pnpm`, and the `wasm32-unknown-unknown` target with `wasm-bindgen-cli`.

```
just setup          # Python env, web deps, native nxsim module
just serve          # API on 127.0.0.1:8000 + web UI on http://localhost:5173
```

Open http://localhost:5173, pick N (2–100), press **Scramble**, then **Solve**. The server returns the moves and which orbit/round/action produced each one; the browser replays them exactly with the Rust engine compiled to WebAssembly.

- **Neural** uses the checkpoint named in `artifacts/checkpoints/CURRENT` (trained with `just train default`). Without one, only **baseline** is offered.
- **Orbit overlay** dims unsolved orbits and lights the orbit being worked on; **by type** colors stickers by orbit kind.
- Play **per move** (animated turns up to 10×10), **per action**, or **per round** (all orbits advance together); scrub by round.
- Click a sticker to open the **orbit inspector**: its type and indices, its 24 slots, and its action history with Q-values.
- From the command line: `just baseline 50 7` (Rust baseline) or `just solve 100 1 nn` (neural).

## Documentation
| Doc | Contents |
|---|---|
| [docs/nn/ARCHITECTURE.md](docs/nn/ARCHITECTURE.md) | What we're building, every component, training, inference, UI |
| [docs/nn/CONVENTIONS.md](docs/nn/CONVENTIONS.md) | Exact formats: facelets, moves, orbit slots, library.json, HTTP API |
| [docs/nn/MILESTONES.md](docs/nn/MILESTONES.md) | Ordered task list with acceptance checks |
| [docs/nn/DECISIONS.md](docs/nn/DECISIONS.md) | Why each design choice was made |
| [docs/nn/SETUP.md](docs/nn/SETUP.md) | Windows (+ optional WSL2) setup |
| [AGENTS.md](AGENTS.md) | Rules and handoff protocol for Claude Code / Codex |
| [docs/legacy/](docs/legacy/) | The earlier graph-theory version of this project (superseded) |

## Stack
Rust (simulator, orbits, macro discovery, baseline solver, verification), PyO3/maturin bridge, Python 3.12 + PyTorch (CUDA) for the network, FastAPI (local server), and the React + three.js web app with the simulator compiled to WebAssembly.

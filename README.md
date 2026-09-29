# Rubiks N Solver — NxN neural cube solver

> One small, self-trained neural network that solves Rubik's cubes of **any size**: 2×2, 10×10, 100×100, 400×400. There's a local web page to watch it work.

**How it works (short):** every NxN cube is made of small groups of pieces called **orbits** that only ever swap among themselves. There are only 7 kinds, and each is a 24-slot puzzle that looks the same on every cube size. The system:
1. discovers its own move tricks (macros) that shuffle 3 pieces of one orbit and leave everything else untouched
2. trains one network, by self-play, to solve any orbit with those tricks
3. solves all orbits of a cube in parallel on the GPU
4. replays the whole solution on an exact simulator to prove the cube is solved

Bigger cubes just mean more orbits.

**Status:** working end to end. One trained network (130k steps, ~8 h on an RTX 4060 laptop) solves every orbit type; on random cubes from 2×2 to 100×100 the neural solver needs 0.44–0.81× the moves of the deterministic baseline, never needed the baseline fallback in the benchmark, and solves a 100×100 in about 1.6 s. Every solution is replayed and verified. Numbers: [docs/nn/RESULTS.md](docs/nn/RESULTS.md). History: [CHANGELOG.md](CHANGELOG.md).

## Trained network

**Download:** [**weights-v1 release**](https://github.com/its-arnavtech/Rubiks-N-Solver/releases/tag/weights-v1): `rubiks-n-solver-weights-step130000.pt` (35.4 MiB, sha256 `280f22d3d87b7cad3749c9a5da25ce5d2cd2be9e306298c26ea65636ea68b392`). It works with the macro library in this repo (sha256 `88e142fb…`), and the loader refuses any other.

To use it, from the repo root (needs the [GitHub CLI](https://cli.github.com/); or download it from the release page):

```
gh release download weights-v1 -R its-arnavtech/Rubiks-N-Solver -D artifacts/checkpoints
echo artifacts/checkpoints/rubiks-n-solver-weights-step130000.pt > artifacts/checkpoints/CURRENT
```

`just serve` and `just solve N SEED nn` then use it. Without weights, only the baseline solver is available; to train your own instead, run `just train default` (about 8 h on a laptop GPU). The weights are covered by the [LICENSE](LICENSE) (all rights reserved).

## See it run

One-time setup (Windows; see [docs/nn/SETUP.md](docs/nn/SETUP.md)): Rust, `just`, `uv`, `pnpm`, and the `wasm32-unknown-unknown` target with `wasm-bindgen-cli`.

```
just setup          # Python env, web deps, native nxsim module
just serve          # API on 127.0.0.1:8000 + web UI on http://localhost:5173
```

Open http://localhost:5173, pick N (2–100), press **Scramble**, then **Solve**. The server returns the moves and which orbit/round/action produced each one; the browser replays them exactly with the Rust engine compiled to WebAssembly.

- **Neural** uses the checkpoint named in `artifacts/checkpoints/CURRENT` (see [Trained network](#trained-network) for the released weights). Without one, only **baseline** is offered.
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

## License

Copyright (c) 2026 its-arnavtech. **All rights reserved.** The code, documentation, macro library, the neural network (architecture, training code, and all trained weights and checkpoints) and all other data and results of this project belong to the copyright holder. The repository is public to read, but no license to use, copy, modify, redistribute, or build on it is granted without written permission. See [LICENSE](LICENSE). Third-party dependencies keep their own licenses.

# CHANGELOG

Handoff log for all agents. See AGENTS.md §4 for how to update it.
- **"Current state"** is overwritten every session.
- **"Log"** is append-only, newest first.

---

## Current state

- **Project:** NxN neural cube solver (see `docs/nn/ARCHITECTURE.md`)
- **Active milestone:** M0 — Repo preparation
- **Last completed task:** M0.5 — `justfile` and `.gitignore` updates; `just check` is green
- **In progress:** none
- **Next task:** **M0.6** (CI workflow), then M0.7 (USER)
- **Blockers:** none
- **Needs user:** M0.7. Try WSL2 per `docs/nn/SETUP.md` §2, time-boxed; otherwise use native Windows. Report which one.
- **How to verify:** `just check` (needs `just`: `winget install Casey.Just` or `cargo install just`). Also: `cargo build --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`, `cargo build -p nx-wasm --target wasm32-unknown-unknown` all pass. Plain `cargo build` skips `nx-py`. Python: `cd python; uv sync --no-install-package torch; uv run --no-sync pytest` (1 test; full `uv sync` downloads the ~2.5 GB CUDA torch wheel).
- **Last updated:** 2026-09-28 by Claude Code (Sonnet 5.5)

---

## Log

### Entry template
```
### YYYY-MM-DD — <agent name/model> — <task ids>
**Done:** what was completed (with task ids)
**Files:** main files added/changed
**Tests:** what was run and the result (e.g. `just check` green; 1000/1000 verified at N=2..20)
**Decisions:** ADRs added/changed, conventions changed (and library_version bump if any)
**Problems / open questions:** anything unresolved
**Next:** the exact next step
```

### 2026-09-28 — Claude Code (Sonnet 5.5) — M0.5
**Done:** M0.5. `justfile` with `windows-shell` set to PowerShell and all 14 recipes: setup, check, test, fmt, discover, verify-library, baseline, py-build, train, eval, solve, serve, web, bench (plus `pytest` and a default list). Recipes for commands that don't exist yet (discover, verify-library, baseline, train, eval, solve, serve, bench) are wired to their planned CLIs and fail until those milestones land. Python recipes use `[working-directory: 'python']` so config paths like `../artifacts/...` resolve. `.gitignore` gained `artifacts/checkpoints/`, `runs/`, `python/.venv*`, `target-wsl/`, `__pycache__/`, `*.pyc`, `.pytest_cache/`, `.hypothesis/`. Removed two `.pyc` files that M0.4 committed by mistake.
**Files:** `justfile`, `.gitignore`.
**Tests:** installed `just` 1.58.0 via `cargo install just --locked` (user-level, in `~/.cargo/bin`; not on the Git Bash PATH by default). `just check` passes end to end (fmt, clippy -D warnings, cargo test, pytest -m "not gpu", biome). `just py-build` also works: maturin builds `nx-py` and `import nxsim; nxsim.version()` works in `python/.venv`, which was a bonus ahead of M5.1.
**Decisions:** none.
**Problems / open questions:** `just serve` only starts the API server for now; M8.7 must add starting the web dev server too (cross-platform, in Python or xtask). `cargo test` also runs the legacy `rg-*` tests, which are slow (~30 s for `rg-solve`).
**Next:** M0.6.

### 2026-09-28 — Claude Code (Sonnet 5.5) — M0.4
**Done:** M0.4. `python/pyproject.toml` (hatchling build, package `nxnn`, `requires-python >=3.12,<3.13`, all deps from the milestone; `maturin` in the `dev` group with pytest and hypothesis). Torch is pinned to the cu126 index via `[tool.uv.sources]`. `uv lock` resolved torch 2.14.0+cu126. Added `configs/smoke.yaml` (CPU, tiny model, 200 steps) and `configs/default.yaml` (ARCHITECTURE §8–9 sizes) plus a `gpu` pytest marker and one import test.
**Files:** `python/pyproject.toml`, `python/uv.lock`, `python/nxnn/__init__.py`, `python/tests/test_package.py`, `python/configs/*.yaml`.
**Tests:** `uv sync --no-install-package torch` then `pytest` → 1 passed. Both YAML files load. The full torch install and CUDA check were NOT done (M0.7, user's machine setup).
**Decisions:** none. The config key layout (model/train/curriculum/eval/checkpoint) is a first draft; M6.1's pydantic schema is the authority and may rename keys.
**Problems / open questions:** the cu126 index is applied on every platform, as SETUP.md says. Linux CI will download the CUDA wheel unless M0.6 overrides it (M0.6 decides).
**Next:** M0.5.

### 2026-09-28 — Claude Code (Sonnet 5.5) — M0.3
**Done:** M0.3. Added `nx-sim`, `nx-macro`, `nx-solve`, `nx-cli` (binary `nx`), `nx-wasm`, `nx-py` (PyO3 module `nxsim` with a `version()` function). Root `Cargo.toml` has `default-members` without `nx-py`. `nx-py` has its own `[lints]` (unsafe allowed). Added workspace deps `nx-sim`, `nx-macro`, `nx-solve`, `pyo3 = "0.29"`. `extension-module` is not enabled in Cargo.toml; maturin will enable it in M0.4/M5.1.
**Files:** `Cargo.toml`, `crates/nx-*/`; `crates/rg-solve/src/nxn5.rs` reformatted by `cargo fmt` (whitespace only, needed for the fmt gate).
**Tests:** `cargo build --workspace` OK; `cargo clippy --workspace --all-targets -- -D warnings` OK; `cargo fmt --check` OK; `nx-wasm` builds for `wasm32-unknown-unknown`; `cargo run -p nx-cli` prints the skeleton banner.
**Decisions:** none.
**Problems / open questions:** `just` is not installed on this machine yet (M0.5 writes the justfile, but `just check` can't be run until `winget install Casey.Just`). Local Python is 3.10; the `python/` project pins 3.12 through uv.
**Next:** M0.4.

### 2026-09-28 — Claude Code (Sonnet 5.5) — M0.1
**Done:** M0.1. `.git/index.lock` was empty and no git process was running, so it was removed. Committed the legacy tree (rg-* crates, web, xtask, docs/legacy) as `cc2bf05`, tagged `legacy-graph-theory`. The new project docs (AGENTS/CLAUDE/CHANGELOG/README, docs/nn) are in the following commit.
**Files:** none changed; git history only.
**Tests:** none (no code changes).
**Decisions:** none.
**Problems / open questions:** `core.autocrlf=true` on this machine prints LF→CRLF warnings on add; harmless.
**Next:** M0.3.

### 2026-09-28 — Claude (Cowork) — M0.2
**Done:** Designed the full architecture with the user. Wrote the project docs.
**Files:**
- added `AGENTS.md`, `CLAUDE.md`, `CHANGELOG.md`
- added `docs/nn/ARCHITECTURE.md`, `CONVENTIONS.md`, `MILESTONES.md`, `DECISIONS.md`, `SETUP.md`
- replaced `README.md`
- moved the old `README.md` and `docs/01..12-*.md` into `docs/legacy/`

**Tests:** none. No code was written.
**Decisions:** ADR-001 … ADR-013 (orbit decomposition, fixed-corner frame, nx-sim with rg-cube oracle, searched macros, Q-value iteration, one shared network, correctness by code, training decoupled from Rust, local UI, same repo, library hash pins checkpoints, no hidden info, WSL2 time-boxed).
**Problems / open questions:**
- Repo state as found: no commits, and a stale `.git/index.lock` that could not be removed from the architecture session.
- The existing `web/` app (React 19, Vite, three.js, Zustand, Tailwind, pnpm) and `xtask` (wasm build) will be reused. The legacy UI will be replaced in M8.

**Next:** M0.1, then M0.3–M0.6.

# Task runner. Windows is the primary OS: recipes are single commands that PowerShell can run.
# Tooling logic lives in Python or Rust, never in shell scripts (AGENTS.md §3).
set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# List the recipes.
default:
    @just --list

# One-time install: Python env, web deps, native `nxsim` module.
setup:
    uv sync --project python
    pnpm --dir web install
    just py-build

# The gate: fmt, clippy, cargo test, pytest (no GPU), web check.
check:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test
    just pytest
    pnpm --dir web check
    pnpm --dir web test

# Rust + Python tests (no GPU).
test:
    cargo test
    just pytest

[working-directory: 'python']
pytest:
    uv run --no-sync pytest -m "not gpu"

# Format all Rust code.
fmt:
    cargo fmt --all

# Regenerate artifacts/macros/library.json (M3).
discover:
    cargo run --release -p nx-cli -- discover

# Purity / invariance / coverage checks of the library (M3).
verify-library:
    cargo run --release -p nx-cli -- verify-library

# Verified baseline solve: just baseline 50 7
baseline N SEED:
    cargo run --release -p nx-cli -- solve --baseline --n {{N}} --seed {{SEED}}

# Build the `nxsim` Python module into python/.venv with maturin (M5.1).
py-build:
    uv run --project python maturin develop --release -m crates/nx-py/Cargo.toml

# Train: just train default  (uses python/configs/<CONFIG>.yaml)
[working-directory: 'python']
train CONFIG:
    uv run python -m nxnn.train --config configs/{{CONFIG}}.yaml

# Evaluate a checkpoint: just eval artifacts/checkpoints/<run>/step_<n>.pt
[working-directory: 'python']
eval CKPT:
    uv run python -m nxnn.evaluate --checkpoint {{CKPT}}

# NN or baseline solve: just solve 100 1 nn
[working-directory: 'python']
solve N SEED SOLVER:
    uv run python -m nxnn.solve --n {{N}} --seed {{SEED}} --solver {{SOLVER}}

# API server (127.0.0.1:8000) + web UI (http://localhost:5173). Ctrl+C stops both.
serve:
    cargo xtask serve

# API server only.
[working-directory: 'python']
api:
    uv run --no-sync python -m nxnn.server

# Web UI dev server on localhost:5173.
web:
    pnpm --dir web dev

# Criterion benchmarks (M1.5).
bench:
    cargo bench -p nx-sim

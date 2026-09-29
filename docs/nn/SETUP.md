# Environment Setup

**Goal:** working in minutes, never blocked. Everything runs on **native Windows**. WSL2 is an *optional* speed-up for training only (ADR-013).

## 1. Native Windows (required)

Install these once (PowerShell):

| Tool | How | Check |
|---|---|---|
| Git | `winget install Git.Git` | `git --version` |
| Rust (stable ≥ 1.85) | `winget install Rustlang.Rustup`, then `rustup target add wasm32-unknown-unknown` | `cargo --version` |
| wasm-bindgen-cli | version must match `Cargo.lock`; `cargo xtask doctor` prints the exact command | `cargo xtask doctor` |
| just | `winget install Casey.Just` | `just --version` |
| Node 22+ and pnpm | `winget install OpenJS.NodeJS.LTS`, then `npm i -g pnpm` | `pnpm -v` |
| uv (Python manager) | `winget install astral-sh.uv` | `uv --version` |
| NVIDIA driver (recent Game Ready/Studio) | GeForce Experience or nvidia.com | `nvidia-smi` shows the RTX 4060 |

Then, from the repo root:

```powershell
just setup      # uv sync (python/.venv), pnpm install (web), builds nxsim via maturin
just check      # fmt + clippy + cargo test + pytest (not gpu) + web check
```

**CUDA PyTorch:** `python/pyproject.toml` pins torch to the official CUDA 12.x wheel index through `[tool.uv.sources]`:

```toml
[[tool.uv.index]]
name = "pytorch-cu126"
url = "https://download.pytorch.org/whl/cu126"
explicit = true

[tool.uv.sources]
torch = { index = "pytorch-cu126" }
```

If the driver is too old for that CUDA version, update the driver. Don't downgrade torch.

Verify:
```powershell
uv run --project python python -c "import torch;print(torch.__version__, torch.cuda.is_available(), torch.cuda.get_device_name(0))"
```
It must print `True` and the RTX 4060.

## 2. WSL2 for training (optional, time-boxed to ~30 min)

**Stop and use native Windows** if any of these happen:
- `wsl --install` asks for BIOS virtualization changes you don't want to make
- the install errors
- `nvidia-smi` fails inside WSL

Record the outcome in CHANGELOG.md.

1. Admin PowerShell: `wsl --status`. If WSL isn't installed: `wsl --install -d Ubuntu-24.04`, then reboot.
2. In Ubuntu:
   - `nvidia-smi` must show the GPU. The driver comes from Windows; **do not** install a Linux NVIDIA driver.
   - Install uv: `curl -LsSf https://astral.sh/uv/install.sh | sh`
3. Use the repo from Windows' drive. Training reads almost no files, so `/mnt/c` speed doesn't matter:
   ```bash
   cd /mnt/c/rubiks-graph/python
   export UV_PROJECT_ENVIRONMENT=.venv-wsl     # keep separate from Windows' .venv
   uv sync
   uv run python -m nxnn.train --config configs/default.yaml
   ```
   Training imports **only** torch and `library.json`, so no Rust build is needed in WSL (ADR-008).
4. Checkpoints land in `C:\rubiks-graph\artifacts\checkpoints\`. Serve and solve from **Windows**, where `nxsim` is built.

If you ever build Rust inside WSL, set `CARGO_TARGET_DIR=target-wsl` so it doesn't clobber the Windows `target/`.

## 3. Everyday commands

| Command | What it does |
|---|---|
| `just check` | everything CI runs |
| `just discover` / `just verify-library` | regenerate / verify `artifacts/macros/library.json` |
| `just baseline 50 7` | baseline solve of N=50, seed 7, verified |
| `just train default` | train with `python/configs/default.yaml` (TensorBoard: `uv run --project python tensorboard --logdir runs`) |
| `just eval <checkpoint>` | per-type evaluation report |
| `just solve 100 1 nn` | NN solve of N=100, seed 1 |
| `just serve` | local server on `127.0.0.1:8000` + web on `localhost:5173` |

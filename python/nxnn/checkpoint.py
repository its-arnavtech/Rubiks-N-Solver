"""Checkpoints (ARCHITECTURE §9, CONVENTIONS §9, ADR-011): weights, optimizer, config,
library sha256, git commit, curriculum and eval summary. Loading with a different library hash
is an error."""

from __future__ import annotations

import subprocess
from pathlib import Path
from typing import Any

import torch

from .config import Config, ModelConfig
from .library import REPO_ROOT, Library, LibraryError
from .model import QNet

CHECKPOINT_DIR = REPO_ROOT / "artifacts" / "checkpoints"
CURRENT = CHECKPOINT_DIR / "CURRENT"


def git_commit() -> str:
    try:
        out = subprocess.run(
            ["git", "rev-parse", "HEAD"], cwd=REPO_ROOT, capture_output=True, text=True, check=True
        )
        return out.stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def save_checkpoint(path: Path, state: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(".tmp")
    torch.save(state, tmp)
    tmp.replace(path)


def load_checkpoint(path: str | Path, lib: Library, device: str | torch.device = "cpu"):
    """Returns `(model, checkpoint_dict)`. Raises `LibraryError` if the checkpoint was trained
    against another library."""
    ckpt = torch.load(path, map_location=device, weights_only=False)
    if ckpt.get("library_sha256") != lib.sha256:
        raise LibraryError(
            f"checkpoint {path} was trained with library {ckpt.get('library_sha256')}, "
            f"but the current library is {lib.sha256}"
        )
    cfg = Config.model_validate(ckpt["config"])
    model = QNet(ModelConfig.model_validate(cfg.model.model_dump()), lib).to(device)
    model.load_state_dict(ckpt["model"])
    model.eval()
    return model, ckpt


def current_checkpoint() -> Path | None:
    """The checkpoint named by `artifacts/checkpoints/CURRENT` (a path relative to the repo)."""
    if not CURRENT.exists():
        return None
    rel = CURRENT.read_text(encoding="utf-8").strip()
    if not rel:
        return None
    p = Path(rel)
    return p if p.is_absolute() else REPO_ROOT / p

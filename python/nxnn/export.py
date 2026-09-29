"""Export a weights-only checkpoint for distribution (e.g. a GitHub release).

`python -m nxnn.export <checkpoint.pt> <out.pt>` keeps what inference needs (model weights,
config, library sha256, git commit, step, eval summary) and drops the optimizer and scheduler.
The result loads with `nxnn.checkpoint.load_checkpoint` like any checkpoint. Copyright and
license: see LICENSE (all rights reserved).
"""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path

import torch

KEEP = ("model", "config", "library_sha256", "git_commit", "step", "run_id", "eval")


def export(src: Path, dst: Path) -> str:
    ckpt = torch.load(src, map_location="cpu", weights_only=False)
    out = {k: ckpt[k] for k in KEEP if k in ckpt}
    out["license"] = "Copyright (c) 2026 its-arnavtech. All rights reserved. See LICENSE."
    dst.parent.mkdir(parents=True, exist_ok=True)
    torch.save(out, dst)
    return hashlib.sha256(dst.read_bytes()).hexdigest()


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("src", type=Path)
    p.add_argument("dst", type=Path)
    args = p.parse_args()
    digest = export(args.src, args.dst)
    size = args.dst.stat().st_size / 2**20
    print(f"wrote {args.dst} ({size:.1f} MiB), sha256 {digest}")


if __name__ == "__main__":
    main()

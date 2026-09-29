"""Local HTTP API on 127.0.0.1:8000 (CONVENTIONS §8, ARCHITECTURE §12).

`python -m nxnn.server [--port 8000] [--checkpoint PATH] [--max-n 100]`

The server loads the library and, if one is available, the checkpoint named by
`artifacts/checkpoints/CURRENT`. Without a checkpoint only the baseline solver is offered.
Every solve is verified; an unverified result is an HTTP 500, never a success (ADR-007).
"""

from __future__ import annotations

import argparse
import base64
import binascii
import os
from typing import Literal

import nxsim
import torch
from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field

from .library import LibraryError
from .solve import Solver, SolveError, solve

DEFAULT_MAX_N = int(os.environ.get("NXNN_MAX_N", "100"))


class ScrambleRequest(BaseModel):
    n: int
    mode: Literal["random_state", "moves"] = "random_state"
    seed: int = 0
    length: int | None = None


class SolveRequest(BaseModel):
    n: int
    facelets_b64: str
    solver: Literal["nn", "baseline"] = "nn"
    beam: int = Field(1, ge=1, le=64)
    max_steps: int = Field(64, ge=1, le=512)


def _load_solver(checkpoint: str | None, device: str | None) -> tuple[Solver, str | None]:
    """Solver with the model if possible; otherwise baseline-only, with the reason."""
    try:
        return Solver.load(checkpoint, device, need_model=True), None
    except (SolveError, LibraryError, FileNotFoundError) as e:
        return Solver.load(None, device, need_model=False), str(e)


def create_app(solver: Solver | None = None, max_n: int = DEFAULT_MAX_N, checkpoint: str | None = None,
               device: str | None = None) -> FastAPI:
    app = FastAPI(title="nxnn", version="0.1.0")
    app.add_middleware(
        CORSMiddleware,
        allow_origin_regex=r"http://(localhost|127\.0\.0\.1)(:\d+)?",
        allow_methods=["*"],
        allow_headers=["*"],
    )
    note: str | None = None
    if solver is None:
        solver, note = _load_solver(checkpoint, device)

    def check_n(n: int) -> None:
        if not 2 <= n <= max_n:
            raise HTTPException(400, f"n must be in 2..{max_n}")

    @app.get("/api/health")
    def health() -> dict:
        return {
            "status": "ok",
            "checkpoint": solver.checkpoint,
            "library_sha256": solver.lib.sha256,
            "device": str(solver.device),
            "nn_available": solver.model is not None,
            "note": note,
            "max_n": max_n,
        }

    @app.post("/api/scramble")
    def scramble(req: ScrambleRequest) -> dict:
        check_n(req.n)
        if req.mode == "random_state":
            facelets, moves = nxsim.random_state(req.n, req.seed), None
        else:
            length = req.length if req.length is not None else 20 * req.n
            if not 0 <= length <= 100_000:
                raise HTTPException(400, "length must be in 0..100000")
            facelets, moves = nxsim.scramble(req.n, length, req.seed)
        return {
            "n": req.n,
            "facelets_b64": base64.b64encode(facelets).decode(),
            "scramble_moves_b64": base64.b64encode(moves).decode() if moves is not None else None,
        }

    @app.post("/api/solve")
    def solve_endpoint(req: SolveRequest) -> dict:
        check_n(req.n)
        try:
            facelets = base64.b64decode(req.facelets_b64, validate=True)
        except (binascii.Error, ValueError) as e:
            raise HTTPException(400, f"facelets_b64 is not base64: {e}") from e
        try:
            nxsim.validate(req.n, facelets)
        except ValueError as e:
            raise HTTPException(400, f"invalid cube: {e}") from e
        if req.solver == "nn" and solver.model is None:
            raise HTTPException(503, f"no trained model loaded ({note}); use solver=baseline")
        try:
            with torch.no_grad():
                return solve(solver, req.n, facelets, req.solver, req.beam, req.max_steps)
        except SolveError as e:
            raise HTTPException(500, f"solve failed: {e}") from e

    @app.get("/api/orbits/{n}")
    def orbits(n: int) -> dict:
        check_n(n)
        indices = {
            "Wing": lambda a, b: {"p": a},
            "XCenter": lambda a, b: {"a": a},
            "PlusCenter": lambda a, b: {"a": a},
            "ObliqueA": lambda a, b: {"a": a, "b": b},
            "ObliqueB": lambda a, b: {"a": a, "b": b},
        }
        return {
            "orbits": [
                {"id": oid, "type": t, "indices": indices.get(t, lambda a, b: {})(a, b)}
                for oid, t, a, b in nxsim.orbits(n)
            ],
            "sticker_orbit_b64": base64.b64encode(nxsim.sticker_orbit(n)).decode(),
        }

    return app


def main() -> None:
    import uvicorn

    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--host", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8000)
    p.add_argument("--checkpoint", default=None)
    p.add_argument("--device", default=None)
    p.add_argument("--max-n", type=int, default=DEFAULT_MAX_N)
    args = p.parse_args()
    app = create_app(max_n=args.max_n, checkpoint=args.checkpoint, device=args.device)
    uvicorn.run(app, host=args.host, port=args.port)


if __name__ == "__main__":
    main()

"""Training configuration (pydantic). YAML files live in `python/configs/`.

Relative paths in a config resolve against the `python/` directory, whatever the CWD.
"""

from __future__ import annotations

from pathlib import Path

import yaml
from pydantic import BaseModel, ConfigDict, Field, field_validator

from .library import TYPE_NAMES

PYTHON_DIR = Path(__file__).resolve().parents[1]


class _Strict(BaseModel):
    model_config = ConfigDict(extra="forbid")


class ModelConfig(_Strict):
    d_model: int = 256
    n_layers: int = 6
    n_heads: int = 8
    ffn_dim: int = 1024
    action_emb_dim: int = 256
    # Q = cost + q_scale · softplus(...) (ADR-014): targets reach ~100+ moves.
    q_scale: float = 10.0
    # "free": one learned vector per action. "structured": built from the action's slot
    # effect (role-specific slot embeddings + orientation pattern) plus a free residual.
    # Structured learned much slower in a 4k-step comparison (ADR-016), so "free" is default.
    action_embedding: str = "free"


class TrainConfig(_Strict):
    steps: int = 500_000
    batch_size: int = 4096
    actions_per_state: int = 8
    lr: float = 3e-4
    weight_decay: float = 0.01
    warmup_steps: int = 1000
    grad_clip: float = 1.0
    target_sync: int = 2000
    # Sync the target net early once the loss falls below this (DeepCubeA rule); None = off.
    target_sync_loss: float | None = None
    huber_delta: float = 1.0
    # One of the random actions per scrambled state is the inverse of its last scramble
    # action (ADR-015).
    hindsight: bool = True
    # At each curriculum check, type t's sampling weight becomes
    # type_weights[t] · (0.25 + 1 − solve_rate_t): lagging types get up to 5× the share.
    adaptive_weights: bool = True
    bf16: bool = True
    type_weights: dict[str, float] = Field(default_factory=lambda: {t: 1.0 for t in TYPE_NAMES})
    log_every: int = 50

    @field_validator("type_weights")
    @classmethod
    def _known_types(cls, v: dict[str, float]) -> dict[str, float]:
        unknown = set(v) - set(TYPE_NAMES)
        if unknown:
            raise ValueError(f"unknown types {sorted(unknown)}")
        if not any(w > 0 for w in v.values()):
            raise ValueError("some type weight must be positive")
        return v

    @field_validator("actions_per_state")
    @classmethod
    def _even(cls, v: int) -> int:
        if v < 2 or v % 2:
            raise ValueError("actions_per_state must be even and >= 2 (half greedy, half random)")
        return v


class CurriculumConfig(_Strict):
    k_start: int = 2
    k_max: int = 30
    advance_solve_rate: float = 0.95
    # When K_t reaches k_uniform, a fraction p_uniform_after of states are uniform random.
    k_uniform: int = 20
    p_uniform_after: float = 0.5
    check_every: int = 500
    check_states: int = 512


class EvalConfig(_Strict):
    every: int = 5000
    states_per_type: int = 4096
    step_cap: int = 64
    beam_width: int = 8


class CheckpointConfig(_Strict):
    dir: Path = Path("../artifacts/checkpoints")
    every: int = 5000
    tensorboard_dir: Path = Path("../runs")


class Config(_Strict):
    run_name: str
    seed: int = 0
    device: str = "cuda"  # cpu | cuda
    library: Path = Path("../artifacts/macros/library.json")
    model: ModelConfig = Field(default_factory=ModelConfig)
    train: TrainConfig = Field(default_factory=TrainConfig)
    curriculum: CurriculumConfig = Field(default_factory=CurriculumConfig)
    eval: EvalConfig = Field(default_factory=EvalConfig)
    checkpoint: CheckpointConfig = Field(default_factory=CheckpointConfig)

    def resolve(self, p: Path) -> Path:
        return p if p.is_absolute() else (PYTHON_DIR / p).resolve()


def load_config(path: str | Path) -> Config:
    with open(path, encoding="utf-8") as f:
        return Config.model_validate(yaml.safe_load(f))

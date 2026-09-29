"""M6.1: config schema."""

from pathlib import Path

import pytest
from pydantic import ValidationError

from nxnn.config import PYTHON_DIR, Config, load_config

CONFIGS = PYTHON_DIR / "configs"


@pytest.mark.parametrize("name", ["smoke", "default"])
def test_configs_load(name: str) -> None:
    cfg = load_config(CONFIGS / f"{name}.yaml")
    assert cfg.run_name == name
    assert cfg.resolve(cfg.library).exists()
    assert cfg.resolve(cfg.checkpoint.dir).name == "checkpoints"


def test_typos_and_bad_values_are_rejected() -> None:
    with pytest.raises(ValidationError):
        Config.model_validate({"run_name": "x", "trian": {}})
    with pytest.raises(ValidationError):
        Config.model_validate({"run_name": "x", "train": {"type_weights": {"Edge": 1}}})
    with pytest.raises(ValidationError):
        Config.model_validate({"run_name": "x", "train": {"actions_per_state": 3}})


def test_absolute_paths_stay() -> None:
    cfg = Config(run_name="x")
    p = Path(PYTHON_DIR.anchor) / "tmp" / "lib.json"
    assert cfg.resolve(p) == p

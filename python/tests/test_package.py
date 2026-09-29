import re
from pathlib import Path

import nxnn

PKG = Path(nxnn.__file__).parent

# ADR-008: training runs without the Rust module.
NO_NXSIM = ["library", "envs", "model", "train", "config", "evaluate", "baseline"]


def test_package_imports() -> None:
    assert nxnn.__version__


def test_training_modules_do_not_import_nxsim() -> None:
    for name in NO_NXSIM:
        path = PKG / f"{name}.py"
        if path.exists():
            text = path.read_text(encoding="utf-8")
            assert not re.search(r"^\s*(import|from)\s+nxsim", text, re.M), name

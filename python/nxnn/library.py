"""Load `artifacts/macros/library.json` and check its sha256 (CONVENTIONS §7, ADR-011).

Pure Python: training never imports `nxsim` (ADR-008).
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import numpy as np

LIBRARY_VERSION = 1

REPO_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_LIBRARY = REPO_ROOT / "artifacts" / "macros" / "library.json"

# Network type ids (CONVENTIONS §3).
TYPE_NAMES = ["Corner", "MidEdge", "Wing", "XCenter", "PlusCenter", "ObliqueA", "ObliqueB"]
TYPE_IDS = {name: i for i, name in enumerate(TYPE_NAMES)}


class LibraryError(ValueError):
    pass


@dataclass(frozen=True)
class TypeLibrary:
    name: str
    type_id: int
    content: str  # "piece" or "color"
    orientation_mod: int
    slots: int
    layer_refs: list[str]
    perm: np.ndarray  # [A, slots] int64, gather: new[i] = old[perm[i]]
    ori_delta: np.ndarray  # [A, slots] int64
    cost: np.ndarray  # [A] int64
    kinds: list[str]  # per action: three_cycle | twist_pair | flip_pair (of its macro)

    @property
    def num_actions(self) -> int:
        return int(self.perm.shape[0])

    @property
    def is_color(self) -> bool:
        return self.content == "color"

    def solved(self) -> np.ndarray:
        """Solved contents: piece i (orientation 0) in slot i, or the slot's face color."""
        i = np.arange(self.slots, dtype=np.int64)
        return i // 4 if self.is_color else i * self.orientation_mod


@dataclass(frozen=True)
class Library:
    version: int
    sha256: str
    types: dict[str, TypeLibrary]

    def by_id(self, type_id: int) -> TypeLibrary:
        return self.types[TYPE_NAMES[type_id]]


def action_roles(t: TypeLibrary) -> tuple[np.ndarray, np.ndarray]:
    """Structure of each action's slot effect: `slots[A, 3]` and `code[A]`.

    3-cycle: `slots = [a, b, c]` (the piece at a goes to b, b to c, c to a), `code` = the
    orientation deltas at a, b, c as base-3 digits. Orientation pair: `slots = [a, b, -1]`,
    `code = 27 + delta at a`.
    """
    a_n = t.num_actions
    slots = np.full((a_n, 3), -1, dtype=np.int64)
    code = np.zeros(a_n, dtype=np.int64)
    for k in range(a_n):
        perm, ori = t.perm[k], t.ori_delta[k]
        moved = [i for i in range(t.slots) if perm[i] != i]
        if moved:
            a = moved[0]
            b = int(np.flatnonzero(perm == a)[0])
            c = int(np.flatnonzero(perm == b)[0])
            slots[k] = [a, b, c]
            code[k] = 9 * ori[a] + 3 * ori[b] + ori[c]
        else:
            turned = [i for i in range(t.slots) if ori[i] != 0]
            slots[k, :2] = turned[:2]
            code[k] = 27 + ori[turned[0]]
    return slots, code


def canonical_json(obj: Any) -> str:
    """Canonical JSON as written by Rust: keys sorted, no whitespace, UTF-8."""
    return json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def content_hash(raw: dict[str, Any]) -> str:
    """sha256 of the canonical JSON without `sha256` and `generator.git_commit`."""
    obj = {k: v for k, v in raw.items() if k != "sha256"}
    gen = dict(obj.get("generator", {}))
    gen.pop("git_commit", None)
    obj["generator"] = gen
    return hashlib.sha256(canonical_json(obj).encode("utf-8")).hexdigest()


def load_library(path: str | Path | None = None) -> Library:
    """Load and check the library. Raises `LibraryError` on a version or hash mismatch."""
    p = Path(path) if path is not None else DEFAULT_LIBRARY
    raw = json.loads(p.read_text(encoding="utf-8"))
    version = raw.get("library_version")
    if version != LIBRARY_VERSION:
        raise LibraryError(f"library_version {version}, expected {LIBRARY_VERSION}")
    want = content_hash(raw)
    if raw.get("sha256") != want:
        raise LibraryError(f"sha256 mismatch: file says {raw.get('sha256')}, content is {want}")
    types: dict[str, TypeLibrary] = {}
    for name, t in raw["types"].items():
        if TYPE_IDS.get(name) != t["type_id"]:
            raise LibraryError(f"type {name} has id {t['type_id']}")
        macro_kind = [m["kind"] for m in t["macros"]]
        actions = t["actions"]
        types[name] = TypeLibrary(
            name=name,
            type_id=t["type_id"],
            content=t["content"],
            orientation_mod=t["orientation_mod"],
            slots=t["slots"],
            layer_refs=list(t["layer_refs"]),
            perm=np.array([a["perm"] for a in actions], dtype=np.int64),
            ori_delta=np.array([a["ori_delta"] for a in actions], dtype=np.int64),
            cost=np.array([a["cost"] for a in actions], dtype=np.int64),
            kinds=[macro_kind[a["macro"]] for a in actions],
        )
    missing = set(TYPE_NAMES) - set(types)
    if missing:
        raise LibraryError(f"library lacks types {sorted(missing)}")
    return Library(version=version, sha256=raw["sha256"], types=types)

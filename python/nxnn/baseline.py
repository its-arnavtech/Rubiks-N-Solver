"""Orbit-level baseline (ARCHITECTURE §7), batched in NumPy for evaluation metrics.

Mirrors `nx-solve/src/orbit_solver.rs` choice for choice, so its plans (and costs) equal the
Rust fallback's: cycle-sort with the cheapest action per directed 3-cycle (split as
`(x→y→w)(w→z→x)` when missing), then orientation pairs; color types first get a target
assignment. No `nxsim` (ADR-008).
"""

from __future__ import annotations

import numpy as np

from .library import TypeLibrary

FIXED_CORNER_SLOT = 6


def _classify(perm: np.ndarray, ori: np.ndarray):
    """('cycle', [a, b, c]) with the piece at a moving to b, ('pair', [a, b]), or None."""
    s = len(perm)
    moved = [i for i in range(s) if perm[i] != i]
    turned = [i for i in range(s) if perm[i] == i and ori[i] != 0]
    if len(moved) == 3 and not turned:
        a = moved[0]
        b = int(np.flatnonzero(perm == a)[0])
        c = int(np.flatnonzero(perm == b)[0])
        return ("cycle", [a, b, c]) if perm[a] == c else None
    if not moved and len(turned) == 2:
        return ("pair", turned)
    return None


def assign_targets(colors: list[int]) -> list[int]:
    """Target slot per piece of a color orbit (mirrors Rust `assign_targets`)."""
    n = len(colors)
    target = [-1] * n
    free: list[list[int]] = [[] for _ in range(6)]
    for i in range(n):
        if colors[i] == i // 4:
            target[i] = i
        else:
            free[i // 4].append(i)
    for i in range(n):
        if target[i] == -1:
            c = colors[i]
            pick = next((k for k, t in enumerate(free[c]) if colors[t] == i // 4), 0)
            target[i] = free[c].pop(pick)
    if _parity(target):
        best = None
        for i in range(n):
            for j in range(i + 1, n):
                if colors[i] == colors[j]:
                    score = int(target[i] != i) + int(target[j] != j)
                    if best is None or score > best[0]:
                        best = (score, i, j)
        _, i, j = best
        target[i], target[j] = target[j], target[i]
    return target


def _parity(p: list[int]) -> bool:
    seen = [False] * len(p)
    odd = False
    for s in range(len(p)):
        if seen[s]:
            continue
        length, i = 0, s
        while not seen[i]:
            seen[i] = True
            i = p[i]
            length += 1
        odd ^= length % 2 == 0
    return odd


class Baseline:
    """Baseline for one orbit type."""

    def __init__(self, t: TypeLibrary) -> None:
        self.name = t.name
        self.s = t.slots
        self.m = t.orientation_mod
        self.is_color = t.is_color
        self.perm = t.perm
        self.ori = t.ori_delta
        self.cost = t.cost
        s, m = self.s, self.m
        cycle = np.full((s, s, s), -1, dtype=np.int64)
        pair = np.full((s, s, max(m, 1)), -1, dtype=np.int64)
        for idx in range(t.num_actions):
            kind = _classify(self.perm[idx], self.ori[idx])
            if kind is None:
                raise ValueError(f"{t.name} action {idx} is not a 3-cycle or pair")
            if kind[0] == "cycle":
                a, b, c = kind[1]
                old = cycle[a, b, c]
                if old < 0 or self.cost[idx] < self.cost[old]:
                    for x, y, z in ((a, b, c), (b, c, a), (c, a, b)):
                        cycle[x, y, z] = idx
            else:
                a, b = kind[1]
                for x, y in ((a, b), (b, a)):
                    key = (x, y, self.ori[idx][x])
                    old = pair[key]
                    if old < 0 or self.cost[idx] < self.cost[old]:
                        pair[key] = idx
        self.pair = pair
        # first/second action per directed 3-cycle, using the decomposition when missing.
        first = cycle.copy()
        second = np.full_like(cycle, -1)
        fixed = FIXED_CORNER_SLOT if t.name == "Corner" else -1
        for x in range(s):
            for y in range(s):
                for z in range(s):
                    if len({x, y, z}) < 3 or cycle[x, y, z] >= 0:
                        continue
                    for w in range(s):
                        if w in (x, y, z) or w == fixed:
                            continue
                        if cycle[x, y, w] >= 0 and cycle[w, z, x] >= 0:
                            first[x, y, z], second[x, y, z] = cycle[x, y, w], cycle[w, z, x]
                            break
        self.first, self.second = first, second
        # Orientation fallback partner: first slot != i that is not the fixed corner.
        self.fallback = np.array(
            [next(j for j in range(s) if j != i and j != fixed) for i in range(s)]
        )

    def _apply(self, states: np.ndarray, actions: np.ndarray, live: np.ndarray) -> np.ndarray:
        a = np.where(live, actions, 0)
        moved = np.take_along_axis(states, self.perm[a], axis=1)
        if self.m > 1:
            moved = moved // self.m * self.m + (moved % self.m + self.ori[a]) % self.m
        return np.where(live[:, None], moved, states)

    def solve(self, contents: np.ndarray) -> tuple[list[list[int]], np.ndarray]:
        """Plans (action ids) and total costs for a batch `[B, S]` of contents."""
        states = np.asarray(contents, dtype=np.int64).copy()
        b, s, m = states.shape[0], self.s, self.m
        if self.is_color:
            states = np.array([assign_targets(row.tolist()) for row in states], dtype=np.int64)
        steps: list[np.ndarray] = []
        idx = np.arange(s)
        rows = np.arange(b)
        for _ in range(4 * s + 8):
            piece = states // m
            misplaced = piece != idx
            active = misplaced.any(1)
            if not active.any():
                break
            a = misplaced.argmax(1)
            p1 = piece[rows, a]
            p2 = piece[rows, p1]
            other = misplaced & (idx != a[:, None]) & (idx != p1[:, None])
            d = other.argmax(1)
            z = np.where(p2 != a, p2, d)
            act1 = self.first[a, p1, z]
            act2 = self.second[a, p1, z]
            if (active & (act1 < 0)).any():
                raise ValueError(f"{self.name}: no actions for a needed 3-cycle")
            states = self._apply(states, act1, active)
            steps.append(np.where(active, act1, -1))
            live2 = active & (act2 >= 0)
            if live2.any():
                states = self._apply(states, act2, live2)
                steps.append(np.where(live2, act2, -1))
        if m > 1:
            for i in range(s):
                o = states[:, i] % m
                need = o != 0
                if not need.any():
                    continue
                later = (states % m != 0) & (idx > i)
                j = np.where(later.any(1), later.argmax(1), self.fallback[i])
                act = self.pair[i, j, (m - o) % m]
                if (need & (act < 0)).any():
                    raise ValueError(f"{self.name}: missing orientation pair")
                states = self._apply(states, act, need)
                steps.append(np.where(need, act, -1))
        solved = (states == idx * m).all(1)
        if not solved.all():
            raise ValueError(f"{self.name}: baseline did not solve {int((~solved).sum())} states")
        table = np.stack(steps, 1) if steps else np.zeros((b, 0), dtype=np.int64)
        plans = [[int(x) for x in row if x >= 0] for row in table]
        costs = np.where(table >= 0, self.cost[np.maximum(table, 0)], 0).sum(1)
        return plans, costs

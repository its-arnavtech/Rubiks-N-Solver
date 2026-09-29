"""Orbit puzzles as tensors (ARCHITECTURE §9): per type, the library's actions become
`perm[A, S]`, `ori_delta[A, S]` and `cost[A]`; applying an action is one gather plus an add.

Contents follow CONVENTIONS §4: `piece·m + orientation` for Corner / MidEdge (m = 3 / 2), the
piece id (home slot) for Wing, the color for center types. Pure PyTorch: no `nxsim` (ADR-008).
"""

from __future__ import annotations

import torch

from .library import TYPE_NAMES, Library, TypeLibrary

# The corner orbit's fixed slot (DBL, CONVENTIONS §4).
FIXED_CORNER_SLOT = 6


class OrbitEnv:
    """One orbit type on one device."""

    def __init__(self, t: TypeLibrary, device: torch.device | str = "cpu") -> None:
        self.name = t.name
        self.type_id = t.type_id
        self.slots = t.slots
        self.m = t.orientation_mod
        self.is_color = t.is_color
        self.num_actions = t.num_actions
        self.device = torch.device(device)
        self.perm = torch.as_tensor(t.perm, device=self.device)
        self.ori_delta = torch.as_tensor(t.ori_delta, device=self.device)
        self.cost = torch.as_tensor(t.cost, device=self.device, dtype=torch.float32)
        self.solved_state = torch.as_tensor(t.solved(), device=self.device)
        # Content vocabulary size (for embeddings): pieces × orientations, or 6 colors.
        self.vocab = 6 if self.is_color else self.slots * self.m

    def solved(self, batch: int) -> torch.Tensor:
        return self.solved_state.expand(batch, -1).clone()

    def apply(self, states: torch.Tensor, actions: torch.Tensor) -> torch.Tensor:
        """`states[B, S]` (int64), `actions[B]` → new states (CONVENTIONS §7 gather rule)."""
        idx = self.perm[actions]
        moved = torch.gather(states, 1, idx)
        if self.m == 1:
            return moved
        piece, ori = moved // self.m, moved % self.m
        return piece * self.m + (ori + self.ori_delta[actions]) % self.m

    def is_solved(self, states: torch.Tensor) -> torch.Tensor:
        return (states == self.solved_state).all(dim=1)

    def scramble(
        self, batch: int, max_k: int | torch.Tensor, generator: torch.Generator | None = None
    ) -> tuple[torch.Tensor, torch.Tensor]:
        """Apply `k ~ U(1, max_k)` uniformly random actions to solved states.

        `max_k` may be a per-element tensor. Returns `(states, k)`.
        """
        dev = self.device
        kmax = torch.as_tensor(max_k, device=dev).long().expand(batch)
        u = torch.rand(batch, generator=generator, device=dev)
        k = torch.minimum(1 + (u * kmax).long(), kmax)
        states = self.solved(batch)
        for step in range(int(kmax.max().item())):
            a = torch.randint(self.num_actions, (batch,), generator=generator, device=dev)
            live = step < k
            states = torch.where(live[:, None], self.apply(states, a), states)
        return states, k

    def random_states(self, batch: int, generator: torch.Generator | None = None) -> torch.Tensor:
        """Uniform random states that the solver can meet for this type (ARCHITECTURE §7):
        even piece permutations (parities are fixed by code first), orientation sums 0, the
        DBL corner home; any 4-per-color arrangement for center types."""
        dev = self.device
        if self.is_color:
            colors = torch.arange(self.slots, device=dev) // 4
            order = _random_perms(batch, self.slots, generator, dev)
            return colors[order]
        if self.name == "Corner":
            movable = torch.tensor(
                [s for s in range(8) if s != FIXED_CORNER_SLOT], device=dev, dtype=torch.long
            )
            perm7 = _even(_random_perms(batch, 7, generator, dev))
            pieces = torch.full((batch, 8), FIXED_CORNER_SLOT, device=dev, dtype=torch.long)
            pieces[:, movable] = movable[perm7]
            twist = torch.randint(3, (batch, 8), generator=generator, device=dev)
            twist[:, FIXED_CORNER_SLOT] = 0
            twist[:, movable[-1]] = 0
            twist[:, movable[-1]] = (-twist.sum(dim=1)) % 3
            return pieces * 3 + twist
        pieces = _even(_random_perms(batch, self.slots, generator, dev))
        if self.m == 1:
            return pieces
        flips = torch.randint(self.m, (batch, self.slots), generator=generator, device=dev)
        flips[:, -1] = 0
        flips[:, -1] = (-flips.sum(dim=1)) % self.m
        return pieces * self.m + flips


def _random_perms(batch: int, n: int, generator: torch.Generator | None, dev: torch.device) -> torch.Tensor:
    return torch.argsort(torch.rand(batch, n, generator=generator, device=dev), dim=1)


def permutation_parity(perms: torch.Tensor) -> torch.Tensor:
    """Parity (1 = odd) of each row, by counting inversions."""
    inv = (perms[:, :, None] > perms[:, None, :]).triu(diagonal=1).sum(dim=(1, 2))
    return inv % 2


def _even(perms: torch.Tensor) -> torch.Tensor:
    """Make every permutation even by swapping its first two entries where it is odd."""
    odd = permutation_parity(perms).bool()
    out = perms.clone()
    out[odd, 0], out[odd, 1] = perms[odd, 1], perms[odd, 0]
    return out


class Envs:
    """All seven types on one device, indexed by type id or name."""

    def __init__(self, lib: Library, device: torch.device | str = "cpu") -> None:
        self.library_sha256 = lib.sha256
        self.envs = [OrbitEnv(lib.types[name], device) for name in TYPE_NAMES]

    def __getitem__(self, key: int | str) -> OrbitEnv:
        if isinstance(key, str):
            return self.envs[TYPE_NAMES.index(key)]
        return self.envs[key]

    def __iter__(self):
        return iter(self.envs)

    def __len__(self) -> int:
        return len(self.envs)

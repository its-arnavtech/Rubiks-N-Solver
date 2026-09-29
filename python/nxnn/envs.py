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


class BatchEnv:
    """All seven types in one table, for mixed-type batches in training.

    States are padded to 24 slots (padding holds 0 and never moves); actions are per-type ids.
    One gather applies actions of any mix of types, so a training step needs no per-type loop.
    """

    def __init__(self, lib: Library, device: torch.device | str = "cpu") -> None:
        dev = torch.device(device)
        self.device = dev
        types = [lib.types[name] for name in TYPE_NAMES]
        counts = [t.num_actions for t in types]
        self.counts = torch.tensor(counts, device=dev)
        self.offsets = torch.tensor([sum(counts[:i]) for i in range(7)], device=dev)
        self.slots = torch.tensor([t.slots for t in types], device=dev)
        self.m = torch.tensor([t.orientation_mod for t in types], device=dev)
        perm = torch.arange(24).repeat(sum(counts), 1)
        ori = torch.zeros(sum(counts), 24, dtype=torch.long)
        solved = torch.zeros(7, 24, dtype=torch.long)
        cost = []
        for i, t in enumerate(types):
            lo = sum(counts[:i])
            perm[lo : lo + t.num_actions, : t.slots] = torch.as_tensor(t.perm)
            ori[lo : lo + t.num_actions, : t.slots] = torch.as_tensor(t.ori_delta)
            solved[i, : t.slots] = torch.as_tensor(t.solved())
            cost.append(torch.as_tensor(t.cost, dtype=torch.float32))
        self.perm, self.ori = perm.to(dev), ori.to(dev)
        self.solved_table = solved.to(dev)
        self.cost_all = torch.cat(cost).to(dev)
        # Per-type id of each action's inverse (same slot effect undone), or -1 if the library
        # lacks it. Used to explore the way back along a scramble (ADR-015).
        inverse = torch.full((sum(counts),), -1, dtype=torch.long)
        for i, t in enumerate(types):
            key = {(tuple(p), tuple(o)): a for a, (p, o) in enumerate(zip(t.perm.tolist(), t.ori_delta.tolist()))}
            m = t.orientation_mod
            for a, (p, o) in enumerate(zip(t.perm.tolist(), t.ori_delta.tolist())):
                ip, io = [0] * t.slots, [0] * t.slots
                for j in range(t.slots):
                    ip[p[j]] = j
                    io[p[j]] = (m - o[j]) % m
                inverse[sum(counts[:i]) + a] = key.get((tuple(ip), tuple(io)), -1)
        self.inverse = inverse.to(dev)

    def global_ids(self, types: torch.Tensor, actions: torch.Tensor) -> torch.Tensor:
        return (self.offsets[types].reshape(-1, *([1] * (actions.dim() - 1))) + actions)

    def cost(self, types: torch.Tensor, actions: torch.Tensor) -> torch.Tensor:
        return self.cost_all[self.global_ids(types, actions)]

    def apply(self, types: torch.Tensor, states: torch.Tensor, actions: torch.Tensor) -> torch.Tensor:
        """`types[B]`, `states[B, 24]`, `actions[B]` → new states."""
        g = self.offsets[types] + actions
        moved = torch.gather(states, 1, self.perm[g])
        m = self.m[types][:, None]
        return moved // m * m + (moved % m + self.ori[g]) % m

    def is_solved(self, types: torch.Tensor, states: torch.Tensor) -> torch.Tensor:
        return (states == self.solved_table[types]).all(1)

    def random_actions(self, types: torch.Tensor, k: int, generator: torch.Generator | None = None) -> torch.Tensor:
        u = torch.rand(types.shape[0], k, generator=generator, device=self.device)
        return (u * self.counts[types][:, None]).long()

    def scramble(
        self, types: torch.Tensor, kmax: torch.Tensor, max_k: int, generator: torch.Generator | None = None
    ) -> tuple[torch.Tensor, torch.Tensor]:
        """`k ~ U(1, kmax[row])` random actions from solved, per row; `max_k ≥ kmax.max()`.
        Returns `(states, back)`: `back` is the inverse of the last scramble action (-1 if
        the library lacks it)."""
        b = types.shape[0]
        u = torch.rand(b, generator=generator, device=self.device)
        k = torch.minimum(1 + (u * kmax).long(), kmax)
        states = self.solved_table[types].clone()
        last = torch.zeros(b, dtype=torch.long, device=self.device)
        for step in range(max_k):
            a = self.random_actions(types, 1, generator).squeeze(1)
            live = step < k
            states = torch.where(live[:, None], self.apply(types, states, a), states)
            last = torch.where(live, a, last)
        return states, self.inverse[self.offsets[types] + last]


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

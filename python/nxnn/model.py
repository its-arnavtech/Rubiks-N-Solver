"""One shared Q-network for all orbit types (ARCHITECTURE §8, ADR-005/006).

Input: a type id and ≤ 24 slot contents. Tokens are `E_slot[t, i] + E_content[t, c_i]` after
a `CLS = E_type[t]` token; a pre-LN Transformer encodes them. The Q-head scores every action
of type `t`:

    Q(s, a) = cost(a) + softplus(h · E_action[t, a] + bias[t, a]),   h = MLP(CLS_out)

so Q ≥ cost(a) by construction, and actions of other types are masked.
"""

from __future__ import annotations

import torch
import torch.nn.functional as F
from torch import nn

from .config import ModelConfig
from .library import TYPE_NAMES, Library

MAX_SLOTS = 24
NUM_TYPES = len(TYPE_NAMES)


class QNet(nn.Module):
    def __init__(self, cfg: ModelConfig, lib: Library) -> None:
        super().__init__()
        d, e = cfg.d_model, cfg.action_emb_dim
        types = [lib.types[name] for name in TYPE_NAMES]
        counts = [t.num_actions for t in types]
        offsets = [0]
        for c in counts[:-1]:
            offsets.append(offsets[-1] + c)
        self.cfg = cfg
        self.max_actions = max(counts)
        self.register_buffer("counts", torch.tensor(counts), persistent=False)
        self.register_buffer("offsets", torch.tensor(offsets), persistent=False)
        self.register_buffer("slots", torch.tensor([t.slots for t in types]), persistent=False)
        self.register_buffer(
            "cost", torch.cat([torch.as_tensor(t.cost, dtype=torch.float32) for t in types]),
            persistent=False,
        )
        self.slot_emb = nn.Embedding(NUM_TYPES * MAX_SLOTS, d)
        self.content_emb = nn.Embedding(NUM_TYPES * MAX_SLOTS, d)
        self.type_emb = nn.Embedding(NUM_TYPES, d)
        layer = nn.TransformerEncoderLayer(
            d, cfg.n_heads, cfg.ffn_dim, dropout=0.0, activation="gelu",
            batch_first=True, norm_first=True,
        )
        self.encoder = nn.TransformerEncoder(layer, cfg.n_layers, enable_nested_tensor=False)
        self.norm = nn.LayerNorm(d)
        self.head = nn.Sequential(nn.Linear(d, d), nn.GELU(), nn.Linear(d, e))
        self.action_emb = nn.Embedding(sum(counts), e)
        self.action_bias = nn.Parameter(torch.zeros(sum(counts)))
        nn.init.normal_(self.action_emb.weight, std=e**-0.5)

    def encode(self, types: torch.Tensor, contents: torch.Tensor) -> torch.Tensor:
        """`types[B]`, `contents[B, 24]` (padding beyond the type's slots is ignored) → `h[B, e]`."""
        b = types.shape[0]
        pos = torch.arange(MAX_SLOTS, device=types.device)
        pad = pos[None, :] >= self.slots[types][:, None]
        base = types[:, None] * MAX_SLOTS
        tok = self.slot_emb(base + pos) + self.content_emb(base + contents.clamp(0, MAX_SLOTS - 1))
        x = torch.cat([self.type_emb(types)[:, None, :], tok], dim=1)
        mask = torch.cat([torch.zeros(b, 1, dtype=torch.bool, device=types.device), pad], dim=1)
        out = self.encoder(x, src_key_padding_mask=mask)
        return self.head(self.norm(out[:, 0]))

    def _type_q(self, h: torch.Tensor, t: int) -> torch.Tensor:
        """Q for every action of type `t`: `[n, A_t]`."""
        lo = int(self.offsets[t])
        hi = lo + int(self.counts[t])
        logits = h @ self.action_emb.weight[lo:hi].T + self.action_bias[lo:hi]
        return self.cost[lo:hi] + F.softplus(logits.float())

    def q_all(self, types: torch.Tensor, contents: torch.Tensor) -> torch.Tensor:
        """`[B, max_actions]`, `+inf` where the action does not exist for the row's type."""
        h = self.encode(types, contents)
        q = torch.full((types.shape[0], self.max_actions), float("inf"), device=h.device)
        for t in types.unique().tolist():
            rows = types == t
            q_t = self._type_q(h[rows], t)
            q[rows, : q_t.shape[1]] = q_t
        return q

    def q_type(self, t: int, contents: torch.Tensor) -> torch.Tensor:
        """All rows of one type: `[B, A_t]`."""
        types = torch.full((contents.shape[0],), t, dtype=torch.long, device=contents.device)
        return self._type_q(self.encode(types, contents), t)

    def q_actions(self, types: torch.Tensor, contents: torch.Tensor, actions: torch.Tensor) -> torch.Tensor:
        """Q of chosen actions: `actions[B, K]` (per-type indices) → `[B, K]`."""
        h = self.encode(types, contents)
        idx = self.offsets[types][:, None] + actions
        logits = (h[:, None, :] * self.action_emb(idx)).sum(-1) + self.action_bias[idx]
        return self.cost[idx] + F.softplus(logits.float())


def pad_contents(states: torch.Tensor) -> torch.Tensor:
    """Pad `[B, S]` slot contents to `[B, 24]`."""
    if states.shape[1] == MAX_SLOTS:
        return states
    return F.pad(states, (0, MAX_SLOTS - states.shape[1]))

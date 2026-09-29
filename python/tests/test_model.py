"""M6.2: model shapes, Q ≥ cost, action masking."""

import torch

from nxnn.config import ModelConfig
from nxnn.envs import Envs
from nxnn.library import TYPE_NAMES, load_library
from nxnn.model import QNet, pad_contents

LIB = load_library()
CFG = ModelConfig(d_model=32, n_layers=2, n_heads=2, ffn_dim=64, action_emb_dim=16)


def batch(envs: Envs, per_type: int = 3):
    g = torch.Generator().manual_seed(0)
    types, contents = [], []
    for env in envs:
        s = env.random_states(per_type, generator=g)
        types.append(torch.full((per_type,), env.type_id))
        contents.append(pad_contents(s))
    return torch.cat(types), torch.cat(contents)


def test_shapes_bounds_and_masking() -> None:
    torch.manual_seed(0)
    model = QNet(CFG, LIB)
    envs = Envs(LIB)
    types, contents = batch(envs)
    q = model.q_all(types, contents)
    assert q.shape == (len(types), max(LIB.types[n].num_actions for n in TYPE_NAMES))
    for row, t in enumerate(types.tolist()):
        tl = LIB.by_id(t)
        a = tl.num_actions
        assert torch.isinf(q[row, a:]).all()  # other types' action slots are masked
        cost = torch.as_tensor(tl.cost, dtype=torch.float32)
        assert (q[row, :a] >= cost).all()  # Q ≥ cost by construction
        assert torch.isfinite(q[row, :a]).all()
    # q_actions agrees with q_all; q_type agrees too.
    acts = torch.stack([torch.randint(LIB.by_id(t).num_actions, (4,)) for t in types.tolist()])
    qa = model.q_actions(types, contents, acts)
    assert torch.allclose(qa, q.gather(1, acts), atol=1e-4)
    xt = types == 3
    assert torch.allclose(model.q_type(3, contents[xt]), q[xt, : LIB.by_id(3).num_actions], atol=1e-4)


def test_padding_is_ignored_and_gradients_flow() -> None:
    torch.manual_seed(1)
    model = QNet(CFG, LIB)
    envs = Envs(LIB)
    s = envs["Corner"].random_states(4)
    a = pad_contents(s)
    b = a.clone()
    b[:, 8:] = 5  # garbage beyond the corner orbit's 8 slots
    t = torch.zeros(4, dtype=torch.long)
    assert torch.allclose(model.q_all(t, a), model.q_all(t, b), atol=1e-5)
    loss = model.q_actions(t, a, torch.zeros(4, 1, dtype=torch.long)).sum()
    loss.backward()
    assert model.action_emb.weight.grad is not None
    assert model.slot_emb.weight.grad.abs().sum() > 0

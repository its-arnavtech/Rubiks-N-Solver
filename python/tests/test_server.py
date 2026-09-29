"""M8.1: API contract tests (CONVENTIONS §8)."""

import base64

import pytest
import torch

nxsim = pytest.importorskip("nxsim")

from fastapi.testclient import TestClient  # noqa: E402

from nxnn.config import PYTHON_DIR, load_config  # noqa: E402
from nxnn.envs import Envs  # noqa: E402
from nxnn.library import DEFAULT_LIBRARY, load_library  # noqa: E402
from nxnn.model import QNet  # noqa: E402
from nxnn.server import create_app  # noqa: E402
from nxnn.solve import Solver  # noqa: E402

LIB = load_library()


def make_client(with_model: bool) -> TestClient:
    torch.manual_seed(0)
    model = QNet(load_config(PYTHON_DIR / "configs" / "smoke.yaml").model, LIB).eval() if with_model else None
    s = Solver(LIB, nxsim.Library(str(DEFAULT_LIBRARY)), Envs(LIB), model, "test.pt" if with_model else None,
               torch.device("cpu"))
    return TestClient(create_app(solver=s, max_n=30))


@pytest.fixture(scope="module")
def client() -> TestClient:
    return make_client(True)


def test_health(client: TestClient) -> None:
    r = client.get("/api/health").json()
    assert r["status"] == "ok" and r["library_sha256"] == LIB.sha256
    assert {"checkpoint", "device"} <= set(r)


def test_scramble_modes(client: TestClient) -> None:
    r = client.post("/api/scramble", json={"n": 5, "mode": "random_state", "seed": 3}).json()
    assert r["n"] == 5 and r["scramble_moves_b64"] is None
    assert len(base64.b64decode(r["facelets_b64"])) == 150
    r = client.post("/api/scramble", json={"n": 4, "mode": "moves", "seed": 1, "length": 30}).json()
    moves = base64.b64decode(r["scramble_moves_b64"])
    assert len(moves) == 120
    assert nxsim.apply_moves(4, nxsim.solved(4), moves) == base64.b64decode(r["facelets_b64"])


@pytest.mark.parametrize("solver", ["nn", "baseline"])
def test_solve_returns_verified_result(client: TestClient, solver: str) -> None:
    sc = client.post("/api/scramble", json={"n": 6, "seed": 2}).json()
    r = client.post("/api/solve", json={"n": 6, "facelets_b64": sc["facelets_b64"], "solver": solver, "max_steps": 4})
    assert r.status_code == 200, r.text
    res = r.json()
    for key in ["n", "solver", "verified", "moves_b64", "raw_len", "cancelled_len", "segments", "stats",
                "timings_ms", "checkpoint", "library_sha256"]:
        assert key in res
    assert res["verified"] is True and res["solver"] == solver
    start = base64.b64decode(sc["facelets_b64"])
    assert nxsim.verify(6, start, base64.b64decode(res["moves_b64"]))


def test_errors(client: TestClient) -> None:
    assert client.post("/api/scramble", json={"n": 31}).status_code == 400
    assert client.post("/api/scramble", json={"n": 1}).status_code == 400
    bad = bytearray(nxsim.solved(4))
    bad[0], bad[20] = bad[20], bad[0]
    r = client.post("/api/solve", json={"n": 4, "facelets_b64": base64.b64encode(bytes(bad)).decode()})
    assert r.status_code == 400
    r = client.post("/api/solve", json={"n": 4, "facelets_b64": "!!!"})
    assert r.status_code == 400
    no_model = make_client(False)
    good = base64.b64encode(nxsim.random_state(4, 0)).decode()
    assert no_model.post("/api/solve", json={"n": 4, "facelets_b64": good}).status_code == 503
    assert no_model.post("/api/solve", json={"n": 4, "facelets_b64": good, "solver": "baseline"}).status_code == 200


def test_orbits(client: TestClient) -> None:
    r = client.get("/api/orbits/7").json()
    assert [o["type"] for o in r["orbits"][:3]] == ["Corner", "MidEdge", "FixedCenter"]
    assert r["orbits"][9] == {"id": 9, "type": "ObliqueA", "indices": {"a": 1, "b": 2}}
    ids = base64.b64decode(r["sticker_orbit_b64"])
    assert len(ids) == 4 * 6 * 49

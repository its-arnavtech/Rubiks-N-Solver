# Decision Log (ADRs)

Each record gives the decision, the reason, and the alternative we rejected. Records are **append-only**. To change a decision, add a new ADR that says `Supersedes ADR-0xx`, and update the old one's status line. Never silently edit a decision.

---

### ADR-001 — Decompose every cube into orbits; the network solves orbits, plain code manages
**Status:** accepted (2026-09-28)
**Decision:** A cube is split into orbits (24-slot groups of 7 fixed types). Code decides the phase order and hands orbits to the network.
**Why:** A single network looking at the whole cube cannot realistically generalize to unseen sizes (up to 400×400) or train on one laptop GPU. Orbits look identical at every N, so generalization is guaranteed by construction.
**Rejected:** a whole-cube transformer with local/global attention trained on small N, hoping it extrapolates. There's no evidence it would, and it has quadratic cost at large N.
**Cost:** the high-level strategy (phase order) is given, not learned.

### ADR-002 — Fixed-corner frame: layer N−1 is never turned
**Status:** accepted
**Why:** It removes whole-cube-rotation duplicates, makes "solved" unique, and works the same for odd and even N. It is equivalent in power to the full move set.
**Rejected:** allowing all layers plus whole-cube rotations. That creates 24× redundant states and an ambiguous solved state on even N.

### ADR-003 — New simulator `nx-sim`; `rg-cube` kept as a test oracle
**Status:** accepted
**Why:** `rg-cube` caps N at 7, uses an 8-bit layer mask and 16-bit sticker indices, and builds a full permutation per move. `nx-sim` supports any N with O(affected) moves. It uses rg-cube's exact facelet conventions so rg-cube can verify it for N ≤ 7.

### ADR-004 — Macros are discovered by automated search and verified; the network chooses among them
**Status:** accepted
**Why:**
- Commutator search is cheap, and the result is exactly checkable. Existence is mathematically guaranteed (Demaine et al. 2011).
- Precedent: focused macro-actions, IJCAI 2021.
- The network gets a clean, N-independent action space.
- No human solving algorithms are supplied.

**Rejected:** a network that emits primitive moves for each orbit. The reward is sparse, "preserve everything else" is very hard to learn, and it is not N-independent. This stays a stretch goal.

### ADR-005 — Self-play Q-value iteration (DeepCubeAQ-style); no teacher data
**Status:** accepted
**Why:** Training uses only states generated from the solved state, plus the network's own bootstrapped targets. That meets the "no external solver" requirement. A Q-head scores ~2,000 actions in one forward pass instead of expanding every child.
**Note:** This is "self-generated learning" (value iteration), not textbook policy-gradient RL. The user accepted this framing.

### ADR-006 — One shared network for all orbit types
**Status:** accepted
**Why:** It matches the user's goal of *one* network. The type embedding plus per-type action embeddings let it share encoder capacity. The input is always ≤ 25 tokens.

### ADR-007 — Correctness is enforced by code, never by the network
**Status:** accepted
**Decision:**
- Parity, the core frame and validation are exact code.
- Every orbit has a baseline fallback.
- Every result is replayed and verified.
- An unverified result is an error, never reported as a solve.

**Why:** 40,000 orbits multiply even tiny failure rates, and a solver that is sometimes silently wrong is useless.

### ADR-008 — Training is decoupled from Rust
**Status:** accepted
**Why:** The orbit puzzles run as tensor gathers built from `library.json`. Training needs only PyTorch, so WSL2 or Windows both work without building Rust, and the GPU never waits on the CPU.

### ADR-009 — Local-only UI: FastAPI server + existing Vite/React/three.js app; `nx-wasm` replays moves
**Status:** accepted
**Why:** The user wants a local page to watch orbits get solved for N ≤ 100. Replaying with the same Rust code compiled to wasm guarantees the picture matches the verified solution.

### ADR-010 — Stay in the `rubiks-graph` repo; legacy crates are frozen
**Status:** accepted (user decision)
**Decision:**
- `rg-graph`, `rg-solve`, `rg-wasm` and `rg-cli` stay compiling but are not extended.
- `rg-cube` is used only as a test oracle.
- The legacy UI is replaced in M8.
- Removing the legacy crates entirely is a later task, only if the user asks.

### ADR-011 — `library.json` hash pins checkpoints
**Status:** accepted
**Why:** Action ids are the network's outputs. A changed library silently breaks a trained model, so the loader refuses a mismatched hash.

### ADR-012 — No hidden information reaches the solver or the network
**Status:** accepted
**Why:** Real cubes only show colors. The solver never sees center identities or other labels that exist only inside `LabeledCube`.

### ADR-013 — WSL2 for training only if setup is quick; otherwise native Windows
**Status:** accepted (user decision)
**Decision:** Time-box WSL2 setup to about 30 minutes. If anything blocks it (virtualization disabled, install errors, `nvidia-smi` failing inside WSL), use native Windows. Code must never depend on WSL-only features. `torch.compile` is optional and off by default.

### ADR-014 — Q-head gets a state-value term and an output scale
**Status:** accepted (2026-09-28, M6.3; refines ARCHITECTURE §8, does not supersede an ADR)
**Decision:** `Q(s,a) = cost(a) + q_scale · softplus( v(s) + h·E_action[t,a] + bias[t,a] )`, where `v(s) = MLP_v(CLS_out)` is a scalar and `q_scale` is a config value (default 10).
**Why:** In the M6 smoke run the §8 head did not learn: the loss did not fall. For a state far from solved, Q must rise for *every* action, and with only `h·E_a + b_a` that has to happen through thousands of separately trained action embeddings (each seen rarely), while Huber gradients are capped at 1 and targets are tens to hundreds of moves. A per-state scalar lets one output carry "how far is this state", and the scale keeps logits O(1–10). With it the smoke loss drops within tens of steps.
**Kept:** `Q ≥ cost(a)` by construction, per-type masking, one pass scores all actions.
**Rejected:** plain MSE on raw targets (unstable at large targets); a separate value network (two models, more latency).

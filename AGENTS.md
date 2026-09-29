# AGENTS.md — rules for every coding agent (Claude Code, Codex, others)

This repo is built by more than one AI agent, taking turns. **Any agent may be stopped at any moment** (usage limits), and another agent continues from where it stopped. These rules exist so that handoff always works.

## 1. Start of every session (do this first, every time)
1. Read **`CHANGELOG.md` → "Current state"**. It tells you the active task, what is half-done, and blockers.
2. Run `git status` and `git log --oneline -10`. If there are uncommitted changes, compare them with the changelog's "In progress" note. Finish that work or clearly revert it. Never discard work silently.
3. Read `docs/nn/MILESTONES.md` for the next unchecked task. Then read the ARCHITECTURE/CONVENTIONS sections that task points to.
4. Run `just check` (once M0 is done) to confirm the tree is green before changing anything.

## 2. Source of truth
| Question | File |
|---|---|
| What are we building and why? | `docs/nn/ARCHITECTURE.md` |
| Exact formats, numbering, API | `docs/nn/CONVENTIONS.md` |
| What to do next, acceptance checks | `docs/nn/MILESTONES.md` |
| Why a choice was made | `docs/nn/DECISIONS.md` |
| Where we are right now | `CHANGELOG.md` |
| Machine setup | `docs/nn/SETUP.md` |

`docs/legacy/` and the `rg-graph`/`rg-solve`/`rg-wasm`/`rg-cli` crates belong to the **old** graph-theory project. Don't extend them (ADR-010). `rg-cube` is used only as a test oracle.

## 3. Working rules
- **One task at a time**, in milestone order. Don't start M(n+1) before M(n)'s acceptance passes, unless the changelog records a reason.
- **Small commits:** one task (or a clear sub-step) per commit. Message format: `M3.2: discovery search for Wing and XCenter`. Commit at least every ~30–45 minutes of work, so a sudden stop loses little.
- **Tests with the code.** A task isn't done without the tests its milestone lists.
- **Follow CONVENTIONS.md exactly.** If you must change a convention, update the doc, add an ADR if it's a design change, and bump `library_version` if it affects the library or checkpoints.
- **Never weaken a correctness gate** to make something pass: verification, purity, invariance, coverage, the "never report an unverified solve" rule. If a gate fails, fix the cause or record the blocker.
- **Don't change architecture silently.** If the design seems wrong, write the problem and a proposal in CHANGELOG under "Open questions / needs user", and add a *proposed* ADR. Continue with other tasks if possible.
- **No destructive git** (force push, reset --hard, deleting branches or others' work). Nothing is pushed to a remote unless the user asks.
- **Keep it portable:** Windows is the primary OS. Don't use bash-only scripts in `just` recipes. Use Python or Rust for tooling logic.
- **Determinism:** seeded RNG everywhere (CONVENTIONS §9). `library.json` must regenerate byte-identically.
- **Don't commit large or binary outputs:** checkpoints, runs and target dirs are git-ignored.
- **Long GPU runs:** start them in the background with logs to `runs/`. Write the exact command and how to monitor it in CHANGELOG. Don't block on them.

## 4. End of every session, and before you might stop (handoff protocol)
Update **`CHANGELOG.md`** and commit it, even for partial work:
1. **Overwrite "Current state"**: active milestone, last completed task, in-progress task (what's done, what's left, which files), next task, blockers, how to verify, date and agent name.
2. **Prepend a log entry** (newest first) using the template in CHANGELOG.md.
3. Tick finished tasks in `docs/nn/MILESTONES.md`.
4. If you're stopping mid-task, commit the work in progress with a message starting `WIP M<x.y>:`. Say exactly what is incomplete or broken, so the next agent can pick it up.

If you're about to run out of context or usage, **do step 4 first**.

## 5. Commands (after M0)
```
just check            # the gate: fmt, clippy, cargo test, pytest -m "not gpu", web check
just discover         # regenerate artifacts/macros/library.json
just verify-library   # purity / invariance / coverage
just baseline N SEED  # verified baseline solve
just py-build         # build the nxsim Python module (maturin)
just train CONFIG     # training (GPU)
just eval CKPT        # evaluation
just solve N SEED SOLVER
just serve            # local server + web UI
```

## 6. Definition of done (for any task)
Code, tests, green `just check`, docs updated where behavior or format changed, milestone box ticked, CHANGELOG updated, committed.

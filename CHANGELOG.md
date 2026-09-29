# CHANGELOG

Handoff log for all agents. See AGENTS.md §4 for how to update it.
- **"Current state"** is overwritten every session.
- **"Log"** is append-only, newest first.

---

## Current state

- **Project:** NxN neural cube solver (see `docs/nn/ARCHITECTURE.md`)
- **Active milestone:** M0 — Repo preparation
- **Last completed task:** M0.1 — stale lock removed; legacy baseline committed and tagged `legacy-graph-theory`; new docs committed
- **In progress:** none
- **Next task:** **M0.3** (workspace skeleton crates), then M0.4–M0.6
- **Blockers:** none
- **Needs user:** M0.7. Try WSL2 per `docs/nn/SETUP.md` §2, time-boxed; otherwise use native Windows. Report which one.
- **How to verify:** the legacy workspace still builds with `cargo build`.
- **Last updated:** 2026-09-28 by Claude Code (Sonnet 5.5)

---

## Log

### Entry template
```
### YYYY-MM-DD — <agent name/model> — <task ids>
**Done:** what was completed (with task ids)
**Files:** main files added/changed
**Tests:** what was run and the result (e.g. `just check` green; 1000/1000 verified at N=2..20)
**Decisions:** ADRs added/changed, conventions changed (and library_version bump if any)
**Problems / open questions:** anything unresolved
**Next:** the exact next step
```

### 2026-09-28 — Claude Code (Sonnet 5.5) — M0.1
**Done:** M0.1. `.git/index.lock` was empty and no git process was running, so it was removed. Committed the legacy tree (rg-* crates, web, xtask, docs/legacy) as `cc2bf05`, tagged `legacy-graph-theory`. The new project docs (AGENTS/CLAUDE/CHANGELOG/README, docs/nn) are in the following commit.
**Files:** none changed; git history only.
**Tests:** none (no code changes).
**Decisions:** none.
**Problems / open questions:** `core.autocrlf=true` on this machine prints LF→CRLF warnings on add; harmless.
**Next:** M0.3.

### 2026-09-28 — Claude (Cowork) — M0.2
**Done:** Designed the full architecture with the user. Wrote the project docs.
**Files:**
- added `AGENTS.md`, `CLAUDE.md`, `CHANGELOG.md`
- added `docs/nn/ARCHITECTURE.md`, `CONVENTIONS.md`, `MILESTONES.md`, `DECISIONS.md`, `SETUP.md`
- replaced `README.md`
- moved the old `README.md` and `docs/01..12-*.md` into `docs/legacy/`

**Tests:** none. No code was written.
**Decisions:** ADR-001 … ADR-013 (orbit decomposition, fixed-corner frame, nx-sim with rg-cube oracle, searched macros, Q-value iteration, one shared network, correctness by code, training decoupled from Rust, local UI, same repo, library hash pins checkpoints, no hidden info, WSL2 time-boxed).
**Problems / open questions:**
- Repo state as found: no commits, and a stale `.git/index.lock` that could not be removed from the architecture session.
- The existing `web/` app (React 19, Vite, three.js, Zustand, Tailwind, pnpm) and `xtask` (wasm build) will be reused. The legacy UI will be replaced in M8.

**Next:** M0.1, then M0.3–M0.6.

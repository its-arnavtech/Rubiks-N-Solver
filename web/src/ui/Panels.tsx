import { type ReactNode, useMemo, useState } from "react";
import { useStore } from "../app/store";
import type { CubeController } from "../engine/controller";
import { engine } from "../engine/engine";
import { SOLVABLE_SIZES } from "../engine/solver";

export function Card({ title, children, aside }: { title: string; children: ReactNode; aside?: ReactNode }) {
  return (
    <section className="rounded-xl border border-white/8 bg-white/[0.03] p-4">
      <div className="mb-3 flex items-center justify-between">
        <h2 className="text-[11px] font-semibold uppercase tracking-[0.14em] text-zinc-400">{title}</h2>
        {aside}
      </div>
      {children}
    </section>
  );
}

function Button({
  children,
  onClick,
  variant = "plain",
  title,
  disabled = false,
}: {
  children: ReactNode;
  onClick: () => void;
  variant?: "plain" | "primary" | "go";
  title?: string;
  disabled?: boolean;
}) {
  const style = {
    primary: "bg-indigo-500 text-white hover:bg-indigo-400",
    go: "bg-emerald-500 text-emerald-950 hover:bg-emerald-400",
    plain: "bg-white/6 text-zinc-200 hover:bg-white/12",
  }[variant];
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      disabled={disabled}
      className={`rounded-lg px-3 py-1.5 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${style}`}
    >
      {children}
    </button>
  );
}

function SolveButton({ controller }: { controller: CubeController }) {
  const n = useStore((s) => s.n);
  const solved = useStore((s) => s.solved);
  const pending = useStore((s) => s.pending);
  const status = useStore((s) => s.solve.kind);
  const busy = status === "preparing" || status === "searching";
  const supported = SOLVABLE_SIZES.has(n);

  const title = !supported
    ? `The ${n}×${n} solver is not in the app yet`
    : solved
      ? "Already solved: scramble first"
      : pending > 0
        ? "Wait for the queued moves to finish"
        : "Find a path from this position to the solved vertex";
  const label = status === "preparing" ? "Building tables…" : status === "searching" ? "Searching…" : "Solve";

  return (
    <Button
      variant="go"
      onClick={() => void controller.solve()}
      disabled={!supported || solved || pending > 0 || busy}
      title={title}
    >
      {label}
    </Button>
  );
}

export function StateCard({ controller }: { controller: CubeController }) {
  const lastScramble = useStore((s) => s.lastScramble);
  const error = useStore((s) => s.error);
  const speedMs = useStore((s) => s.speedMs);
  const [alg, setAlg] = useState("");

  const submit = () => {
    if (alg.trim() && controller.applyAlg(alg)) setAlg("");
  };

  return (
    <Card title="State">
      <div className="flex flex-wrap gap-2">
        <Button variant="primary" onClick={() => controller.scramble()} title="Random-move scramble">
          Scramble
        </Button>
        <SolveButton controller={controller} />
        <Button onClick={() => controller.undo()} title="Ctrl+Z">
          Undo
        </Button>
        <Button onClick={() => controller.reset()}>Reset</Button>
      </div>

      <label className="mt-4 block text-xs text-zinc-400" htmlFor="alg">
        Algorithm (WCA / SiGN notation)
      </label>
      <div className="mt-1.5 flex gap-2">
        <input
          id="alg"
          value={alg}
          onChange={(e) => setAlg(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          placeholder="R U R' U'"
          spellCheck={false}
          className="mono min-w-0 flex-1 rounded-lg border border-white/10 bg-black/30 px-2.5 py-1.5 text-sm outline-none focus:border-indigo-400"
        />
        <Button onClick={submit}>Apply</Button>
      </div>
      {error && <p className="mt-2 text-xs text-rose-400">{error}</p>}

      {lastScramble && (
        <div className="mt-4">
          <p className="text-xs text-zinc-400">Last scramble</p>
          <p className="mono mt-1 text-xs leading-relaxed break-words text-zinc-300">{lastScramble}</p>
        </div>
      )}

      <label className="mt-4 flex items-center justify-between text-xs text-zinc-400" htmlFor="speed">
        <span>Turn speed</span>
        <span className="mono">{speedMs} ms</span>
      </label>
      <input
        id="speed"
        type="range"
        min={40}
        max={600}
        step={10}
        value={speedMs}
        onChange={(e) => useStore.setState({ speedMs: Number(e.target.value) })}
        className="mt-1.5 w-full accent-indigo-400"
      />
    </Card>
  );
}

export function MovePad({ controller }: { controller: CubeController }) {
  const n = useStore((s) => s.n);
  const bases = useMemo(() => {
    const faces = ["U", "D", "R", "L", "F", "B"];
    const out = [...faces];
    if (n >= 4) out.push(...faces.map((f) => `${f}w`));
    if (n % 2 === 1) out.push("M", "E", "S");
    out.push("x", "y", "z");
    return out;
  }, [n]);

  return (
    <Card
      title="Moves"
      aside={<span className="text-[11px] text-zinc-500">keys: U R F D L B · Shift = '</span>}
    >
      <div className="grid grid-cols-3 gap-1.5">
        {bases.map((b) => (
          <div key={b} className="flex overflow-hidden rounded-md border border-white/8">
            {[b, `${b}'`, `${b}2`].map((m) => (
              <button
                type="button"
                key={m}
                onClick={() => controller.applyAlg(m)}
                className="mono flex-1 bg-white/[0.03] py-1 text-xs text-zinc-300 transition-colors hover:bg-indigo-500/30"
              >
                {m}
              </button>
            ))}
          </div>
        ))}
      </div>
    </Card>
  );
}

const VERTICES: Record<number, string> = {
  2: "3,674,160",
  3: "43,252,003,274,489,856,000",
  4: "≈ 7.40 × 10⁴⁵",
  5: "≈ 2.83 × 10⁷⁴",
};
const METRIC: Record<number, string> = { 2: "HTM", 3: "HTM", 4: "OBTM", 5: "OBTM" };

export function GraphFacts() {
  const n = useStore((s) => s.n);
  const history = useStore((s) => s.history);
  const solved = useStore((s) => s.solved);
  const degree = useMemo(() => engine.generators(n).length * 3, [n]);
  const walk = useMemo(() => engine.simplify(history).length, [history]);

  const rows: Array<[string, ReactNode]> = [
    ["Vertices (positions)", VERTICES[n] ?? "?"],
    ["Degree (moves per vertex)", `${degree} (${METRIC[n] ?? ""})`],
    ["Walk from solved", `${walk} edge${walk === 1 ? "" : "s"}`],
    ["Distance to solved", solved ? "0 · at the goal vertex" : `≤ ${walk} (upper bound)`],
  ];

  return (
    <Card title="This cube as a graph">
      <dl className="space-y-2.5 text-sm">
        {rows.map(([k, v]) => (
          <div key={k}>
            <dt className="text-xs text-zinc-500">{k}</dt>
            <dd className="mono break-all text-zinc-100">{v}</dd>
          </div>
        ))}
      </dl>
      <p className="mt-3 text-xs leading-relaxed text-zinc-500">
        Every position is a vertex and every turn is an edge. Solving means finding a path to the solved
        vertex; on the 2×2 the path found is always a shortest one.
      </p>
    </Card>
  );
}

const SOLVER_BLURB: Record<number, string> = {
  2: "BFS over the whole graph (3,674,160 positions) gives every position's exact distance; the solver walks downhill, so solutions are optimal.",
  3: "Kociemba two-phase: IDA* through two coset graphs (2.2·10⁹ and 1.95·10¹⁰ vertices), guided by BFS distances in four ~1M-vertex quotient graphs.",
  4: "Reduction in three graph phases: one centre pair onto an axis (735,471-vertex quotient graph), a second pair plus wing orientation and parity (5.4M), then all centres and edge pairs (staged IDA*). The reduced cube is finished as a 3×3 by Kociemba.",
  5: "Reduction in four graph phases: R/L x- and t-centres onto their axis, U/D centres plus wing parity, wing and midge orientation, then both centre orbits. The twelve edges are then paired by chaining macro operators — short sequences, themselves found by search, that restore the centres and move only a few wings. Solutions run long (about 150 moves); the reduced cube is finished as a 3×3 by Kociemba.",
};

const PHASE_LABEL: Record<number, string> = {
  3: "Phase 1 + phase 2",
  4: "Phases 1 + 2 + 3 + 3×3",
  5: "Phases 1 + 2a + 2b + 3 + 3×3",
};

function Stat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <dt className="text-xs text-zinc-500">{label}</dt>
      <dd className="mono text-zinc-100">{value}</dd>
    </div>
  );
}

export function SolveCard() {
  const n = useStore((s) => s.n);
  const solve = useStore((s) => s.solve);

  if (!SOLVABLE_SIZES.has(n)) {
    return (
      <Card title="Solver">
        <p className="text-sm leading-relaxed text-zinc-400">
          The {n}×{n} solver is not offered yet. Its first phases work (centres onto their axes, edge
          orientation), but its final phase, solving the centres while pairing the edges, is still too slow
          and unpredictable. The 2×2, 3×3 and 4×4 can be solved.
        </p>
      </Card>
    );
  }

  return (
    <Card title="Solver">
      {solve.kind === "idle" && <p className="text-sm leading-relaxed text-zinc-400">{SOLVER_BLURB[n]}</p>}
      {solve.kind === "preparing" && (
        <p className="text-sm text-indigo-300">
          Building lookup tables with BFS (first solve only
          {n >= 4 ? `; the ${n}×${n} has over 10 million table entries, so this takes a while` : ""})…
        </p>
      )}
      {solve.kind === "searching" && <p className="text-sm text-indigo-300">Searching the graph…</p>}
      {solve.kind === "error" && <p className="text-sm text-rose-400">{solve.message}</p>}
      {solve.kind === "done" && (
        <div className="space-y-3">
          <p className="text-sm text-zinc-300">{solve.algorithm}</p>
          <dl className="grid grid-cols-2 gap-x-4 gap-y-2.5 text-sm">
            <Stat
              label={`Length (${METRIC[n] ?? "moves"})`}
              value={
                <>
                  {solve.length}{" "}
                  <span className={solve.optimal ? "text-emerald-400" : "text-zinc-500"}>
                    {solve.optimal ? "optimal" : "near-optimal"}
                  </span>
                </>
              }
            />
            <Stat label="Search time" value={`${solve.searchMs.toFixed(1)} ms`} />
            {solve.phaseLengths.length > 1 && (
              <Stat label={PHASE_LABEL[n] ?? "Phases"} value={solve.phaseLengths.join(" + ")} />
            )}
            {solve.nodes > 0 && <Stat label="Nodes expanded" value={solve.nodes.toLocaleString()} />}
            {solve.tableMs > 0 && <Stat label="Tables built in" value={`${solve.tableMs.toFixed(0)} ms`} />}
          </dl>
          <p className="mono text-xs leading-relaxed break-words text-zinc-300">{solve.text}</p>
        </div>
      )}
    </Card>
  );
}

export function Roadmap() {
  const steps: Array<[string, string, boolean]> = [
    ["M0", "Workspace, wasm toolchain, app shell", true],
    ["M1", "Geometry-derived NxN model + 3D viewer", true],
    ["M2", "Graph engine: BFS, IDA*, distance tables", true],
    ["M3", "2×2: BFS the entire 3.67M-vertex graph", true],
    ["M5", "3×3 Thistlethwaite: four coset graphs", false],
    ["M6", "3×3 Kociemba two-phase (search-tree view next)", true],
    ["M8", "4×4 reduction chain", true],
    ["M9", "5×5 reduction chain, with macro operators for the pairing phase", true],
  ];
  return (
    <Card title="Roadmap">
      <ol className="space-y-1.5 text-sm">
        {steps.map(([id, label, done]) => (
          <li key={id} className="flex gap-2.5">
            <span className={`mono w-7 shrink-0 ${done ? "text-emerald-400" : "text-zinc-500"}`}>{id}</span>
            <span className={done ? "text-zinc-200" : "text-zinc-500"}>
              {label}
              {done && " ✓"}
            </span>
          </li>
        ))}
      </ol>
    </Card>
  );
}

export function Timeline() {
  const n = useStore((s) => s.n);
  const history = useStore((s) => s.history);
  const pending = useStore((s) => s.pending);
  const text = useMemo(() => (history.length ? engine.format(n, history.slice(-60)) : ""), [n, history]);

  return (
    <div className="flex min-h-11 items-center gap-4 border-t border-white/8 px-5 py-2 text-sm">
      <span className="text-[11px] font-semibold uppercase tracking-[0.14em] text-zinc-400">History</span>
      <span className="mono text-zinc-500">{history.length}</span>
      <p className="mono min-w-0 flex-1 truncate text-zinc-300" dir="rtl">
        <bdi>{text || "—"}</bdi>
      </p>
      {pending > 0 && <span className="text-xs text-indigo-300">{pending} queued</span>}
    </div>
  );
}

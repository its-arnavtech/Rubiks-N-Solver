import { type ReactNode, useEffect, useMemo, useState } from "react";
import { scramble, seek, setSize, solve, stepBack, stepForward } from "../app/actions";
import { type Overlay, useStore } from "../app/store";
import { segmentAt } from "../app/timeline";
import { engine, formatMove, KIND_NAMES } from "../engine/engine";
import { KIND_COLOURS, STICKER_COLOURS } from "../theme";
import { useActiveOrbit, useSolvedOrbits } from "../view/CubeView";

export function Card({ title, children, right }: { title: string; children: ReactNode; right?: ReactNode }) {
  return (
    <section className="rounded-xl border border-white/8 bg-white/[0.03] p-4">
      <header className="mb-3 flex items-center justify-between">
        <h2 className="text-xs font-semibold tracking-wider text-zinc-400 uppercase">{title}</h2>
        {right}
      </header>
      {children}
    </section>
  );
}

function Seg<T extends string>({
  value,
  options,
  onChange,
  disabled,
}: {
  value: T;
  options: readonly (readonly [T, string])[];
  onChange: (v: T) => void;
  disabled?: (v: T) => boolean;
}) {
  return (
    <div className="flex rounded-lg bg-white/5 p-0.5">
      {options.map(([v, label]) => (
        <button
          type="button"
          key={v}
          disabled={disabled?.(v)}
          onClick={() => onChange(v)}
          className={`flex-1 rounded-md px-2 py-1 text-xs font-medium transition-colors disabled:opacity-30 ${
            v === value ? "bg-indigo-500 text-white" : "text-zinc-400 hover:text-zinc-100"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

const inputCls =
  "w-full rounded-md border border-white/10 bg-black/30 px-2 py-1 text-sm text-zinc-100 outline-none focus:border-indigo-400";

export function Controls() {
  const n = useStore((s) => s.n);
  const seed = useStore((s) => s.seed);
  const mode = useStore((s) => s.scrambleMode);
  const solver = useStore((s) => s.solver);
  const beam = useStore((s) => s.beam);
  const busy = useStore((s) => s.busy);
  const health = useStore((s) => s.health);
  const maxN = health?.max_n ?? 100;
  const [nText, setNText] = useState(String(n));
  useEffect(() => setNText(String(n)), [n]);
  const nnOk = health?.nn_available !== false;

  const commitN = () => {
    const v = Math.max(2, Math.min(maxN, Math.round(Number(nText)) || n));
    if (v !== n) void setSize(v);
    else setNText(String(n));
  };

  return (
    <Card title="Cube">
      <div className="space-y-3 text-sm">
        <label className="flex items-center gap-3">
          <span className="w-16 text-zinc-400">N</span>
          <input
            className={inputCls}
            type="number"
            min={2}
            max={maxN}
            value={nText}
            onChange={(e) => setNText(e.target.value)}
            onBlur={commitN}
            onKeyDown={(e) => e.key === "Enter" && commitN()}
          />
        </label>
        <div className="flex flex-wrap gap-1">
          {[3, 4, 7, 20, 50, 100].map((v) => (
            <button
              type="button"
              key={v}
              disabled={v > maxN}
              onClick={() => void setSize(v)}
              className={`rounded px-2 py-0.5 text-xs ${v === n ? "bg-indigo-500 text-white" : "bg-white/5 text-zinc-400 hover:text-zinc-100"}`}
            >
              {v}
            </button>
          ))}
        </div>
        <label className="flex items-center gap-3">
          <span className="w-16 text-zinc-400">Seed</span>
          <input
            className={inputCls}
            type="number"
            value={seed}
            onChange={(e) =>
              useStore.setState({ seed: Math.max(0, Math.floor(Number(e.target.value) || 0)) })
            }
          />
        </label>
        <Seg
          value={mode}
          options={[
            ["random_state", "random state"],
            ["moves", "move scramble"],
          ]}
          onChange={(v) => useStore.setState({ scrambleMode: v })}
        />
        <button
          type="button"
          disabled={!!busy}
          onClick={() => void scramble()}
          className="w-full rounded-md bg-white/10 py-1.5 text-sm font-medium hover:bg-white/15 disabled:opacity-40"
        >
          Scramble
        </button>
        <div className="border-t border-white/8 pt-3" />
        <Seg
          value={solver}
          options={[
            ["nn", "neural"],
            ["baseline", "baseline"],
          ]}
          onChange={(v) => useStore.setState({ solver: v })}
          disabled={(v) => v === "nn" && !nnOk}
        />
        <label className="flex items-center gap-3">
          <span className="w-16 text-zinc-400">Beam</span>
          <input
            className={inputCls}
            type="number"
            min={1}
            max={16}
            value={beam}
            disabled={solver !== "nn"}
            onChange={(e) =>
              useStore.setState({ beam: Math.max(1, Math.min(16, Number(e.target.value) || 1)) })
            }
          />
        </label>
        <button
          type="button"
          disabled={!!busy || (solver === "nn" && !nnOk)}
          onClick={() => void solve()}
          className="w-full rounded-md bg-indigo-500 py-1.5 text-sm font-semibold text-white hover:bg-indigo-400 disabled:opacity-40"
        >
          Solve
        </button>
        {busy && <p className="text-xs text-indigo-300">{busy}…</p>}
        {!nnOk && (
          <p className="text-xs text-amber-300/80">
            No trained checkpoint loaded: only the baseline is available.
          </p>
        )}
      </div>
    </Card>
  );
}

export function ViewCard() {
  const view = useStore((s) => s.view);
  const overlay = useStore((s) => s.overlay);
  const animate = useStore((s) => s.animate);
  const n = useStore((s) => s.n);
  return (
    <Card title="View">
      <div className="space-y-2">
        <Seg
          value={view}
          options={[
            ["3d", "3D"],
            ["net", "net"],
          ]}
          onChange={(v) => useStore.setState({ view: v })}
        />
        <Seg<Overlay>
          value={overlay}
          options={[
            ["colors", "colors"],
            ["orbits", "orbit overlay"],
            ["types", "by type"],
          ]}
          onChange={(v) => useStore.setState({ overlay: v })}
        />
        <label className="flex items-center gap-2 text-xs text-zinc-400">
          <input
            type="checkbox"
            checked={animate}
            disabled={n > 10}
            onChange={(e) => useStore.setState({ animate: e.target.checked })}
          />
          animate turns (N ≤ 10, move playback)
        </label>
        {overlay === "types" && (
          <div className="grid grid-cols-2 gap-1 pt-1 text-[11px] text-zinc-400">
            {KIND_NAMES.map((k, i) => (
              <span key={k} className="flex items-center gap-1.5">
                <span className="inline-block size-2.5 rounded-sm" style={{ background: KIND_COLOURS[i] }} />
                {k}
              </span>
            ))}
          </div>
        )}
      </div>
    </Card>
  );
}

const PHASE_ORDER = ["parity", "core_frame", "core", "orbits", "fallback"] as const;
const PHASE_LABEL: Record<string, string> = {
  parity: "Parity",
  core_frame: "Core frame",
  core: "Core",
  orbits: "Orbits",
  fallback: "Fallback",
};

export function Progress() {
  const result = useStore((s) => s.result);
  const pos = useStore((s) => s.pos);
  const orbitKind = useStore((s) => s.orbitKind);
  const solved = useSolvedOrbits();

  const perKind = useMemo(() => {
    const total = new Array(KIND_NAMES.length).fill(0);
    const done = new Array(KIND_NAMES.length).fill(0);
    orbitKind.forEach((k, i) => {
      total[k]++;
      if (solved[i]) done[k]++;
    });
    return { total, done };
  }, [orbitKind, solved]);

  const seg = result ? result.segments[segmentAt(result.segments, Math.max(0, pos - 1))] : undefined;
  const phase = pos === 0 ? null : (seg?.phase ?? (result && pos >= result.raw_len ? "done" : null));
  const allSolved = solved.every((x) => x === 1);

  return (
    <Card
      title="Progress"
      right={
        <span
          className={`rounded-full px-2 py-0.5 text-[11px] font-medium ${allSolved ? "bg-emerald-500/15 text-emerald-300" : "bg-amber-500/15 text-amber-300"}`}
        >
          {allSolved ? "solved" : "unsolved"}
        </span>
      }
    >
      <div className="mb-3 flex gap-1">
        {PHASE_ORDER.map((p) => (
          <span
            key={p}
            className={`flex-1 rounded px-1 py-0.5 text-center text-[10px] ${
              p === phase ? "bg-indigo-500 text-white" : "bg-white/5 text-zinc-500"
            }`}
          >
            {PHASE_LABEL[p]}
          </span>
        ))}
      </div>
      <div className="space-y-1.5">
        {KIND_NAMES.map((k, i) =>
          perKind.total[i] ? (
            <div key={k} className="text-xs">
              <div className="flex justify-between text-zinc-400">
                <span>{k}</span>
                <span className="mono">
                  {perKind.done[i]}/{perKind.total[i]}
                </span>
              </div>
              <div className="h-1.5 rounded bg-white/5">
                <div
                  className="h-1.5 rounded"
                  style={{
                    width: `${(100 * perKind.done[i]) / perKind.total[i]}%`,
                    background: KIND_COLOURS[i],
                  }}
                />
              </div>
            </div>
          ) : null,
        )}
      </div>
      {result && (
        <dl className="mono mt-3 grid grid-cols-2 gap-x-3 gap-y-0.5 text-[11px] text-zinc-400">
          <dt>solver</dt>
          <dd className="text-right text-zinc-200">{result.solver}</dd>
          <dt>round</dt>
          <dd className="text-right text-zinc-200">
            {seg?.round ?? "–"} / {result.stats.rounds}
          </dd>
          <dt>moves so far</dt>
          <dd className="text-right text-zinc-200">{pos.toLocaleString()}</dd>
          <dt>raw / cancelled</dt>
          <dd className="text-right text-zinc-200">
            {result.raw_len.toLocaleString()} / {result.cancelled_len.toLocaleString()}
          </dd>
          <dt>fallback orbits</dt>
          <dd className="text-right text-zinc-200">
            {result.stats.orbits_fallback} / {result.stats.orbits_total}
          </dd>
          <dt>server time</dt>
          <dd className="text-right text-zinc-200">{Math.round(result.timings_ms.total ?? 0)} ms</dd>
          <dt>verified</dt>
          <dd className="text-right text-emerald-300">{result.verified ? "yes" : "no"}</dd>
        </dl>
      )}
    </Card>
  );
}

export function Inspector() {
  const selected = useStore((s) => s.selected);
  const n = useStore((s) => s.n);
  const facelets = useStore((s) => s.facelets);
  const orbitKind = useStore((s) => s.orbitKind);
  const orbitInfo = useStore((s) => s.orbitInfo);
  const result = useStore((s) => s.result);
  const pos = useStore((s) => s.pos);
  const active = useActiveOrbit();

  const orbit = selected;
  const content = useMemo(() => {
    if (orbit === null) return null;
    try {
      return engine.extractOrbit(n, facelets, orbit);
    } catch {
      return null;
    }
  }, [n, facelets, orbit]);
  const slots = useMemo(() => (orbit === null ? null : engine.orbitSlots(n, orbit)), [n, orbit]);
  const history = useMemo(
    () => (result && orbit !== null ? result.segments.filter((s) => s.orbit_id === orbit) : []),
    [result, orbit],
  );

  if (orbit === null || !content || !slots) {
    return (
      <Card title="Orbit inspector">
        <p className="text-xs text-zinc-500">Click a sticker to inspect its orbit.</p>
      </Card>
    );
  }
  const kind = KIND_NAMES[orbitKind[orbit] ?? 0] ?? "?";
  const info = orbitInfo[orbit];
  const width = slots.length / content.length;
  const perRow = content.length === 24 ? 4 : content.length === 12 ? 6 : content.length;
  const isColor = !["Corner", "MidEdge", "Wing"].includes(kind);
  const m = kind === "Corner" ? 3 : kind === "MidEdge" ? 2 : 1;

  return (
    <Card
      title="Orbit inspector"
      right={
        <button
          type="button"
          className="text-xs text-zinc-500 hover:text-zinc-200"
          onClick={() => useStore.setState({ selected: null })}
        >
          close
        </button>
      }
    >
      <p className="mb-2 text-sm">
        <span className="font-semibold" style={{ color: KIND_COLOURS[orbitKind[orbit] ?? 0] }}>
          {kind}
        </span>{" "}
        <span className="mono text-zinc-400">
          #{orbit}
          {info && Object.keys(info.indices).length > 0
            ? ` (${Object.entries(info.indices)
                .map(([k, v]) => `${k}=${v}`)
                .join(", ")})`
            : ""}
        </span>
        {active === orbit && <span className="ml-2 text-xs text-indigo-300">acting now</span>}
      </p>
      <div className="mb-3 grid gap-1" style={{ gridTemplateColumns: `repeat(${perRow}, minmax(0, 1fr))` }}>
        {Array.from(content).map((c, i) => {
          const home = isColor ? Math.floor(i / 4) : i;
          const piece = isColor ? c : Math.floor(c / m);
          const ok = isColor ? c === home : c === i * m;
          const firstSticker = slots[i * width] ?? 0;
          return (
            <div
              // biome-ignore lint/suspicious/noArrayIndexKey: slots are positional
              key={i}
              title={`slot ${i}${isColor ? "" : `: piece ${piece}${m > 1 ? `, orientation ${c % m}` : ""}`}`}
              className={`mono grid h-7 place-items-center rounded text-[10px] ${ok ? "ring-1 ring-emerald-400/60" : ""}`}
              style={{ background: STICKER_COLOURS[facelets[firstSticker] ?? 0], color: "#111" }}
            >
              {isColor ? i : `${piece}${m > 1 ? `·${c % m}` : ""}`}
            </div>
          );
        })}
      </div>
      <h3 className="mb-1 text-[11px] font-semibold tracking-wider text-zinc-500 uppercase">
        Actions ({history.length})
      </h3>
      <div className="max-h-48 space-y-0.5 overflow-y-auto">
        {history.length === 0 && <p className="text-xs text-zinc-600">none</p>}
        {history.map((s) => {
          const current = pos > s.start && pos <= s.end;
          return (
            <button
              type="button"
              key={s.start}
              onClick={() => seek(s.end)}
              className={`mono flex w-full justify-between rounded px-1.5 py-0.5 text-[11px] ${
                current ? "bg-indigo-500/30 text-white" : pos >= s.end ? "text-zinc-400" : "text-zinc-600"
              } hover:bg-white/5`}
            >
              <span>
                {s.phase} r{s.round ?? "-"} a{s.action_id ?? "-"}
              </span>
              <span>{s.q !== null ? `Q ${s.q.toFixed(1)}` : `${s.end - s.start} mv`}</span>
            </button>
          );
        })}
      </div>
    </Card>
  );
}

export function Timeline() {
  const result = useStore((s) => s.result);
  const raw = useStore((s) => s.raw);
  const pos = useStore((s) => s.pos);
  const playing = useStore((s) => s.playing);
  const speed = useStore((s) => s.speed);
  const playMode = useStore((s) => s.playMode);
  const stops = useStore((s) => s.stops);

  useEffect(() => {
    if (!playing) return;
    const id = window.setInterval(() => {
      if (!stepForward()) useStore.setState({ playing: false });
    }, 1000 / speed);
    return () => window.clearInterval(id);
  }, [playing, speed]);

  if (!result) {
    return (
      <footer className="border-t border-white/8 px-5 py-3 text-xs text-zinc-500">
        Scramble a cube, then press Solve. The solution is replayed exactly by the Rust engine (wasm).
      </footer>
    );
  }
  const roundIdx = stops.round.filter((p) => p <= pos).length - 1;
  return (
    <footer className="space-y-2 border-t border-white/8 px-5 py-3">
      <div className="flex flex-wrap items-center gap-3">
        <button type="button" className="rounded bg-white/10 px-2 py-1 text-xs" onClick={() => seek(0)}>
          ⏮
        </button>
        <button type="button" className="rounded bg-white/10 px-2 py-1 text-xs" onClick={stepBack}>
          ◀
        </button>
        <button
          type="button"
          className="w-16 rounded bg-indigo-500 px-2 py-1 text-xs font-semibold text-white"
          onClick={() => {
            if (pos >= raw.length) seek(0);
            useStore.setState({ playing: !playing });
          }}
        >
          {playing ? "pause" : "play"}
        </button>
        <button type="button" className="rounded bg-white/10 px-2 py-1 text-xs" onClick={() => stepForward()}>
          ▶
        </button>
        <button
          type="button"
          className="rounded bg-white/10 px-2 py-1 text-xs"
          onClick={() => seek(raw.length)}
        >
          ⏭
        </button>
        <div className="w-56">
          <Seg
            value={playMode}
            options={[
              ["move", "per move"],
              ["action", "per action"],
              ["round", "per round"],
            ]}
            onChange={(v) => useStore.setState({ playMode: v })}
          />
        </div>
        <label className="flex items-center gap-2 text-xs text-zinc-400">
          speed
          <input
            type="range"
            min={1}
            max={60}
            value={speed}
            onChange={(e) => useStore.setState({ speed: Number(e.target.value) })}
          />
          <span className="mono w-10">{speed}/s</span>
        </label>
        <span className="mono ml-auto text-xs text-zinc-400">
          move {pos.toLocaleString()} / {raw.length.toLocaleString()} · round group {Math.max(0, roundIdx)} /{" "}
          {stops.round.length - 1}
          {pos > 0 && raw[pos - 1] !== undefined ? ` · last ${formatMove(raw[pos - 1] as number)}` : ""}
        </span>
      </div>
      <input
        type="range"
        className="w-full"
        min={0}
        max={stops.round.length - 1}
        value={Math.max(0, roundIdx)}
        onChange={(e) => seek(stops.round[Number(e.target.value)] ?? 0)}
        aria-label="Scrub by round"
      />
    </footer>
  );
}

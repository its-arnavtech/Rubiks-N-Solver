import { useEffect, useState } from "react";
import { useStore } from "./app/store";
import { CubeCanvas } from "./cube3d/CubeCanvas";
import { CubeController } from "./engine/controller";
import { engine, initEngine } from "./engine/engine";
import { NetView } from "./net/NetView";
import { Card, GraphFacts, MovePad, Roadmap, SolveCard, StateCard, Timeline } from "./ui/Panels";

let singleton: CubeController | null = null;

const FACE_KEYS: Record<string, string> = {
  KeyU: "U",
  KeyD: "D",
  KeyR: "R",
  KeyL: "L",
  KeyF: "F",
  KeyB: "B",
  KeyX: "x",
  KeyY: "y",
  KeyZ: "z",
  KeyM: "M",
  KeyE: "E",
  KeyS: "S",
};

function useKeyboard(controller: CubeController | null) {
  useEffect(() => {
    if (!controller) return;
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target?.closest("input, textarea")) return;
      if ((e.ctrlKey || e.metaKey) && e.code === "KeyZ") {
        e.preventDefault();
        controller.undo();
        return;
      }
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const base = FACE_KEYS[e.code];
      if (!base) return;
      if ("MES".includes(base) && controller.n % 2 === 0) return;
      e.preventDefault();
      controller.applyAlg(e.shiftKey ? `${base}'` : base);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [controller]);
}

export function App() {
  const [controller, setController] = useState<CubeController | null>(singleton);
  const [failure, setFailure] = useState<string | null>(null);
  const n = useStore((s) => s.n);
  const solved = useStore((s) => s.solved);
  const version = useStore((s) => s.engineVersion);

  useEffect(() => {
    initEngine()
      .then(() => {
        singleton ??= new CubeController(3);
        useStore.setState({ ready: true, engineVersion: engine.version() });
        setController(singleton);
      })
      .catch((e: unknown) => setFailure(String(e)));
  }, []);

  useKeyboard(controller);

  if (failure) {
    return <p className="p-8 text-rose-400">Engine failed to load: {failure}</p>;
  }

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-x-6 gap-y-2 border-b border-white/8 px-5 py-3">
        <div className="flex items-baseline gap-3">
          <h1 className="text-base font-semibold tracking-tight">rubiks-graph</h1>
          <span className="hidden text-xs text-zinc-500 sm:inline">position = vertex · turn = edge</span>
        </div>
        <nav className="flex rounded-lg bg-white/5 p-0.5" aria-label="Cube size">
          {[2, 3, 4, 5].map((size) => (
            <button
              type="button"
              key={size}
              onClick={() => controller?.setSize(size)}
              className={`rounded-md px-3 py-1 text-sm font-medium transition-colors ${
                size === n ? "bg-indigo-500 text-white" : "text-zinc-400 hover:text-zinc-100"
              }`}
            >
              {size}×{size}
            </button>
          ))}
        </nav>
        <div className="ml-auto flex items-center gap-3 text-xs text-zinc-500">
          <span
            className={`rounded-full px-2.5 py-1 font-medium ${
              solved ? "bg-emerald-500/15 text-emerald-300" : "bg-amber-500/15 text-amber-300"
            }`}
          >
            {solved ? "solved" : "scrambled"}
          </span>
          <span className="mono">engine {version ? `v${version} (wasm)` : "loading…"}</span>
        </div>
      </header>

      {/* Narrow screens: one column, the whole page scrolls. Wide: three columns, panels scroll. */}
      <main className="grid min-h-0 flex-1 grid-cols-1 overflow-y-auto lg:grid-cols-[300px_minmax(0,1fr)_320px] lg:overflow-hidden">
        <aside className="order-2 space-y-4 p-4 lg:order-1 lg:overflow-y-auto">
          {controller && <StateCard controller={controller} />}
          {controller && <SolveCard />}
          {controller && <MovePad controller={controller} />}
        </aside>

        <section className="relative order-1 min-h-[55vh] lg:order-2 lg:min-h-0">
          {controller ? (
            <CubeCanvas controller={controller} />
          ) : (
            <p className="absolute inset-0 grid place-items-center text-zinc-500">Loading engine…</p>
          )}
          <p className="pointer-events-none absolute bottom-3 left-1/2 -translate-x-1/2 text-xs text-zinc-600">
            drag to orbit · scroll to zoom
          </p>
        </section>

        <aside className="order-3 space-y-4 p-4 lg:overflow-y-auto">
          {controller && (
            <>
              <Card title="Net">
                <NetView />
              </Card>
              <GraphFacts />
              <Roadmap />
            </>
          )}
        </aside>
      </main>

      {controller && <Timeline />}
    </div>
  );
}

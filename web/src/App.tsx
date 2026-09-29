import { useEffect, useState } from "react";
import { api } from "./api";
import { setSize } from "./app/actions";
import { useStore } from "./app/store";
import { engine, initEngine } from "./engine/engine";
import { Controls, Inspector, Progress, Timeline, ViewCard } from "./ui/Panels";
import { CubeView } from "./view/CubeView";

export function App() {
  const [failure, setFailure] = useState<string | null>(null);
  const ready = useStore((s) => s.ready);
  const version = useStore((s) => s.engineVersion);
  const health = useStore((s) => s.health);
  const error = useStore((s) => s.error);
  const n = useStore((s) => s.n);

  useEffect(() => {
    initEngine()
      .then(async () => {
        useStore.setState({ ready: true, engineVersion: engine.version() });
        await setSize(useStore.getState().n);
      })
      .catch((e: unknown) => setFailure(String(e)));
    api
      .health()
      .then((h) => useStore.setState({ health: h, solver: h.nn_available === false ? "baseline" : "nn" }))
      .catch(() =>
        useStore.setState({ error: "API server not reachable on 127.0.0.1:8000 (run `just serve`)." }),
      );
  }, []);

  if (failure) return <p className="p-8 text-rose-400">Engine failed to load: {failure}</p>;

  return (
    <div className="flex h-full flex-col">
      <header className="flex flex-wrap items-center gap-x-6 gap-y-2 border-b border-white/8 px-5 py-3">
        <div className="flex items-baseline gap-3">
          <h1 className="text-base font-semibold tracking-tight">rubiks-graph · NxN orbit solver</h1>
          <span className="hidden text-xs text-zinc-500 sm:inline">
            {n}×{n} · one network, 7 orbit types
          </span>
        </div>
        <div className="mono ml-auto flex items-center gap-3 text-[11px] text-zinc-500">
          {health && (
            <span title={health.checkpoint ?? "no checkpoint"}>
              library {health.library_sha256.slice(0, 8)} · {health.device}
              {health.nn_available === false ? " · baseline only" : ""}
            </span>
          )}
          <span>engine {version ? `v${version} (wasm)` : "loading…"}</span>
        </div>
      </header>
      {error && (
        <div className="border-b border-rose-500/30 bg-rose-500/10 px-5 py-1.5 text-xs text-rose-300">
          {error}
          <button type="button" className="ml-3 underline" onClick={() => useStore.setState({ error: null })}>
            dismiss
          </button>
        </div>
      )}
      <main className="grid min-h-0 flex-1 grid-cols-1 overflow-y-auto lg:grid-cols-[280px_minmax(0,1fr)_320px] lg:overflow-hidden">
        <aside className="order-2 space-y-4 p-4 lg:order-1 lg:overflow-y-auto">
          <Controls />
          <ViewCard />
        </aside>
        <section className="relative order-1 min-h-[55vh] lg:order-2 lg:min-h-0">
          {ready ? (
            <CubeView />
          ) : (
            <p className="absolute inset-0 grid place-items-center text-zinc-500">Loading engine…</p>
          )}
        </section>
        <aside className="order-3 space-y-4 p-4 lg:overflow-y-auto">
          <Progress />
          <Inspector />
        </aside>
      </main>
      <Timeline />
    </div>
  );
}

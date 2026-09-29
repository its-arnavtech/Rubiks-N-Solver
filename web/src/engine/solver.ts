import * as Comlink from "comlink";
import type { SolverApi } from "./solver.worker";

export type { SolveResponse } from "./solver.worker";

/** Sizes with a solver in the app. */
export const SOLVABLE_SIZES: ReadonlySet<number> = new Set([2, 3, 4, 5]);

let remote: Comlink.Remote<SolverApi> | null = null;

/** Lazily start the solver worker. */
export function solverWorker(): Comlink.Remote<SolverApi> {
  remote ??= Comlink.wrap<SolverApi>(
    new Worker(new URL("./solver.worker.ts", import.meta.url), { type: "module", name: "solver" }),
  );
  return remote;
}

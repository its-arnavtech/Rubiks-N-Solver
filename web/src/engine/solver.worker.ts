// Solver worker: holds the full wasm instance with lookup tables, so table builds and
// graph searches never block the UI thread (docs/04 §1).
import * as Comlink from "comlink";
import init, { Solver } from "./pkg/rg_wasm.js";

export interface SolveResponse {
  /** Encoded [axis, layers, turns] triples, in the caller's frame. */
  moves: Uint8Array;
  algorithm: string;
  optimal: boolean;
  phaseLengths: number[];
  nodes: number;
  searchMs: number;
}

let solver: Solver | null = null;

async function instance(): Promise<Solver> {
  await init();
  solver ??= new Solver();
  return solver;
}

const api = {
  /** Build lookup tables for size n. Resolves to milliseconds spent (0 if cached). */
  async prepare(n: number): Promise<number> {
    return (await instance()).prepare(n);
  },

  async solve(
    n: number,
    facelets: Uint8Array,
    targetLength: number,
    maxTimeMs: number,
  ): Promise<SolveResponse> {
    const r = (await instance()).solve(n, facelets, targetLength, maxTimeMs);
    const out: SolveResponse = {
      moves: r.moves,
      algorithm: r.algorithm,
      optimal: r.optimal,
      phaseLengths: Array.from(r.phaseLengths),
      nodes: r.nodes,
      searchMs: r.searchMs,
    };
    r.free();
    return out;
  },
};

export type SolverApi = typeof api;

Comlink.expose(api);

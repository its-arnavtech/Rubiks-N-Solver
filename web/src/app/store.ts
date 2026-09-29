import { create } from "zustand";
import type { Move } from "../engine/engine";

export type SolveStatus =
  | { kind: "idle" }
  | { kind: "preparing"; n: number }
  | { kind: "searching"; n: number }
  | {
      kind: "done";
      n: number;
      text: string;
      length: number;
      algorithm: string;
      optimal: boolean;
      phaseLengths: number[];
      nodes: number;
      searchMs: number;
      tableMs: number;
    }
  | { kind: "error"; message: string };

export interface CubeState {
  ready: boolean;
  engineVersion: string;
  n: number;
  facelets: Uint8Array;
  history: Move[];
  pending: number;
  solved: boolean;
  lastScramble: string;
  error: string | null;
  speedMs: number;
  solve: SolveStatus;
}

export const useStore = create<CubeState>(() => ({
  ready: false,
  engineVersion: "",
  n: 3,
  facelets: new Uint8Array(),
  history: [],
  pending: 0,
  solved: true,
  lastScramble: "",
  error: null,
  speedMs: 180,
  solve: { kind: "idle" },
}));

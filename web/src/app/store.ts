import { create } from "zustand";
import type { Health, OrbitInfo, SolveResult } from "../api";
import type { PlayMode, Stops } from "./timeline";

export type Overlay = "colors" | "orbits" | "types";
export type View = "3d" | "net";

export interface AppState {
  ready: boolean;
  engineVersion: string;
  health: Health | null;
  // Inputs.
  n: number;
  seed: number;
  scrambleMode: "random_state" | "moves";
  solver: "nn" | "baseline";
  beam: number;
  // Cube and solution.
  start: Uint8Array;
  facelets: Uint8Array;
  stickerOrbit: Uint32Array;
  orbitKind: Uint8Array;
  orbitInfo: OrbitInfo[];
  result: SolveResult | null;
  raw: Uint32Array;
  stops: Stops;
  pos: number;
  // Playback and view.
  playing: boolean;
  speed: number; // steps per second
  playMode: PlayMode;
  view: View;
  overlay: Overlay;
  animate: boolean;
  selected: number | null;
  busy: string | null;
  error: string | null;
}

export const useStore = create<AppState>(() => ({
  ready: false,
  engineVersion: "",
  health: null,
  n: 7,
  seed: 1,
  scrambleMode: "random_state",
  solver: "nn",
  beam: 1,
  start: new Uint8Array(),
  facelets: new Uint8Array(),
  stickerOrbit: new Uint32Array(),
  orbitKind: new Uint8Array(),
  orbitInfo: [],
  result: null,
  raw: new Uint32Array(),
  stops: { action: [0], round: [0] },
  pos: 0,
  playing: false,
  speed: 8,
  playMode: "action",
  view: "3d",
  overlay: "orbits",
  animate: true,
  selected: null,
  busy: null,
  error: null,
}));

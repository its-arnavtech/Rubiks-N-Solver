// State transitions that touch the engine or the API. Components call these.
import { api } from "../api";
import { engine, fromB64, type Replayer, toB64, wordsFromB64 } from "../engine/engine";
import { useStore } from "./store";
import { buildStops, nextStop, prevStop } from "./timeline";

let replayer: Replayer | null = null;

const set = useStore.setState;
const get = useStore.getState;

function fail(e: unknown) {
  set({ busy: null, error: e instanceof Error ? e.message : String(e), playing: false });
}

/** Load cube size N: orbit maps from the engine, orbit indices from the server. */
export async function setSize(n: number) {
  replayer?.free();
  replayer = null;
  const solved = engine.solved(n);
  set({
    n,
    start: solved,
    facelets: solved,
    stickerOrbit: engine.stickerOrbits(n),
    orbitKind: engine.orbitKinds(n),
    orbitInfo: [],
    result: null,
    raw: new Uint32Array(),
    stops: { action: [0], round: [0] },
    pos: 0,
    playing: false,
    selected: null,
    error: null,
    view: n > 20 ? "net" : "3d",
  });
  try {
    const info = await api.orbits(n);
    if (get().n === n) set({ orbitInfo: info.orbits });
  } catch {
    // Server offline: indices are optional.
  }
}

export async function scramble() {
  const { n, scrambleMode, seed } = get();
  set({ busy: "scrambling", error: null, playing: false });
  try {
    const r = await api.scramble(n, scrambleMode, seed);
    const facelets = fromB64(r.facelets_b64);
    replayer?.free();
    replayer = null;
    set({
      start: facelets,
      facelets,
      result: null,
      raw: new Uint32Array(),
      pos: 0,
      busy: null,
      stops: { action: [0], round: [0] },
    });
  } catch (e) {
    fail(e);
  }
}

export async function solve() {
  const { n, start, solver, beam } = get();
  set({ busy: `solving (${solver})`, error: null, playing: false });
  try {
    const result = await api.solve(n, toB64(start), solver, beam);
    const raw = wordsFromB64(result.moves_b64);
    replayer?.free();
    const every = Math.max(256, Math.floor(4_000_000 / Math.max(1, start.length)));
    replayer = engine.replayer(n, start, raw, every);
    if (!replayer.endsSolved()) throw new Error("replayed solution does not end solved");
    // Start playing right away: a computed-but-paused solution looks like nothing happened.
    // Big cubes have thousands of actions, so they play a whole round per step.
    set({
      result,
      raw,
      stops: buildStops(result.segments),
      pos: 0,
      facelets: start,
      busy: null,
      playing: raw.length > 0,
      playMode: n > 20 ? "round" : get().playMode,
    });
  } catch (e) {
    fail(e);
  }
}

export function seek(pos: number) {
  if (!replayer) return;
  const clamped = Math.max(0, Math.min(pos, replayer.len()));
  replayer.seek(clamped);
  set({ pos: clamped, facelets: replayer.facelets() });
}

export function stepForward(): boolean {
  const { stops, playMode, pos, raw } = get();
  if (pos >= raw.length) return false;
  seek(nextStop(stops, playMode, pos, raw.length));
  return true;
}

export function stepBack() {
  const { stops, playMode, pos } = get();
  seek(prevStop(stops, playMode, pos));
}

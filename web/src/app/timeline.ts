// Navigation over a SolveResult's raw move list (pure functions, unit-tested).
import type { Segment } from "../api";

export type PlayMode = "move" | "action" | "round";

/** Index of the segment containing move `pos` (start ≤ pos < end), or -1. */
export function segmentAt(segments: readonly Segment[], pos: number): number {
  let lo = 0;
  let hi = segments.length - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    const s = segments[mid];
    if (!s) break;
    if (pos < s.start) hi = mid - 1;
    else if (pos >= s.end) lo = mid + 1;
    else return mid;
  }
  return -1;
}

/** Group key: all orbits' actions of one round play together (they commute: ARCHITECTURE §7). */
function groupKey(s: Segment, i: number): string {
  return s.phase === "orbits" || s.phase === "fallback" ? `${s.phase}:${s.round}` : `${s.phase}#${i}`;
}

export interface Stops {
  action: number[];
  round: number[];
}

/** Positions where an action / a round ends, ascending, starting with 0. */
export function buildStops(segments: readonly Segment[]): Stops {
  const action = [0];
  const round = [0];
  segments.forEach((s, i) => {
    if (s.end > (action[action.length - 1] ?? 0)) action.push(s.end);
    const next = segments[i + 1];
    if (!next || groupKey(next, i + 1) !== groupKey(s, i)) {
      if (s.end > (round[round.length - 1] ?? 0)) round.push(s.end);
    }
  });
  return { action, round };
}

export function nextStop(stops: Stops, mode: PlayMode, pos: number, len: number): number {
  if (mode === "move") return Math.min(pos + 1, len);
  const list = mode === "action" ? stops.action : stops.round;
  for (const p of list) if (p > pos) return p;
  return len;
}

export function prevStop(stops: Stops, mode: PlayMode, pos: number): number {
  if (mode === "move") return Math.max(pos - 1, 0);
  const list = mode === "action" ? stops.action : stops.round;
  let best = 0;
  for (const p of list) if (p < pos) best = p;
  return best;
}

/** Per orbit: is every sticker showing its own face's color? (= the orbit is solved) */
export function solvedOrbits(n: number, facelets: Uint8Array, stickerOrbit: Uint32Array, orbitCount: number) {
  const bad = new Uint8Array(orbitCount);
  const per = n * n;
  for (let i = 0; i < facelets.length; i++) {
    if (facelets[i] !== Math.floor(i / per)) bad[stickerOrbit[i] ?? 0] = 1;
  }
  return bad.map((b) => 1 - b);
}

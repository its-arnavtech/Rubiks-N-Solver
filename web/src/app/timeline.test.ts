import { describe, expect, it } from "vitest";
import type { Segment } from "../api";
import { buildStops, nextStop, prevStop, segmentAt, solvedOrbits } from "./timeline";

function seg(phase: Segment["phase"], start: number, end: number, round: number | null, orbit = 0): Segment {
  return { phase, orbit_id: orbit, orbit_type: "Wing", round, action_id: 1, q: null, start, end };
}

// parity (1 move), core (2 actions), then orbits: round 0 for two orbits, round 1 for one.
const SEGS: Segment[] = [
  seg("parity", 0, 1, null),
  seg("core", 1, 9, 0),
  seg("core", 9, 20, 1),
  seg("orbits", 20, 28, 0, 5),
  seg("orbits", 28, 37, 0, 6),
  seg("orbits", 37, 45, 1, 5),
];

describe("timeline", () => {
  it("finds the segment of a move", () => {
    expect(segmentAt(SEGS, 0)).toBe(0);
    expect(segmentAt(SEGS, 8)).toBe(1);
    expect(segmentAt(SEGS, 28)).toBe(4);
    expect(segmentAt(SEGS, 44)).toBe(5);
    expect(segmentAt(SEGS, 45)).toBe(-1);
  });

  it("stops at action ends and at round-group ends", () => {
    const stops = buildStops(SEGS);
    expect(stops.action).toEqual([0, 1, 9, 20, 28, 37, 45]);
    // Round 0 of the orbit phase (two orbits) plays as one step.
    expect(stops.round).toEqual([0, 1, 9, 20, 37, 45]);
    expect(nextStop(stops, "round", 20, 45)).toBe(37);
    expect(nextStop(stops, "action", 20, 45)).toBe(28);
    expect(nextStop(stops, "move", 20, 45)).toBe(21);
    expect(nextStop(stops, "round", 45, 45)).toBe(45);
    expect(prevStop(stops, "round", 37)).toBe(20);
    expect(prevStop(stops, "action", 0)).toBe(0);
  });

  it("marks orbits solved when every sticker shows its face colour", () => {
    const n = 2;
    const facelets = new Uint8Array(24).map((_, i) => Math.floor(i / 4));
    const orbit = new Uint32Array(24).map((_, i) => (i < 12 ? 0 : 1));
    expect(Array.from(solvedOrbits(n, facelets, orbit, 2))).toEqual([1, 1]);
    facelets[13] = 0;
    expect(Array.from(solvedOrbits(n, facelets, orbit, 2))).toEqual([1, 0]);
  });
});

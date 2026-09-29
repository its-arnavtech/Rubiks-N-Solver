// Bridge to the Rust engine (nx-wasm). All cube semantics come from Rust: this file never
// simulates a turn itself (ADR-009).
import init, {
  applyMoves,
  engineVersion,
  extractOrbit,
  orbitKinds,
  orbitSlots,
  Replayer,
  solvedFacelets,
  stickerOrbits,
  stickerPositions,
} from "./pkg/nx_wasm.js";

export * from "./moves";

export const KIND_NAMES = [
  "Corner",
  "MidEdge",
  "FixedCenter",
  "Wing",
  "XCenter",
  "PlusCenter",
  "ObliqueA",
  "ObliqueB",
] as const;
export type KindName = (typeof KIND_NAMES)[number];

let ready: Promise<void> | null = null;

export function initEngine(): Promise<void> {
  ready ??= init().then(() => undefined);
  return ready;
}

export const engine = {
  version: () => engineVersion(),
  solved: (n: number): Uint8Array => solvedFacelets(n),
  apply: (n: number, facelets: Uint8Array, moves: Uint32Array): Uint8Array => applyMoves(n, facelets, moves),
  stickerOrbits: (n: number): Uint32Array => stickerOrbits(n),
  orbitKinds: (n: number): Uint8Array => orbitKinds(n),
  orbitSlots: (n: number, orbit: number): Uint32Array => orbitSlots(n, orbit),
  extractOrbit: (n: number, facelets: Uint8Array, orbit: number): Uint8Array =>
    extractOrbit(n, facelets, orbit),
  stickerPositions: (n: number): Int32Array => stickerPositions(n),
  replayer: (n: number, facelets: Uint8Array, moves: Uint32Array, every: number) =>
    new Replayer(n, facelets, moves, every),
};

export type { Replayer };

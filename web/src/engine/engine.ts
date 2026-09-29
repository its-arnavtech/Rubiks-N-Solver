// Bridge to the Rust engine (rg-wasm). The engine is the single source of truth for
// cube semantics: this file never simulates a turn itself (docs/04 §3).
import init, {
  Cube,
  cubieCentres,
  engineVersion,
  formatAlg,
  generatorMoves,
  invertAlg,
  parseAlg,
  randomScramble,
  simplifyAlg,
  stickerPositions,
} from "./pkg/rg_wasm.js";

export type Axis = 0 | 1 | 2;
export interface Move {
  axis: Axis;
  /** Bit ℓ = layer ℓ, counted from the U/R/F face. */
  layers: number;
  /** Clockwise quarter turns seen from the U/R/F face. */
  turns: 1 | 2 | 3;
}

let ready: Promise<void> | null = null;

export function initEngine(): Promise<void> {
  ready ??= init().then(() => undefined);
  return ready;
}

export function decodeMoves(bytes: Uint8Array): Move[] {
  const out: Move[] = [];
  for (let i = 0; i + 2 < bytes.length; i += 3) {
    out.push({ axis: bytes[i] as Axis, layers: bytes[i + 1] ?? 0, turns: bytes[i + 2] as 1 | 2 | 3 });
  }
  return out;
}

export function encodeMoves(moves: readonly Move[]): Uint8Array {
  return Uint8Array.from(moves.flatMap((m) => [m.axis, m.layers, m.turns]));
}

export const engine = {
  version: () => engineVersion(),
  parse: (n: number, alg: string): Move[] => decodeMoves(parseAlg(n, alg)),
  format: (n: number, moves: readonly Move[]): string => formatAlg(n, encodeMoves(moves)),
  invert: (moves: readonly Move[]): Move[] => decodeMoves(invertAlg(encodeMoves(moves))),
  simplify: (moves: readonly Move[]): Move[] => decodeMoves(simplifyAlg(encodeMoves(moves))),
  scramble: (n: number, seed: bigint): Move[] => decodeMoves(randomScramble(n, seed)),
  generators: (n: number): Move[] => decodeMoves(generatorMoves(n)),
  stickerPositions: (n: number): Int32Array => stickerPositions(n),
  cubieCentres: (n: number): Int32Array => cubieCentres(n),
  newCube: (n: number): Cube => new Cube(n),
};

export type { Cube };

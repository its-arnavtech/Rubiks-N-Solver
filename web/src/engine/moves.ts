// Wire formats (CONVENTIONS §1–2) without the wasm module, so they are unit-testable.

export interface Move {
  axis: 0 | 1 | 2;
  layer: number;
  turns: 1 | 2 | 3;
}

/** Wire word `(layer << 4) | (axis << 2) | turns` → move. */
export function decodeMove(word: number): Move {
  return {
    axis: ((word >>> 2) & 3) as 0 | 1 | 2,
    layer: word >>> 4,
    turns: (word & 3) as 1 | 2 | 3,
  };
}

/** Display notation: `R`, `2R'`, `13U2`. */
export function formatMove(word: number): string {
  const m = decodeMove(word);
  const letter = "RUF"[m.axis] ?? "?";
  const suffix = m.turns === 1 ? "" : m.turns === 2 ? "2" : "'";
  return `${m.layer > 0 ? m.layer + 1 : ""}${letter}${suffix}`;
}

export function fromB64(s: string): Uint8Array {
  const bin = atob(s);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

/** Base64 of little-endian u32s → words. */
export function wordsFromB64(s: string): Uint32Array {
  const bytes = fromB64(s);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const out = new Uint32Array(bytes.length / 4);
  for (let i = 0; i < out.length; i++) out[i] = view.getUint32(4 * i, true);
  return out;
}

export function toB64(bytes: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(bin);
}

import { describe, expect, it } from "vitest";
import { decodeMove, formatMove, fromB64, toB64, wordsFromB64 } from "./moves";

describe("move wire format", () => {
  it("decodes (layer << 4) | (axis << 2) | turns", () => {
    expect(decodeMove((3 << 4) | (2 << 2) | 2)).toEqual({ axis: 2, layer: 3, turns: 2 });
    expect(decodeMove(0b0101)).toEqual({ axis: 1, layer: 0, turns: 1 });
  });

  it("formats display notation", () => {
    expect(formatMove((0 << 4) | (0 << 2) | 1)).toBe("R");
    expect(formatMove((1 << 4) | (1 << 2) | 3)).toBe("2U'");
    expect(formatMove((12 << 4) | (2 << 2) | 2)).toBe("13F2");
  });

  it("reads little-endian u32 lists from base64", () => {
    const words = [1, 0x1234, 0xabcdef, 7 << 4];
    const bytes = new Uint8Array(new Uint32Array(words).buffer); // little-endian hosts
    expect(Array.from(wordsFromB64(toB64(bytes)))).toEqual(words);
    expect(Array.from(fromB64(toB64(new Uint8Array([0, 255, 5]))))).toEqual([0, 255, 5]);
  });
});

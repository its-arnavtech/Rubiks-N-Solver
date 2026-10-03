import { describe, expect, it } from "vitest";
import { layerCentre, layerCubieCoord, turnAngle, turnSlabs } from "./slabs";

describe("turning-layer geometry", () => {
  it("splits the body into slabs that tile the cube, with a unit-thick turning layer", () => {
    for (const n of [2, 3, 4, 7, 10, 30]) {
      for (let layer = 0; layer < n; layer++) {
        const [pos, mid, neg] = turnSlabs(n, layer) as [[number, number], [number, number], [number, number]];
        expect(pos[1]).toBe(n / 2);
        expect(neg[0]).toBe(-n / 2);
        expect(pos[0]).toBeCloseTo(mid[1]);
        expect(mid[0]).toBeCloseTo(neg[1]);
        expect(mid[1] - mid[0]).toBeCloseTo(1);
        // `layer` layers lie on the positive side, the rest on the negative side.
        expect(pos[1] - pos[0]).toBeCloseTo(layer);
        expect(neg[1] - neg[0]).toBeCloseTo(n - 1 - layer);
      }
    }
  });

  it("puts the slab where that layer's stickers are", () => {
    for (const n of [3, 4, 9]) {
      for (let layer = 0; layer < n; layer++) {
        // Sticker positions are doubled coordinates; world = doubled / 2.
        expect(layerCentre(n, layer)).toBe(layerCubieCoord(n, layer) / 2);
      }
    }
    expect(layerCentre(3, 0)).toBe(1); // the R/U/F face layer
    expect(layerCentre(3, 1)).toBe(0); // the middle slice
  });

  it("turns clockwise seen from the positive face", () => {
    expect(turnAngle(1)).toBeCloseTo(-Math.PI / 2);
    expect(turnAngle(2)).toBeCloseTo(-Math.PI);
    expect(turnAngle(3)).toBeCloseTo(Math.PI / 2); // a counter-clockwise quarter turn, the short way
  });
});

// Geometry of a turning layer, kept free of three.js so it can be unit-tested.

/**
 * Along the turning axis, the cube body splits into three slabs `[lo, hi]` (world units, the
 * cube spans −N/2..N/2): the part on the positive side of the layer, the layer itself, and the
 * part on the negative side. `layer` counts from the positive face (CONVENTIONS §2).
 */
export function turnSlabs(n: number, layer: number): [number, number][] {
  const centre = layerCentre(n, layer);
  return [
    [centre + 0.5, n / 2],
    [centre - 0.5, centre + 0.5],
    [-n / 2, centre - 0.5],
  ];
}

/** World coordinate, along the axis, of the centre of `layer`. */
export function layerCentre(n: number, layer: number): number {
  return (n - 1) / 2 - layer;
}

/** Doubled cubie coordinate (as in the engine's sticker positions) of the stickers in `layer`. */
export function layerCubieCoord(n: number, layer: number): number {
  return n - 1 - 2 * layer;
}

/** Rotation angle (radians, right-handed about the positive axis) of `turns` clockwise quarter turns. */
export function turnAngle(turns: 1 | 2 | 3): number {
  return turns === 3 ? Math.PI / 2 : (-Math.PI / 2) * turns;
}

import type { Overlay } from "../app/store";
import { KIND_RGB, STICKER_RGB } from "../theme";

export interface ColorInput {
  facelets: Uint8Array;
  stickerOrbit: Uint32Array;
  orbitKind: Uint8Array;
  solved: Uint8Array; // per orbit, 1 = solved
  overlay: Overlay;
  active: number | null; // orbit acted on by the current segment
  selected: number | null;
}

const DIM = 0.28;

/** RGB per sticker (length 3 · stickers), shared by the 3D and net views. */
export function stickerColors(c: ColorInput, out?: Float32Array): Float32Array {
  const len = c.facelets.length;
  const rgb = out && out.length === 3 * len ? out : new Float32Array(3 * len);
  for (let i = 0; i < len; i++) {
    const orbit = c.stickerOrbit[i] ?? 0;
    let col: readonly number[];
    let k = 1;
    if (c.overlay === "types") {
      col = KIND_RGB[c.orbitKind[orbit] ?? 0] ?? [1, 1, 1];
      if (orbit !== c.selected && orbit !== c.active) k = 0.75;
    } else {
      col = STICKER_RGB[c.facelets[i] ?? 0] ?? [1, 1, 1];
      if (c.overlay === "orbits" && !c.solved[orbit] && orbit !== c.active && orbit !== c.selected) k = DIM;
    }
    let [r = 1, g = 1, b = 1] = col;
    r *= k;
    g *= k;
    b *= k;
    if (orbit === c.selected) {
      // Lift the selected orbit toward white so it stands out in every mode.
      r = r * 0.7 + 0.3;
      g = g * 0.7 + 0.3;
      b = b * 0.7 + 0.3;
    }
    rgb[3 * i] = r;
    rgb[3 * i + 1] = g;
    rgb[3 * i + 2] = b;
  }
  return rgb;
}

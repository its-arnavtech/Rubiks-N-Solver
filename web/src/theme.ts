/** Sticker colours by face index (U R F D L B), Western scheme. */
export const STICKER_COLOURS = [
  "#f4f4f5", // U white
  "#d7263d", // R red
  "#1fa35b", // F green
  "#ffd23f", // D yellow
  "#ff7f11", // L orange
  "#2563eb", // B blue
] as const;

export const FACE_NAMES = ["U", "R", "F", "D", "L", "B"] as const;

/** Orbit-type colours, indexed like KIND_NAMES (Corner … ObliqueB). */
export const KIND_COLOURS = [
  "#e11d48", // Corner
  "#f97316", // MidEdge
  "#71717a", // FixedCenter
  "#eab308", // Wing
  "#22c55e", // XCenter
  "#06b6d4", // PlusCenter
  "#6366f1", // ObliqueA
  "#d946ef", // ObliqueB
] as const;

function rgb(hex: string): [number, number, number] {
  const v = Number.parseInt(hex.slice(1), 16);
  return [((v >> 16) & 255) / 255, ((v >> 8) & 255) / 255, (v & 255) / 255];
}

export const STICKER_RGB = STICKER_COLOURS.map(rgb);
export const KIND_RGB = KIND_COLOURS.map(rgb);

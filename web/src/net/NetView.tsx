import { useMemo } from "react";
import { useStore } from "../app/store";
import { STICKER_COLOURS } from "../theme";

// Face placement in the cross-shaped net, in face units: U R F D L B (docs/05 §2).
const LAYOUT: ReadonlyArray<readonly [number, number]> = [
  [1, 0],
  [2, 1],
  [1, 1],
  [1, 2],
  [0, 1],
  [3, 1],
];
const CELL = 10;
const GAP = 1.2;

export function NetView() {
  const n = useStore((s) => s.n);
  const facelets = useStore((s) => s.facelets);
  const faceSize = n * CELL + 2 * GAP;

  // One cell per facelet, in facelet order: face-major, then row-major.
  const cells = useMemo(
    () =>
      LAYOUT.flatMap(([fx, fy], face) =>
        Array.from({ length: n * n }, (_, i) => ({
          sticker: face * n * n + i,
          x: fx * faceSize + GAP + (i % n) * CELL + 0.6,
          y: fy * faceSize + GAP + Math.floor(i / n) * CELL + 0.6,
        })),
      ),
    [n, faceSize],
  );

  return (
    <svg
      viewBox={`0 0 ${4 * faceSize} ${3 * faceSize}`}
      className="w-full"
      role="img"
      aria-label="Unfolded cube net"
    >
      {cells.map((cell) => (
        <rect
          key={cell.sticker}
          x={cell.x}
          y={cell.y}
          width={CELL - 1.2}
          height={CELL - 1.2}
          rx={1.6}
          fill={STICKER_COLOURS[facelets[cell.sticker] ?? 0]}
        />
      ))}
    </svg>
  );
}

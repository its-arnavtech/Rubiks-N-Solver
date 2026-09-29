import { useEffect, useMemo, useRef } from "react";
import { useStore } from "../app/store";
import { segmentAt, solvedOrbits } from "../app/timeline";
import { decodeMove, engine } from "../engine/engine";
import { CubeScene } from "./CubeScene";
import { stickerColors } from "./colors";

/** Orbit acted on by the move just applied (for highlighting). */
export function useActiveOrbit(): number | null {
  const pos = useStore((s) => s.pos);
  const result = useStore((s) => s.result);
  if (!result || pos === 0) return null;
  const i = segmentAt(result.segments, pos - 1);
  return i >= 0 ? (result.segments[i]?.orbit_id ?? null) : null;
}

export function useSolvedOrbits(): Uint8Array {
  const n = useStore((s) => s.n);
  const facelets = useStore((s) => s.facelets);
  const stickerOrbit = useStore((s) => s.stickerOrbit);
  const orbitKind = useStore((s) => s.orbitKind);
  return useMemo(
    () => solvedOrbits(n, facelets, stickerOrbit, orbitKind.length),
    [n, facelets, stickerOrbit, orbitKind],
  );
}

function useColors(): Float32Array {
  const facelets = useStore((s) => s.facelets);
  const stickerOrbit = useStore((s) => s.stickerOrbit);
  const orbitKind = useStore((s) => s.orbitKind);
  const overlay = useStore((s) => s.overlay);
  const selected = useStore((s) => s.selected);
  const active = useActiveOrbit();
  const solved = useSolvedOrbits();
  return useMemo(
    () => stickerColors({ facelets, stickerOrbit, orbitKind, solved, overlay, active, selected }),
    [facelets, stickerOrbit, orbitKind, solved, overlay, active, selected],
  );
}

function pick(sticker: number) {
  const { stickerOrbit, selected } = useStore.getState();
  const orbit = stickerOrbit[sticker] ?? null;
  useStore.setState({ selected: orbit === selected ? null : orbit });
}

function Cube3D() {
  const ref = useRef<HTMLDivElement>(null);
  const scene = useRef<CubeScene | null>(null);
  const shown = useRef({ n: 0, pos: 0 });
  const n = useStore((s) => s.n);
  const pos = useStore((s) => s.pos);
  const colors = useColors();

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    scene.current = new CubeScene(el, pick);
    return () => {
      scene.current?.dispose();
      scene.current = null;
      shown.current = { n: 0, pos: 0 };
    };
  }, []);

  useEffect(() => {
    const s = scene.current;
    if (!s || n < 2) return;
    s.setCube(n, engine.stickerPositions(n));
    shown.current = { n, pos: useStore.getState().pos };
  }, [n]);

  useEffect(() => {
    const s = scene.current;
    if (!s || shown.current.n !== n) return;
    const { animate, playMode, speed, raw } = useStore.getState();
    const oneStep = pos === shown.current.pos + 1;
    shown.current.pos = pos;
    const word = raw[pos - 1];
    if (oneStep && animate && n <= 10 && playMode === "move" && word !== undefined) {
      s.animateMove(decodeMove(word), Math.min(260, 800 / speed), () => s.setColors(colors));
    } else {
      s.finishAnimation();
      s.setColors(colors);
    }
  }, [colors, pos, n]);

  return <div ref={ref} className="absolute inset-0 cursor-grab active:cursor-grabbing" />;
}

// Face placement in the cross-shaped net, in face units: U R F D L B.
const LAYOUT: ReadonlyArray<readonly [number, number]> = [
  [1, 0],
  [2, 1],
  [1, 1],
  [1, 2],
  [0, 1],
  [3, 1],
];

function NetCanvas() {
  const ref = useRef<HTMLCanvasElement>(null);
  const n = useStore((s) => s.n);
  const colors = useColors();

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const draw = () => {
      const w = canvas.clientWidth;
      const cell = Math.max(0.5, Math.min(w / (4 * n + 3), (canvas.clientHeight || w) / (3 * n + 2)));
      canvas.width = Math.round(w * devicePixelRatio);
      canvas.height = Math.round((3 * n + 2) * cell * devicePixelRatio);
      canvas.style.height = `${(3 * n + 2) * cell}px`;
      const ctx = canvas.getContext("2d");
      if (!ctx) return;
      ctx.scale(devicePixelRatio, devicePixelRatio);
      ctx.fillStyle = "#0b0c10";
      ctx.fillRect(0, 0, w, (3 * n + 2) * cell);
      const gap = cell > 4 ? 1 : 0;
      LAYOUT.forEach(([fx, fy], face) => {
        const ox = fx * (n * cell + cell / 2) + cell / 2;
        const oy = fy * (n * cell + cell / 2) + cell / 2;
        for (let r = 0; r < n; r++) {
          for (let c = 0; c < n; c++) {
            const i = face * n * n + r * n + c;
            const R = Math.round(255 * (colors[3 * i] ?? 0));
            const G = Math.round(255 * (colors[3 * i + 1] ?? 0));
            const B = Math.round(255 * (colors[3 * i + 2] ?? 0));
            ctx.fillStyle = `rgb(${R},${G},${B})`;
            ctx.fillRect(ox + c * cell, oy + r * cell, cell - gap, cell - gap);
          }
        }
      });
      canvas.dataset.cell = String(cell);
    };
    draw();
    const obs = new ResizeObserver(draw);
    obs.observe(canvas);
    return () => obs.disconnect();
  }, [colors, n]);

  const onClick = (e: React.MouseEvent<HTMLCanvasElement>) => {
    const canvas = ref.current;
    if (!canvas) return;
    const cell = Number(canvas.dataset.cell ?? 1);
    const rect = canvas.getBoundingClientRect();
    const x = e.clientX - rect.left;
    const y = e.clientY - rect.top;
    LAYOUT.forEach(([fx, fy], face) => {
      const ox = fx * (n * cell + cell / 2) + cell / 2;
      const oy = fy * (n * cell + cell / 2) + cell / 2;
      const c = Math.floor((x - ox) / cell);
      const r = Math.floor((y - oy) / cell);
      if (r >= 0 && r < n && c >= 0 && c < n) pick(face * n * n + r * n + c);
    });
  };

  return (
    <div className="absolute inset-0 overflow-auto p-4">
      <canvas
        ref={ref}
        onClick={onClick}
        className="w-full cursor-crosshair"
        aria-label="Unfolded cube net"
      />
    </div>
  );
}

export function CubeView() {
  const view = useStore((s) => s.view);
  return view === "3d" ? <Cube3D /> : <NetCanvas />;
}

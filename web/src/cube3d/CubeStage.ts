// Imperative three.js renderer for an NxN cube (docs/09 §2).
// Geometry comes from the engine (doubled integer coordinates); colours come from the
// engine's facelet array. A turn animation is purely cosmetic: when it finishes, every
// cubie snaps back home and all stickers are recoloured from the engine's new state.
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { RoundedBoxGeometry } from "three/addons/geometries/RoundedBoxGeometry.js";
import type { Move } from "../engine/engine";
import { STICKER_COLOURS } from "../theme";

interface Cubie {
  group: THREE.Group;
  /** Doubled-coordinate cubie centre. */
  centre: [number, number, number];
  home: THREE.Vector3;
}

interface Animation {
  cubies: Cubie[];
  axis: "x" | "y" | "z";
  angle: number;
  start: number;
  duration: number;
  commit: () => Uint8Array;
  resolve: () => void;
  watchdog: number;
}

const AXES = ["x", "y", "z"] as const;
const easeInOut = (t: number) => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2);

function roundedSquare(size: number, radius: number): THREE.ShapeGeometry {
  const h = size / 2;
  const s = new THREE.Shape();
  s.moveTo(-h + radius, -h);
  s.lineTo(h - radius, -h);
  s.quadraticCurveTo(h, -h, h, -h + radius);
  s.lineTo(h, h - radius);
  s.quadraticCurveTo(h, h, h - radius, h);
  s.lineTo(-h + radius, h);
  s.quadraticCurveTo(-h, h, -h, h - radius);
  s.lineTo(-h, -h + radius);
  s.quadraticCurveTo(-h, -h, -h + radius, -h);
  return new THREE.ShapeGeometry(s, 6);
}

export class CubeStage {
  private readonly renderer: THREE.WebGLRenderer;
  private readonly scene = new THREE.Scene();
  private readonly camera = new THREE.PerspectiveCamera(30, 1, 0.1, 200);
  private readonly controls: OrbitControls;
  private readonly root = new THREE.Group();
  private readonly pivot = new THREE.Group();
  private readonly resizeObserver: ResizeObserver;
  private readonly bodyGeometry = new RoundedBoxGeometry(0.97, 0.97, 0.97, 3, 0.09);
  private readonly bodyMaterial = new THREE.MeshStandardMaterial({ color: 0x101114, roughness: 0.55 });
  private readonly stickerGeometry = roundedSquare(0.84, 0.12);
  private readonly stickerMaterials = STICKER_COLOURS.map(
    (c) => new THREE.MeshStandardMaterial({ color: c, roughness: 0.32, metalness: 0.0 }),
  );
  private n = 3;
  private cubies: Cubie[] = [];
  private stickers: THREE.Mesh[] = [];
  private anim: Animation | null = null;

  constructor(private readonly container: HTMLElement) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    container.appendChild(this.renderer.domElement);
    this.renderer.domElement.style.display = "block";

    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x3a3a48, 2.2));
    const key = new THREE.DirectionalLight(0xffffff, 2.0);
    key.position.set(4, 7, 5);
    const fill = new THREE.DirectionalLight(0xbfd4ff, 0.7);
    fill.position.set(-6, -2, -4);
    this.scene.add(key, fill);
    this.root.add(this.pivot);
    this.scene.add(this.root);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.enablePan = false;
    this.controls.rotateSpeed = 0.8;

    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(container);
    this.resize();
    document.addEventListener("visibilitychange", this.onVisibility);
    this.renderer.setAnimationLoop((t) => this.frame(t));
  }

  private readonly onVisibility = () => {
    if (document.hidden) this.finish(true);
  };

  /** Rebuild all cubies for size `n`. Cancels any running animation without committing it. */
  build(n: number, positions: Int32Array, centres: Int32Array, facelets: Uint8Array): void {
    this.cancelAnimation();
    for (const c of this.cubies) this.root.remove(c.group);
    this.cubies = [];
    this.stickers = [];
    this.n = n;

    const byKey = new Map<string, Cubie>();
    const count = positions.length / 3;
    for (let i = 0; i < count; i++) {
      const centre: [number, number, number] = [
        centres[3 * i] ?? 0,
        centres[3 * i + 1] ?? 0,
        centres[3 * i + 2] ?? 0,
      ];
      const key = centre.join(",");
      let cubie = byKey.get(key);
      if (!cubie) {
        const group = new THREE.Group();
        const home = new THREE.Vector3(centre[0] / 2, centre[1] / 2, centre[2] / 2);
        group.position.copy(home);
        group.add(new THREE.Mesh(this.bodyGeometry, this.bodyMaterial));
        this.root.add(group);
        cubie = { group, centre, home };
        byKey.set(key, cubie);
        this.cubies.push(cubie);
      }
      // The sticker's normal is the axis on which |position| == n.
      const p = [positions[3 * i] ?? 0, positions[3 * i + 1] ?? 0, positions[3 * i + 2] ?? 0];
      const normal = new THREE.Vector3(...p.map((v) => (Math.abs(v) === n ? Math.sign(v) : 0)));
      const sticker = new THREE.Mesh(this.stickerGeometry, this.stickerMaterials[0]);
      sticker.position.copy(normal).multiplyScalar(0.487);
      sticker.quaternion.setFromUnitVectors(new THREE.Vector3(0, 0, 1), normal);
      cubie.group.add(sticker);
      this.stickers.push(sticker);
    }
    this.setFacelets(facelets);
    this.frameCamera();
  }

  setFacelets(facelets: Uint8Array): void {
    this.stickers.forEach((s, i) => {
      s.material = this.stickerMaterials[facelets[i] ?? 0] ?? this.bodyMaterial;
    });
  }

  /** Animate one turn; `commit` is called synchronously at the end and returns new facelets. */
  animate(move: Move, durationMs: number, commit: () => Uint8Array): Promise<void> {
    this.cancelAnimation();
    // Hidden pages get no animation frames: apply immediately instead of stalling the queue.
    if (document.hidden) {
      this.setFacelets(commit());
      return Promise.resolve();
    }
    return new Promise((resolve) => {
      const cubies = this.cubies.filter((c) => {
        const layer = (this.n - 1 - c.centre[move.axis]) / 2;
        return (move.layers >> layer) & 1;
      });
      for (const c of cubies) this.pivot.add(c.group);
      // Clockwise seen from the + face is a negative angle (right-hand rule).
      const angle = move.turns === 3 ? Math.PI / 2 : (-Math.PI / 2) * move.turns;
      // If the browser stops delivering frames (occluded window, embedded pane), commit
      // anyway: the cube's state must never wait on the render loop.
      const watchdog = window.setTimeout(() => this.finish(true), durationMs + 250);
      this.anim = {
        cubies,
        axis: AXES[move.axis],
        angle,
        start: performance.now(),
        duration: durationMs,
        commit,
        resolve,
        watchdog,
      };
    });
  }

  dispose(): void {
    this.cancelAnimation();
    document.removeEventListener("visibilitychange", this.onVisibility);
    this.renderer.setAnimationLoop(null);
    this.resizeObserver.disconnect();
    this.controls.dispose();
    this.bodyGeometry.dispose();
    this.bodyMaterial.dispose();
    this.stickerGeometry.dispose();
    for (const m of this.stickerMaterials) m.dispose();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }

  private finish(commitIt: boolean): void {
    const a = this.anim;
    if (!a) return;
    this.anim = null;
    window.clearTimeout(a.watchdog);
    for (const c of a.cubies) {
      this.root.add(c.group);
      c.group.position.copy(c.home);
      c.group.quaternion.identity();
    }
    this.pivot.rotation.set(0, 0, 0);
    if (commitIt) this.setFacelets(a.commit());
    a.resolve();
  }

  private cancelAnimation(): void {
    this.finish(false);
  }

  private frame(now: number): void {
    // Moving the window between monitors changes the pixel ratio without a resize event.
    if (this.renderer.getPixelRatio() !== Math.min(window.devicePixelRatio, 2)) this.resize();
    const a = this.anim;
    if (a) {
      const t = Math.min(1, (now - a.start) / a.duration);
      this.pivot.rotation[a.axis] = a.angle * easeInOut(t);
      if (t >= 1) this.finish(true);
    }
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  }

  private frameCamera(): void {
    const dist = this.n * 3.1 + 3.5;
    this.camera.position.set(0.62, 0.52, 0.86).normalize().multiplyScalar(dist);
    this.controls.target.set(0, 0, 0);
    this.controls.minDistance = dist * 0.5;
    this.controls.maxDistance = dist * 2.5;
    this.controls.update();
  }

  private resize(): void {
    const w = Math.max(1, this.container.clientWidth);
    const h = Math.max(1, this.container.clientHeight);
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setSize(w, h);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }
}

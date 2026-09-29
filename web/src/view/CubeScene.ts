// three.js cube: one InstancedMesh for every sticker (60,000 at N=100), per-instance colour.
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import type { Move } from "../engine/engine";

const AXES = [new THREE.Vector3(1, 0, 0), new THREE.Vector3(0, 1, 0), new THREE.Vector3(0, 0, 1)];

export class CubeScene {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera: THREE.PerspectiveCamera;
  private controls: OrbitControls;
  private mesh: THREE.InstancedMesh | null = null;
  private body: THREE.Mesh | null = null;
  private base: THREE.Matrix4[] = [];
  private cubie: Int32Array = new Int32Array();
  private n = 0;
  private frame = 0;
  private anim: {
    stickers: number[];
    axis: number;
    angle: number;
    t0: number;
    ms: number;
    done: () => void;
  } | null = null;
  private resize: ResizeObserver;
  private raycaster = new THREE.Raycaster();

  constructor(
    private el: HTMLElement,
    private onPick: (sticker: number) => void,
  ) {
    this.renderer = new THREE.WebGLRenderer({ antialias: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    this.renderer.setClearColor(0x0b0c10);
    el.appendChild(this.renderer.domElement);
    this.camera = new THREE.PerspectiveCamera(35, 1, 0.1, 5000);
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.scene.add(new THREE.AmbientLight(0xffffff, 1));
    this.resize = new ResizeObserver(() => this.fitView());
    this.resize.observe(el);
    this.renderer.domElement.addEventListener("pointerdown", this.onDown);
    this.renderer.domElement.addEventListener("pointerup", this.onUp);
    this.fitView();
    this.loop();
  }

  private down: [number, number] | null = null;
  private onDown = (e: PointerEvent) => {
    this.down = [e.clientX, e.clientY];
  };
  private onUp = (e: PointerEvent) => {
    const d = this.down;
    this.down = null;
    if (!d || Math.hypot(e.clientX - d[0], e.clientY - d[1]) > 4 || !this.mesh) return;
    const rect = this.renderer.domElement.getBoundingClientRect();
    const ndc = new THREE.Vector2(
      ((e.clientX - rect.left) / rect.width) * 2 - 1,
      -((e.clientY - rect.top) / rect.height) * 2 + 1,
    );
    this.raycaster.setFromCamera(ndc, this.camera);
    const hit = this.raycaster.intersectObject(this.mesh)[0];
    if (hit?.instanceId !== undefined) this.onPick(hit.instanceId);
  };

  private fitView() {
    const w = this.el.clientWidth || 1;
    const h = this.el.clientHeight || 1;
    this.renderer.setSize(w, h);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

  /** Rebuild for size N from the engine's doubled-coordinate sticker positions. */
  setCube(n: number, positions: Int32Array) {
    this.dispose3d();
    this.n = n;
    const count = positions.length / 3;
    const geo = new THREE.PlaneGeometry(0.88, 0.88);
    const mat = new THREE.MeshBasicMaterial({ color: 0xffffff });
    const mesh = new THREE.InstancedMesh(geo, mat, count);
    mesh.instanceColor = new THREE.InstancedBufferAttribute(new Float32Array(3 * count), 3);
    const q = new THREE.Quaternion();
    const zAxis = new THREE.Vector3(0, 0, 1);
    this.base = [];
    this.cubie = new Int32Array(3 * count);
    for (let i = 0; i < count; i++) {
      const p = [positions[3 * i] ?? 0, positions[3 * i + 1] ?? 0, positions[3 * i + 2] ?? 0];
      const a = p.findIndex((v) => Math.abs(v) === n);
      const normal = new THREE.Vector3();
      normal.setComponent(a, Math.sign(p[a] ?? 1));
      q.setFromUnitVectors(zAxis, normal);
      const pos = new THREE.Vector3(p[0] ?? 0, p[1] ?? 0, p[2] ?? 0)
        .multiplyScalar(0.5)
        .addScaledVector(normal, 0.01);
      const m = new THREE.Matrix4().compose(pos, q, new THREE.Vector3(1, 1, 1));
      this.base.push(m);
      mesh.setMatrixAt(i, m);
      for (let k = 0; k < 3; k++) {
        const v = p[k] ?? 0;
        this.cubie[3 * i + k] = Math.abs(v) === n ? Math.sign(v) * (n - 1) : v;
      }
    }
    this.mesh = mesh;
    this.scene.add(mesh);
    this.body = new THREE.Mesh(
      new THREE.BoxGeometry(n * 0.995, n * 0.995, n * 0.995),
      new THREE.MeshBasicMaterial({ color: 0x111114 }),
    );
    this.scene.add(this.body);
    const dist = n * 2.6 + 3;
    this.camera.position.set(dist * 0.62, dist * 0.55, dist * 0.78);
    this.camera.near = 0.05 * n;
    this.camera.far = 20 * n + 50;
    this.camera.updateProjectionMatrix();
    this.controls.target.set(0, 0, 0);
    this.controls.update();
  }

  setColors(rgb: Float32Array) {
    if (!this.mesh?.instanceColor) return;
    (this.mesh.instanceColor.array as Float32Array).set(rgb);
    this.mesh.instanceColor.needsUpdate = true;
  }

  /** Animate one turn of the current colours; `done` then installs the new colours. */
  animateMove(move: Move, ms: number, done: () => void) {
    this.finishAnimation();
    const target = this.n - 1 - 2 * move.layer;
    const stickers: number[] = [];
    for (let i = 0; i < this.base.length; i++) if (this.cubie[3 * i + move.axis] === target) stickers.push(i);
    const angle = move.turns === 3 ? Math.PI / 2 : (-Math.PI / 2) * move.turns;
    this.anim = { stickers, axis: move.axis, angle, t0: performance.now(), ms, done };
  }

  finishAnimation() {
    const a = this.anim;
    if (!a || !this.mesh) return;
    this.anim = null;
    for (const i of a.stickers) this.mesh.setMatrixAt(i, this.base[i] as THREE.Matrix4);
    this.mesh.instanceMatrix.needsUpdate = true;
    a.done();
  }

  private loop = () => {
    this.frame = requestAnimationFrame(this.loop);
    const a = this.anim;
    if (a && this.mesh) {
      const t = Math.min(1, (performance.now() - a.t0) / a.ms);
      const ease = t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
      const rot = new THREE.Matrix4().makeRotationAxis(AXES[a.axis] as THREE.Vector3, a.angle * ease);
      const m = new THREE.Matrix4();
      for (const i of a.stickers)
        this.mesh.setMatrixAt(i, m.multiplyMatrices(rot, this.base[i] as THREE.Matrix4));
      this.mesh.instanceMatrix.needsUpdate = true;
      if (t >= 1) this.finishAnimation();
    }
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  };

  private dispose3d() {
    for (const obj of [this.mesh, this.body]) {
      if (!obj) continue;
      this.scene.remove(obj);
      obj.geometry.dispose();
      (obj.material as THREE.Material).dispose();
    }
    this.mesh = null;
    this.body = null;
  }

  dispose() {
    cancelAnimationFrame(this.frame);
    this.resize.disconnect();
    this.dispose3d();
    this.controls.dispose();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }
}

// Owns the engine cube and the move queue. Every turn is applied by the engine; the
// 3D stage only animates and then re-syncs its colours from the engine (docs/09 §2).
import { useStore } from "../app/store";
import type { CubeStage } from "../cube3d/CubeStage";
import { type Cube, decodeMoves, encodeMoves, engine, type Move } from "./engine";
import { SOLVABLE_SIZES, solverWorker } from "./solver";

/** Kociemba stops at the first solution this short, or when the budget runs out. */
const TARGET_LENGTH = 20;
const SEARCH_BUDGET_MS = 1000;

const sameFacelets = (a: Uint8Array, b: Uint8Array) => a.length === b.length && a.every((v, i) => v === b[i]);

interface QueueItem {
  move: Move;
  kind: "do" | "undo";
  fast: boolean;
}

export class CubeController {
  private cube: Cube;
  private stage: CubeStage | null = null;
  private queue: QueueItem[] = [];
  private history: Move[] = [];
  private planned: Move[] = [];
  private running = false;
  private generation = 0;
  private solving = false;

  constructor(n: number) {
    this.cube = engine.newCube(n);
    this.publish();
  }

  get n(): number {
    return this.cube.n;
  }

  attachStage(stage: CubeStage | null): void {
    this.stage = stage;
    this.rebuildStage();
  }

  setSize(n: number): void {
    if (n === this.n) return;
    this.generation++;
    this.queue = [];
    this.history = [];
    this.planned = [];
    this.cube.free();
    this.cube = engine.newCube(n);
    useStore.setState({ n, lastScramble: "", error: null, solve: { kind: "idle" } });
    this.rebuildStage();
    this.publish();
  }

  /** Parse and queue an algorithm typed by the user. Returns false on a parse error. */
  applyAlg(alg: string): boolean {
    try {
      const moves = engine.parse(this.n, alg);
      useStore.setState({ error: null });
      this.enqueue(moves);
      return true;
    } catch (e) {
      useStore.setState({ error: e instanceof Error ? e.message : String(e) });
      return false;
    }
  }

  enqueue(moves: readonly Move[], fast = false): void {
    for (const move of moves) {
      this.queue.push({ move, kind: "do", fast });
      this.planned.push(move);
    }
    this.publish();
    void this.pump();
  }

  scramble(): void {
    if (this.solving) return;
    this.reset();
    const seed = crypto.getRandomValues(new BigUint64Array(1))[0] ?? 1n;
    const moves = engine.scramble(this.n, seed);
    useStore.setState({ lastScramble: engine.format(this.n, moves) });
    this.enqueue(moves, true);
  }

  undo(): void {
    // `planned` is the history as it will be once the queue drains.
    const last = this.planned.pop();
    if (!last) return;
    const [inverse] = engine.invert([last]);
    if (inverse) this.queue.push({ move: inverse, kind: "undo", fast: false });
    this.publish();
    void this.pump();
  }

  reset(): void {
    this.generation++;
    this.queue = [];
    this.history = [];
    this.planned = [];
    this.cube.reset();
    useStore.setState({ lastScramble: "", error: null, solve: { kind: "idle" } });
    this.rebuildStage();
    this.publish();
  }

  /**
   * Solve the cube's current state (not its move history) in the solver worker, then
   * animate the solution. Results are discarded if the cube changed during the search.
   */
  async solve(): Promise<void> {
    const n = this.n;
    if (!SOLVABLE_SIZES.has(n) || this.solving || this.queue.length > 0 || this.running) return;
    if (this.cube.isSolved()) return;
    const facelets = this.cube.facelets();
    const generation = this.generation;
    this.solving = true;
    try {
      const worker = solverWorker();
      useStore.setState({ solve: { kind: "preparing", n } });
      const tableMs = await worker.prepare(n);
      useStore.setState({ solve: { kind: "searching", n } });
      const r = await worker.solve(n, facelets, TARGET_LENGTH, SEARCH_BUDGET_MS);
      const unchanged =
        generation === this.generation &&
        this.n === n &&
        this.queue.length === 0 &&
        sameFacelets(facelets, this.cube.facelets());
      if (!unchanged) {
        useStore.setState({
          solve: { kind: "error", message: "The cube changed while solving. Press Solve again." },
        });
        return;
      }
      const moves = decodeMoves(r.moves);
      useStore.setState({
        solve: {
          kind: "done",
          n,
          text: engine.format(n, moves),
          length: moves.length,
          algorithm: r.algorithm,
          optimal: r.optimal,
          phaseLengths: r.phaseLengths,
          nodes: r.nodes,
          searchMs: r.searchMs,
          tableMs,
        },
      });
      this.enqueue(moves);
    } catch (e) {
      useStore.setState({ solve: { kind: "error", message: e instanceof Error ? e.message : String(e) } });
    } finally {
      this.solving = false;
    }
  }

  private rebuildStage(): void {
    this.stage?.build(
      this.n,
      engine.stickerPositions(this.n),
      engine.cubieCentres(this.n),
      this.cube.facelets(),
    );
  }

  private commit(item: QueueItem): Uint8Array {
    this.cube.applyEncoded(encodeMoves([item.move]));
    if (item.kind === "do") this.history.push(item.move);
    else this.history.pop();
    return this.cube.facelets();
  }

  private duration(item: QueueItem): number {
    const base = useStore.getState().speedMs;
    const catchUp = base / (1 + this.queue.length * 0.35);
    const ms = item.fast ? Math.min(base, 70) : catchUp;
    return item.move.turns === 2 ? ms * 1.35 : ms;
  }

  private async pump(): Promise<void> {
    if (this.running) return;
    this.running = true;
    const gen = this.generation;
    try {
      while (gen === this.generation) {
        const item = this.queue.shift();
        if (!item) break;
        const ms = this.duration(item);
        if (this.stage && ms > 0) {
          await this.stage.animate(item.move, ms, () => this.commit(item));
        } else {
          const f = this.commit(item);
          this.stage?.setFacelets(f);
        }
        if (gen === this.generation) this.publish();
      }
    } finally {
      this.running = false;
    }
    if (this.queue.length > 0) void this.pump();
  }

  private publish(): void {
    useStore.setState({
      n: this.n,
      facelets: this.cube.facelets(),
      history: [...this.history],
      pending: this.queue.length,
      solved: this.cube.isSolved(),
    });
  }
}

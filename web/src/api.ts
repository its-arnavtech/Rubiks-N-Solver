// Client for the local API (CONVENTIONS §8). In dev, Vite proxies /api to 127.0.0.1:8000.

export interface Health {
  status: string;
  checkpoint: string | null;
  library_sha256: string;
  device: string;
  nn_available?: boolean;
  note?: string | null;
  max_n?: number;
}

export interface Segment {
  phase: "parity" | "core_frame" | "core" | "orbits" | "fallback";
  orbit_id: number | null;
  orbit_type: string | null;
  round: number | null;
  action_id: number | null;
  q: number | null;
  start: number;
  end: number;
}

export interface SolveResult {
  n: number;
  solver: "nn" | "baseline";
  verified: boolean;
  moves_b64: string;
  cancelled_moves_b64?: string;
  raw_len: number;
  cancelled_len: number;
  segments: Segment[];
  stats: { orbits_total: number; orbits_fallback: number; rounds: number };
  timings_ms: Record<string, number>;
  checkpoint: string | null;
  library_sha256: string;
}

export interface OrbitInfo {
  id: number;
  type: string;
  indices: Record<string, number>;
}

async function call<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: { "Content-Type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!res.ok) {
    let detail = res.statusText;
    try {
      const body = (await res.json()) as { detail?: unknown };
      if (body.detail) detail = String(body.detail);
    } catch {
      // not JSON
    }
    throw new Error(`${res.status}: ${detail}`);
  }
  return (await res.json()) as T;
}

export const api = {
  health: () => call<Health>("/api/health"),
  scramble: (n: number, mode: "random_state" | "moves", seed: number, length?: number) =>
    call<{ n: number; facelets_b64: string; scramble_moves_b64: string | null }>("/api/scramble", {
      method: "POST",
      body: JSON.stringify({ n, mode, seed, length }),
    }),
  solve: (n: number, facelets_b64: string, solver: "nn" | "baseline", beam: number) =>
    call<SolveResult>("/api/solve", {
      method: "POST",
      body: JSON.stringify({ n, facelets_b64, solver, beam }),
    }),
  orbits: (n: number) => call<{ orbits: OrbitInfo[]; sticker_orbit_b64: string }>(`/api/orbits/${n}`),
};

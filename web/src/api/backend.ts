import type { OddsResult, SimInput, VariantInfo } from '../types/odds';
import type { SolverConfigDto, SolverProgress, SolverResult } from '../types/solver';

// ── Backend interface ────────────────────────────────────────────────────────

export interface Backend {
  calculateOdds(input: SimInput): Promise<OddsResult>;
  getVariants(): Promise<VariantInfo[]>;
  validateCard(card: string): Promise<boolean>;

  // Solver (Tauri only)
  hasSolver: boolean;
  validateRange(range: string): Promise<number>;
  startSolve(config: SolverConfigDto): Promise<string>;
  cancelSolve(solveId: string): Promise<void>;
  onSolverProgress(cb: (p: SolverProgress) => void): () => void;
  onSolverComplete(cb: (r: SolverResult) => void): () => void;

  // Lifecycle
  destroy(): void;
}

// ── Runtime detection & singleton ────────────────────────────────────────────

let instance: Backend | null = null;

function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function getBackend(): Promise<Backend> {
  if (instance) return instance;

  if (isTauri()) {
    const { TauriBackend } = await import('./tauri-backend');
    instance = new TauriBackend();
  } else {
    const { WasmBackend } = await import('./wasm-backend');
    instance = new WasmBackend();
  }

  return instance;
}

export function isTauriEnv(): boolean {
  return isTauri();
}

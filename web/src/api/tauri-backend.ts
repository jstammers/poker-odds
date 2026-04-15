import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { Backend } from './backend';
import type { OddsResult, SimInput, VariantInfo } from '../types/odds';
import type { SolverConfigDto, SolverProgress, SolverResult } from '../types/solver';

/**
 * Tauri backend: invokes Rust commands via IPC.
 * Solver is available natively.
 */
export class TauriBackend implements Backend {
  hasSolver = true;

  async calculateOdds(input: SimInput): Promise<OddsResult> {
    return invoke<OddsResult>('calculate_odds', { input });
  }

  async getVariants(): Promise<VariantInfo[]> {
    return invoke<VariantInfo[]>('get_variants');
  }

  async validateCard(card: string): Promise<boolean> {
    return invoke<boolean>('validate_card', { card });
  }

  async validateRange(range: string): Promise<number> {
    return invoke<number>('validate_range', { range });
  }

  async startSolve(config: SolverConfigDto): Promise<string> {
    return invoke<string>('start_solve', { config });
  }

  async cancelSolve(solveId: string): Promise<void> {
    return invoke<void>('cancel_solve', { solveId });
  }

  onSolverProgress(cb: (p: SolverProgress) => void): () => void {
    let unlisten: UnlistenFn | null = null;
    listen<SolverProgress>('solver-progress', (event) => {
      cb(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }

  onSolverComplete(cb: (r: SolverResult) => void): () => void {
    let unlisten: UnlistenFn | null = null;
    listen<SolverResult>('solver-complete', (event) => {
      cb(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }

  destroy(): void {
    // Nothing to clean up
  }
}

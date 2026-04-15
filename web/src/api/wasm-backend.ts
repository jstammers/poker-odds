import type { Backend } from './backend';
import type { OddsResult, SimInput, VariantInfo, WorkerRequest, WorkerResponse } from '../types/odds';
import { isError } from '../types/odds';

const DEFAULT_VARIANTS: VariantInfo[] = [
  { id: 'texas_holdem', name: "Texas Hold'em", description: '2 hole cards + 5 community.', hole_card_count: 2, community_card_count: 5, max_players: 9 },
  { id: 'omaha_holdem', name: "Omaha Hold'em", description: '4 hole cards + 5 community.', hole_card_count: 4, community_card_count: 5, max_players: 9 },
  { id: 'seven_card_stud', name: '7-Card Stud', description: '7 individual cards, no board.', hole_card_count: 7, community_card_count: 0, max_players: 7 },
  { id: 'five_card_draw', name: '5-Card Draw', description: '5 hole cards, no board.', hole_card_count: 5, community_card_count: 0, max_players: 6 },
];

let reqCounter = 0;

/**
 * WASM backend: uses a Web Worker for off-thread simulation.
 * Solver is not available in WASM mode.
 */
export class WasmBackend implements Backend {
  private worker: Worker;
  private pending = new Map<number, { resolve: (r: OddsResult) => void; reject: (e: Error) => void }>();

  hasSolver = false;

  constructor() {
    this.worker = new Worker(new URL('../workers/sim.worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<WorkerResponse>) => {
      const { id, result } = e.data;
      const p = this.pending.get(id);
      if (!p) return;
      this.pending.delete(id);
      if (isError(result)) {
        p.reject(new Error(result.error));
      } else {
        p.resolve(result);
      }
    };
    this.worker.onerror = (e) => {
      // Reject all pending
      for (const [, p] of this.pending) {
        p.reject(new Error(e.message ?? 'Worker error'));
      }
      this.pending.clear();
    };
  }

  calculateOdds(input: SimInput): Promise<OddsResult> {
    return new Promise((resolve, reject) => {
      const id = ++reqCounter;
      this.pending.set(id, { resolve, reject });
      const req: WorkerRequest = { id, input };
      this.worker.postMessage(req);
    });
  }

  async getVariants(): Promise<VariantInfo[]> {
    return DEFAULT_VARIANTS;
  }

  async validateCard(card: string): Promise<boolean> {
    // Basic client-side validation
    const ranks = '23456789TJQKA';
    const suits = 'cdhs';
    return card.length === 2 && ranks.includes(card[0]) && suits.includes(card[1]);
  }

  // Solver stubs (not available in WASM)
  async validateRange(): Promise<number> {
    throw new Error('Solver not available in web mode');
  }
  async startSolve(): Promise<string> {
    throw new Error('Solver not available in web mode');
  }
  async cancelSolve(): Promise<void> {
    throw new Error('Solver not available in web mode');
  }
  onSolverProgress(): () => void {
    return () => {};
  }
  onSolverComplete(): () => void {
    return () => {};
  }

  destroy(): void {
    this.worker.terminate();
  }
}

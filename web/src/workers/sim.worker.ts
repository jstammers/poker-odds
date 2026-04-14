/**
 * Web Worker: loads the WASM module and runs simulations off the main thread.
 * The main thread sends WorkerRequest messages and gets WorkerResponse back.
 */
import type { WorkerRequest, WorkerResponse } from '../types/odds';

// Lazily initialised — the first message triggers init
let wasmReady: Promise<void> | null = null;
let calculate_odds_fn: ((json: string) => string) | null = null;

async function ensureInit() {
  if (wasmReady) return wasmReady;
  wasmReady = (async () => {
    // Bundler-target wasm-pack output self-initialises synchronously the moment
    // the module is imported — wasm.__wbindgen_start() is called at module load.
    // There is no default-export init function; just await the dynamic import.
    const mod = await import('poker-odds-wasm');
    calculate_odds_fn = mod.calculate_odds;
  })();
  return wasmReady;
}

self.onmessage = async (e: MessageEvent<WorkerRequest>) => {
  const { id, input } = e.data;
  try {
    await ensureInit();
    const json = calculate_odds_fn!(JSON.stringify(input));
    const result = JSON.parse(json);
    const resp: WorkerResponse = { id, result };
    self.postMessage(resp);
  } catch (err) {
    const resp: WorkerResponse = {
      id,
      result: { error: err instanceof Error ? err.message : String(err) },
    };
    self.postMessage(resp);
  }
};

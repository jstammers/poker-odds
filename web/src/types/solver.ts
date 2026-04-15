// ── Solver configuration ─────────────────────────────────────────────────────

export type Street = 'flop' | 'turn' | 'river';

export function streetBoardCount(s: Street): number {
  return s === 'flop' ? 3 : s === 'turn' ? 4 : 5;
}

export interface SolverConfigDto {
  board: string[];
  range_oop: string;
  range_ip: string;
  algorithm: 'cfr_plus' | 'dcfr';
  iterations: number;
  starting_pot: number;
  effective_stack: number;
  flop_bets: number[];
  flop_raises: number[];
  turn_bets: number[];
  turn_raises: number[];
  river_bets: number[];
  river_raises: number[];
  max_raises: number;
}

// ── Progress & results ───────────────────────────────────────────────────────

export interface SolverProgress {
  solve_id: string;
  iteration: number;
  total: number;
  game_value: number;
}

export interface StrategyEntry {
  label: string;
  actions: ActionProb[];
}

export interface ActionProb {
  name: string;
  probability: number;
}

export interface SolverResult {
  solve_id: string;
  game_value: number;
  exploitability_mbb: number;
  num_info_sets: number;
  num_nodes: number;
  strategies: StrategyEntry[];
}

// ── Default config ───────────────────────────────────────────────────────────

export function defaultSolverConfig(): SolverConfigDto {
  return {
    board: [],
    range_oop: '',
    range_ip: '',
    algorithm: 'cfr_plus',
    iterations: 1_000,
    starting_pot: 100,
    effective_stack: 200,
    flop_bets: [0.33, 0.67, 1.0],
    flop_raises: [0.5, 1.0],
    turn_bets: [0.5, 0.75, 1.0],
    turn_raises: [0.5, 1.0],
    river_bets: [0.5, 0.75, 1.0],
    river_raises: [0.5, 1.0],
    max_raises: 3,
  };
}

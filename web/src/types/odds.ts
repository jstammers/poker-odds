export type Suit = 'c' | 'd' | 'h' | 's';
export type Rank = '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' | 'T' | 'J' | 'Q' | 'K' | 'A';

export const RANKS: Rank[] = ['2','3','4','5','6','7','8','9','T','J','Q','K','A'];
export const SUITS: Suit[] = ['s','h','d','c'];

export const SUIT_GLYPHS: Record<Suit, string> = { c: '♣', d: '♦', h: '♥', s: '♠' };
export const SUIT_LABELS: Record<Suit, string> = { c: 'Clubs', d: 'Diamonds', h: 'Hearts', s: 'Spades' };

export function cardId(rank: Rank, suit: Suit): string {
  return `${rank}${suit}`;
}

export function parseCard(id: string): { rank: Rank; suit: Suit } | null {
  if (id.length !== 2) return null;
  const rank = id[0] as Rank;
  const suit = id[1] as Suit;
  if (!RANKS.includes(rank) || !SUITS.includes(suit)) return null;
  return { rank, suit };
}

// ── Variant info returned by get_variants() ──────────────────────────────────

export interface VariantInfo {
  id: string;
  name: string;
  description: string;
  hole_card_count: number;
  community_card_count: number;
  max_players: number;
}

// ── Simulation I/O ────────────────────────────────────────────────────────────

export interface SimInput {
  variant: string;
  hole_cards: string[];
  community_cards: string[];
  opponent_count: number;
  iterations?: number;
  exact_threshold?: number;
  rng_seed?: number;
}

export interface OddsResult {
  win: number;
  tie: number;
  lose: number;
  simulations_run: number;
  method: string;
  hand_distribution: Record<string, number>;
}

export interface OddsError {
  error: string;
}

export type SimResult = OddsResult | OddsError;

export function isError(r: SimResult): r is OddsError {
  return 'error' in r;
}

// ── Worker message shapes ─────────────────────────────────────────────────────

export interface WorkerRequest {
  id: number;
  input: SimInput;
}

export interface WorkerResponse {
  id: number;
  result: SimResult;
}

// ── Hand categories (in order from weakest to strongest) ─────────────────────

export const HAND_CATEGORIES = [
  'High Card',
  'One Pair',
  'Two Pair',
  'Three of a Kind',
  'Straight',
  'Flush',
  'Full House',
  'Four of a Kind',
  'Straight Flush',
  'Royal Flush',
] as const;

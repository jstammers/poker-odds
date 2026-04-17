//! Simulation engine: Monte Carlo and exact enumeration.

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use itertools::Itertools;
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::cards::{Card, Deck};
use crate::eval::{best_five_of_n, best_five_of_seven, evaluate_omaha, HandValue};
use crate::game::{GameState, GameVariant};
use crate::sim::config::SimConfig;
use crate::sim::result::{OddsResult, SimAccumulator, SimMethod};

pub type CancelFlag = Arc<AtomicBool>;

/// Run the simulation and return the final OddsResult.
/// Calls `progress_cb` periodically with partial results.
pub fn run_simulation<F>(
    state: &GameState,
    config: &SimConfig,
    cancel: CancelFlag,
    mut progress_cb: F,
) -> OddsResult
where
    F: FnMut(OddsResult),
{
    if !state.hole_cards_complete() {
        return OddsResult::default();
    }

    // Build the deck with known cards removed.
    let mut deck = Deck::new();
    for &c in state.known_cards().iter() {
        deck.remove(c);
    }

    let remaining = deck.remaining_count() as u64;
    let cards_needed = state.cards_to_simulate() as u32;
    let exact_combos = if cards_needed <= remaining as u32 {
        combinations_count(remaining as usize, cards_needed as usize)
    } else {
        u64::MAX
    };

    if exact_combos <= config.exact_threshold {
        run_exact(state, deck, cancel, config, &mut progress_cb)
    } else {
        run_monte_carlo(state, deck, config, cancel, &mut progress_cb)
    }
}

// ── Monte Carlo ───────────────────────────────────────────────────────────────

fn run_monte_carlo<F>(
    state: &GameState,
    deck: Deck,
    config: &SimConfig,
    cancel: CancelFlag,
    progress_cb: &mut F,
) -> OddsResult
where
    F: FnMut(OddsResult),
{
    // On native builds, use all available cores via Rayon when there is more
    // than one thread available.
    #[cfg(not(target_arch = "wasm32"))]
    if config.effective_threads() > 1 {
        return run_monte_carlo_parallel(state, deck, config, cancel, progress_cb);
    }

    run_monte_carlo_single(state, deck, config, cancel, progress_cb)
}

/// Single-threaded Monte Carlo (used on WASM and when threads == 1).
fn run_monte_carlo_single<F>(
    state: &GameState,
    deck: Deck,
    config: &SimConfig,
    cancel: CancelFlag,
    progress_cb: &mut F,
) -> OddsResult
where
    F: FnMut(OddsResult),
{
    let iterations = config.iterations;
    let update_every = (iterations / 20).max(1_000);
    let seed = config.rng_seed.unwrap_or_else(rand::random::<u64>);

    let mut acc = SimAccumulator::default();
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);

    for i in 0..iterations {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let mut sim_deck = deck.snapshot();
        simulate_one(state, &mut sim_deck, &mut rng, &mut acc);

        if i > 0 && i % update_every == 0 {
            progress_cb(acc.to_result(SimMethod::MonteCarlo));
        }
    }

    acc.to_result(SimMethod::MonteCarlo)
}

/// Multi-threaded Monte Carlo using Rayon (native only).
///
/// Runs in chunks of `update_every` iterations so the TUI still receives
/// progress callbacks at a reasonable cadence.
#[cfg(not(target_arch = "wasm32"))]
fn run_monte_carlo_parallel<F>(
    state: &GameState,
    deck: Deck,
    config: &SimConfig,
    cancel: CancelFlag,
    progress_cb: &mut F,
) -> OddsResult
where
    F: FnMut(OddsResult),
{
    use rayon::prelude::*;

    let n_threads = config.effective_threads();
    let iterations = config.iterations;
    // Align chunk size to a multiple of n_threads so work is always balanced.
    let chunk_size = {
        let raw = (iterations / 20).max(n_threads as u64 * 100);
        // Round up to nearest multiple of n_threads
        let m = n_threads as u64;
        raw.div_ceil(m) * m
    };
    let seed = config.rng_seed.unwrap_or_else(rand::random::<u64>);

    let mut total_acc = SimAccumulator::default();
    let mut processed = 0u64;

    while processed < iterations {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let chunk = chunk_size.min(iterations - processed);
        let per_thread = chunk / n_threads as u64;
        let extra = chunk % n_threads as u64; // distributed to first `extra` threads
                                              // Use different seed offsets per chunk and per thread so each thread's
                                              // RNG stream is statistically independent.
        let chunk_seed = seed.wrapping_add(processed);

        // Each thread accumulates independently — no locks in the hot path.
        let accs: Vec<SimAccumulator> = (0..n_threads as u64)
            .into_par_iter()
            .map(|t| {
                let thread_iters = per_thread + if t < extra { 1 } else { 0 };
                // Combine chunk seed with a thread-index hash to get independent streams.
                let thread_seed = chunk_seed ^ t.wrapping_mul(0x9e3779b97f4a7c15);
                let mut rng = Xoshiro256PlusPlus::seed_from_u64(thread_seed);
                let mut acc = SimAccumulator::default();
                for _ in 0..thread_iters {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut sim_deck = deck.snapshot();
                    simulate_one(state, &mut sim_deck, &mut rng, &mut acc);
                }
                acc
            })
            .collect();

        for a in &accs {
            total_acc.merge(a);
        }
        processed += chunk;
        progress_cb(total_acc.to_result(SimMethod::MonteCarlo));
    }

    total_acc.to_result(SimMethod::MonteCarlo)
}

// ── Exact enumeration ─────────────────────────────────────────────────────────

fn run_exact<F>(
    state: &GameState,
    deck: Deck,
    cancel: CancelFlag,
    config: &SimConfig,
    progress_cb: &mut F,
) -> OddsResult
where
    F: FnMut(OddsResult),
{
    let remaining: Vec<Card> = deck.remaining_cards().collect();
    let cards_needed = state.cards_to_simulate();

    let mut acc = SimAccumulator::default();
    let update_every = 500usize;
    let mut count = 0usize;

    for combo in remaining.iter().combinations(cards_needed) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }

        let combo_cards: Vec<Card> = combo.into_iter().copied().collect();
        evaluate_exact_combo(state, &combo_cards, &mut acc);

        count += 1;
        if count.is_multiple_of(update_every) {
            progress_cb(acc.to_result(SimMethod::Exact));
        }
    }

    let _ = config;
    acc.to_result(SimMethod::Exact)
}

// ── Hot-path simulation helpers ───────────────────────────────────────────────

/// Simulate one random runout, recording the outcome — **zero heap allocations**.
///
/// Community and opponent cards are buffered on the stack (max 5 community,
/// max 7 hole cards per opponent).
#[inline]
fn simulate_one<R: rand::Rng>(
    state: &GameState,
    deck: &mut Deck,
    rng: &mut R,
    acc: &mut SimAccumulator,
) {
    // ── Community cards ────────────────────────────────────────────────────────
    // Stack buffer: Hold'em/Omaha have up to 5; Stud/Draw have 0.
    let community_known = state.community_cards.len();
    let community_needed = state.community_cards_remaining();
    let community_total = community_known + community_needed;

    let mut community_buf = [Card::from_index(0); 5];
    // Copy the known portion (avoids cloning the Vec).
    community_buf[..community_known].copy_from_slice(&state.community_cards);
    // Deal the unknown community cards.
    for slot in &mut community_buf[community_known..community_total] {
        match deck.deal_random(rng) {
            Some(c) => *slot = c,
            None => return, // deck exhausted — shouldn't happen with valid input
        }
    }
    let community = &community_buf[..community_total];

    // ── Player evaluation ──────────────────────────────────────────────────────
    let player_value = eval_hand(state.variant, &state.hole_cards, community);
    let player_cat = player_value.category();

    // ── Opponent evaluations ───────────────────────────────────────────────────
    // Stack buffer: max 7 hole cards (7-Card Stud).
    let hole_per_opp = state.unknown_hole_cards_per_opponent();
    let mut opp_buf = [Card::from_index(0); 7];
    let mut best_opp = HandValue(0);

    for _ in 0..state.opponent_count {
        for slot in &mut opp_buf[..hole_per_opp] {
            match deck.deal_random(rng) {
                Some(c) => *slot = c,
                None => return,
            }
        }
        let opp_val = eval_hand(state.variant, &opp_buf[..hole_per_opp], community);
        if opp_val > best_opp {
            best_opp = opp_val;
        }
    }

    // ── Record outcome ─────────────────────────────────────────────────────────
    if player_value > best_opp {
        acc.record_win(player_cat);
    } else if player_value == best_opp {
        acc.record_tie(player_cat);
    } else {
        acc.record_loss(player_cat);
    }
}

/// Exact enumeration variant — combo is already dealt; uses stack buffer for community.
fn evaluate_exact_combo(state: &GameState, combo: &[Card], acc: &mut SimAccumulator) {
    let community_known = state.community_cards.len();
    let community_needed = state.community_cards_remaining();
    let community_total = community_known + community_needed;

    let mut community_buf = [Card::from_index(0); 5];
    community_buf[..community_known].copy_from_slice(&state.community_cards);
    community_buf[community_known..community_total].copy_from_slice(&combo[..community_needed]);
    let community = &community_buf[..community_total];

    let player_value = eval_hand(state.variant, &state.hole_cards, community);
    let player_cat = player_value.category();

    let hole_per_opp = state.unknown_hole_cards_per_opponent();
    let mut offset = community_needed;
    let mut best_opp = HandValue(0);

    for _ in 0..state.opponent_count {
        let opp_hand = &combo[offset..offset + hole_per_opp];
        offset += hole_per_opp;
        let opp_val = eval_hand(state.variant, opp_hand, community);
        if opp_val > best_opp {
            best_opp = opp_val;
        }
    }

    if player_value > best_opp {
        acc.record_win(player_cat);
    } else if player_value == best_opp {
        acc.record_tie(player_cat);
    } else {
        acc.record_loss(player_cat);
    }
}

/// Evaluate a hand for any game variant given hole cards and community cards.
#[inline]
fn eval_hand(variant: GameVariant, hole: &[Card], community: &[Card]) -> HandValue {
    match variant {
        GameVariant::TexasHoldem => {
            if community.len() >= 5 && hole.len() >= 2 {
                let cards = [
                    hole[0],
                    hole[1],
                    community[0],
                    community[1],
                    community[2],
                    community[3],
                    community[4],
                ];
                best_five_of_seven(&cards)
            } else {
                let mut all: Vec<Card> = hole.to_vec();
                all.extend_from_slice(community);
                if all.len() >= 5 {
                    best_five_of_n(&all)
                } else {
                    HandValue(0)
                }
            }
        }
        GameVariant::OmahaHoldem => {
            if community.len() >= 5 && hole.len() >= 4 {
                let hole_arr = [hole[0], hole[1], hole[2], hole[3]];
                let board_arr = [
                    community[0],
                    community[1],
                    community[2],
                    community[3],
                    community[4],
                ];
                evaluate_omaha(&hole_arr, &board_arr)
            } else {
                HandValue(0)
            }
        }
        GameVariant::SevenCardStud | GameVariant::FiveCardDraw => {
            if hole.len() >= 5 {
                best_five_of_n(hole)
            } else {
                HandValue(0)
            }
        }
    }
}

fn combinations_count(n: usize, k: usize) -> u64 {
    if k > n {
        return 0;
    }
    if k == 0 {
        return 1;
    }
    let k = k.min(n - k);
    let mut result = 1u64;
    for i in 0..k {
        result = result.saturating_mul((n - i) as u64);
        result /= (i + 1) as u64;
    }
    result
}

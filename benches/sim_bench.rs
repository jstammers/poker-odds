use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use poker_odds::cards::card::{Card, Rank, Suit};
use poker_odds::game::{GameState, GameVariant};
use poker_odds::sim::{run_simulation, SimConfig};
use std::sync::{atomic::AtomicBool, Arc};

// ── Helpers ───────────────────────────────────────────────────────────────────

fn card(r: Rank, s: Suit) -> Card {
    Card::new(r, s)
}

fn cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

/// Texas Hold'em pre-flop — most community cards unknown, maximum simulation work.
fn bench_holdem_preflop(c: &mut Criterion) {
    let mut state = GameState::new(GameVariant::TexasHoldem);
    state.hole_cards = vec![
        card(Rank::Ace, Suit::Spades),
        card(Rank::King, Suit::Spades),
    ];
    state.opponent_count = 1;

    let mut group = c.benchmark_group("holdem_preflop_1opp");
    for iters in [10_000u64, 50_000, 100_000] {
        let cfg = SimConfig {
            iterations: iters,
            rng_seed: Some(42),
            ..SimConfig::default()
        };
        group.bench_with_input(BenchmarkId::from_parameter(iters), &cfg, |b, cfg| {
            b.iter(|| run_simulation(black_box(&state), black_box(cfg), cancel(), |_| {}))
        });
    }
    group.finish();
}

/// Texas Hold'em on the river — 5 community cards known, board complete.
/// Each run deals only 2 opponent cards → tighter inner loop.
fn bench_holdem_river(c: &mut Criterion) {
    let mut state = GameState::new(GameVariant::TexasHoldem);
    state.hole_cards = vec![
        card(Rank::Ace, Suit::Spades),
        card(Rank::King, Suit::Spades),
    ];
    state.community_cards = vec![
        card(Rank::Queen, Suit::Spades),
        card(Rank::Jack, Suit::Spades),
        card(Rank::Ten, Suit::Hearts),
        card(Rank::Two, Suit::Clubs),
        card(Rank::Seven, Suit::Diamonds),
    ];
    state.opponent_count = 1;

    let cfg = SimConfig {
        iterations: 100_000,
        rng_seed: Some(42),
        ..SimConfig::default()
    };
    c.bench_function("holdem_river_100k", |b| {
        b.iter(|| run_simulation(black_box(&state), black_box(&cfg), cancel(), |_| {}))
    });
}

/// Multiple opponents — scales the inner eval loop.
fn bench_holdem_multi_opponent(c: &mut Criterion) {
    let mut group = c.benchmark_group("holdem_preflop_opponents");
    for opps in [1usize, 3, 5, 8] {
        let mut state = GameState::new(GameVariant::TexasHoldem);
        state.hole_cards = vec![
            card(Rank::Ace, Suit::Spades),
            card(Rank::King, Suit::Spades),
        ];
        state.opponent_count = opps;
        let cfg = SimConfig {
            iterations: 50_000,
            rng_seed: Some(42),
            ..SimConfig::default()
        };
        group.bench_with_input(BenchmarkId::from_parameter(opps), &cfg, |b, cfg| {
            b.iter(|| run_simulation(black_box(&state), black_box(cfg), cancel(), |_| {}))
        });
    }
    group.finish();
}

/// Omaha Hold'em — must-use-2-of-4 evaluation is heavier (60 combos per hand).
fn bench_omaha_preflop(c: &mut Criterion) {
    let mut state = GameState::new(GameVariant::OmahaHoldem);
    state.hole_cards = vec![
        card(Rank::Ace, Suit::Spades),
        card(Rank::King, Suit::Spades),
        card(Rank::Queen, Suit::Hearts),
        card(Rank::Jack, Suit::Hearts),
    ];
    state.opponent_count = 1;
    let cfg = SimConfig {
        iterations: 50_000,
        rng_seed: Some(42),
        ..SimConfig::default()
    };
    c.bench_function("omaha_preflop_50k", |b| {
        b.iter(|| run_simulation(black_box(&state), black_box(&cfg), cancel(), |_| {}))
    });
}

criterion_group!(
    benches,
    bench_holdem_preflop,
    bench_holdem_river,
    bench_holdem_multi_opponent,
    bench_omaha_preflop,
);
criterion_main!(benches);

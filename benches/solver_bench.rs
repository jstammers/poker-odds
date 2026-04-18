use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use poker_odds::cards::card::{Card, Rank, Suit};
use poker_odds::solver::abstraction::EquityBuckets;
use poker_odds::solver::action::BetSizingConfig;
use poker_odds::solver::cfr::{CfrAlgorithm, CfrSolver, SolverConfig};
use poker_odds::solver::postflop::{build_river_tree, PostflopConfig, PostflopTreeBuilder};
use poker_odds::solver::range::HandRange;
use poker_odds::solver::toy_games::{KuhnPoker, LeducPoker};

fn bench_kuhn_cfr_plus(c: &mut Criterion) {
    let mut group = c.benchmark_group("kuhn_cfr_plus");
    for &iters in &[100, 1000, 10000] {
        group.bench_with_input(BenchmarkId::from_parameter(iters), &iters, |b, &iters| {
            b.iter(|| {
                let tree = KuhnPoker::build_tree();
                let config = SolverConfig {
                    algorithm: CfrAlgorithm::CfrPlus,
                    iterations: iters,
                    ..Default::default()
                };
                let mut solver = CfrSolver::new(tree, config);
                black_box(solver.solve())
            });
        });
    }
    group.finish();
}

fn bench_kuhn_dcfr(c: &mut Criterion) {
    let mut group = c.benchmark_group("kuhn_dcfr");
    for &iters in &[100, 1000, 10000] {
        group.bench_with_input(BenchmarkId::from_parameter(iters), &iters, |b, &iters| {
            b.iter(|| {
                let tree = KuhnPoker::build_tree();
                let config = SolverConfig {
                    algorithm: CfrAlgorithm::Dcfr,
                    iterations: iters,
                    ..Default::default()
                };
                let mut solver = CfrSolver::new(tree, config);
                black_box(solver.solve())
            });
        });
    }
    group.finish();
}

fn bench_leduc_cfr_plus(c: &mut Criterion) {
    let mut group = c.benchmark_group("leduc_cfr_plus");
    group.sample_size(10); // Leduc is slower, reduce sample count
    for &iters in &[100, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(iters), &iters, |b, &iters| {
            b.iter(|| {
                let tree = LeducPoker::build_tree();
                let config = SolverConfig {
                    algorithm: CfrAlgorithm::CfrPlus,
                    iterations: iters,
                    ..Default::default()
                };
                let mut solver = CfrSolver::new(tree, config);
                black_box(solver.solve())
            });
        });
    }
    group.finish();
}

fn bench_river_simple(c: &mut Criterion) {
    let board = [
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::King, Suit::Hearts),
        Card::new(Rank::Queen, Suit::Diamonds),
        Card::new(Rank::Seven, Suit::Clubs),
        Card::new(Rank::Two, Suit::Spades),
    ];
    let bet_config = BetSizingConfig {
        river_bets: vec![0.5, 1.0],
        river_raises: vec![1.0],
        always_allow_allin: false,
        max_raises_per_street: 1,
        ..Default::default()
    };

    let mut group = c.benchmark_group("river_simple");
    for &iters in &[100, 1000] {
        group.bench_with_input(BenchmarkId::from_parameter(iters), &iters, |b, &iters| {
            b.iter(|| {
                let tree = build_river_tree(board, 100.0, 200.0, bet_config.clone());
                let config = SolverConfig {
                    algorithm: CfrAlgorithm::CfrPlus,
                    iterations: iters,
                    ..Default::default()
                };
                let mut solver = CfrSolver::new(tree, config);
                black_box(solver.solve())
            });
        });
    }
    group.finish();
}

/// Stress benchmark for the CFR traversal hot path. Uses a deeper river tree
/// with more bet sizes and allowed raises so the solver hits more decision
/// nodes per iteration — this is where per-node heap allocations show up.
///
/// Reuse the same tree across iterations to isolate traversal cost from
/// tree-building cost.
/// DCFR stress benchmark. DCFR applies a discount sweep over the entire flat
/// regret and strategy-sum arrays every iteration; on larger trees this pass
/// becomes a meaningful fraction of runtime and benefits from parallelism.
fn bench_river_dcfr_hot(c: &mut Criterion) {
    let board = [
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::King, Suit::Hearts),
        Card::new(Rank::Queen, Suit::Diamonds),
        Card::new(Rank::Seven, Suit::Clubs),
        Card::new(Rank::Two, Suit::Spades),
    ];
    let bet_config = BetSizingConfig {
        river_bets: vec![0.33, 0.67, 1.0],
        river_raises: vec![1.0, 2.0],
        always_allow_allin: true,
        max_raises_per_street: 2,
        ..Default::default()
    };

    let mut group = c.benchmark_group("river_dcfr_hot");
    group.sample_size(10);
    for &iters in &[500, 2000] {
        group.bench_with_input(BenchmarkId::from_parameter(iters), &iters, |b, &iters| {
            b.iter_batched(
                || {
                    let tree = build_river_tree(board, 100.0, 200.0, bet_config.clone());
                    let config = SolverConfig {
                        algorithm: CfrAlgorithm::Dcfr,
                        iterations: iters,
                        ..Default::default()
                    };
                    CfrSolver::new(tree, config)
                },
                |mut solver| black_box(solver.solve()),
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_river_traversal_hot(c: &mut Criterion) {
    let board = [
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::King, Suit::Hearts),
        Card::new(Rank::Queen, Suit::Diamonds),
        Card::new(Rank::Seven, Suit::Clubs),
        Card::new(Rank::Two, Suit::Spades),
    ];
    let bet_config = BetSizingConfig {
        river_bets: vec![0.33, 0.67, 1.0],
        river_raises: vec![1.0, 2.0],
        always_allow_allin: true,
        max_raises_per_street: 2,
        ..Default::default()
    };

    let mut group = c.benchmark_group("river_traversal_hot");
    group.sample_size(10);
    for &iters in &[500, 2000] {
        group.bench_with_input(BenchmarkId::from_parameter(iters), &iters, |b, &iters| {
            b.iter_batched(
                || {
                    let tree = build_river_tree(board, 100.0, 200.0, bet_config.clone());
                    let config = SolverConfig {
                        algorithm: CfrAlgorithm::CfrPlus,
                        iterations: iters,
                        ..Default::default()
                    };
                    CfrSolver::new(tree, config)
                },
                |mut solver| black_box(solver.solve()),
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

/// Realistic postflop CFR benchmark on a flop scenario.
///
/// The previous benches cover toy games (Kuhn, Leduc) and single-street river
/// solves. This one exercises the full flop→turn→river tree with equity-bucket
/// abstraction — representative of the workload a real user hits in the TUI
/// solver, and the bed on which vector-CFR and action-pruning optimizations
/// will be measured.
///
/// Kept small (pocket-pair range, coarse 12-bucket abstraction, two bet sizes)
/// so tree construction and one iteration both stay sub-second. Tree is built
/// once; each bench iteration clones it and runs `solve()` from a fresh store.
fn bench_flop_cfr_plus(c: &mut Criterion) {
    let board = [
        Card::new(Rank::Ten, Suit::Spades),
        Card::new(Rank::Seven, Suit::Hearts),
        Card::new(Rank::Two, Suit::Diamonds),
    ];
    let bet_config = BetSizingConfig {
        flop_bets: vec![0.5],
        flop_raises: vec![1.0],
        turn_bets: vec![0.75],
        turn_raises: vec![1.0],
        river_bets: vec![1.0],
        river_raises: vec![1.0],
        always_allow_allin: false,
        max_raises_per_street: 1,
    };
    let range_oop: HandRange = "QQ-TT".parse().expect("valid range");
    let range_ip: HandRange = "JJ-99".parse().expect("valid range");
    let config = PostflopConfig {
        board: board.to_vec(),
        range_oop,
        range_ip,
        starting_pot: 40.0,
        effective_stack: 100.0,
        bet_config,
        abstraction: Box::new(EquityBuckets::new(12)),
    };
    let tree = PostflopTreeBuilder::new(config).build();

    let mut group = c.benchmark_group("flop_cfr_plus");
    group.sample_size(10);
    for &iters in &[100u32, 500] {
        group.bench_with_input(BenchmarkId::from_parameter(iters), &iters, |b, &iters| {
            let solver_config = SolverConfig {
                algorithm: CfrAlgorithm::CfrPlus,
                iterations: iters,
                ..Default::default()
            };
            b.iter_batched(
                || CfrSolver::new(tree.clone(), solver_config.clone()),
                |mut solver| black_box(solver.solve()),
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_exploitability(c: &mut Criterion) {
    use poker_odds::solver::exploitability::compute_exploitability;

    c.bench_function("kuhn_exploitability", |b| {
        let tree = KuhnPoker::build_tree();
        let config = SolverConfig {
            algorithm: CfrAlgorithm::CfrPlus,
            iterations: 1000,
            ..Default::default()
        };
        let mut solver = CfrSolver::new(tree, config);
        solver.solve();

        b.iter(|| black_box(compute_exploitability(&solver.tree, &solver.store, 1.0)));
    });
}

/// Exploitability benchmark on a deeper river tree. The existing Kuhn case is
/// too small for heap-allocation costs in the best-response traversal to show
/// up — this one exercises a tree with more decision nodes and opponent info
/// sets (where pass1/pass2 read the averaged strategy per node).
fn bench_exploitability_river(c: &mut Criterion) {
    use poker_odds::solver::exploitability::compute_exploitability;

    let board = [
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::King, Suit::Hearts),
        Card::new(Rank::Queen, Suit::Diamonds),
        Card::new(Rank::Seven, Suit::Clubs),
        Card::new(Rank::Two, Suit::Spades),
    ];
    let bet_config = BetSizingConfig {
        river_bets: vec![0.33, 0.67, 1.0],
        river_raises: vec![1.0, 2.0],
        always_allow_allin: true,
        max_raises_per_street: 2,
        ..Default::default()
    };

    let tree = build_river_tree(board, 100.0, 200.0, bet_config);
    let config = SolverConfig {
        algorithm: CfrAlgorithm::CfrPlus,
        iterations: 2000,
        ..Default::default()
    };
    let mut solver = CfrSolver::new(tree, config);
    solver.solve();

    c.bench_function("river_exploitability", |b| {
        b.iter(|| black_box(compute_exploitability(&solver.tree, &solver.store, 1.0)));
    });
}

/// Microbenchmark: the DCFR discount pass over a large synthetic flat store.
///
/// Exercises `InfoSetStore::{discount_regrets_all, discount_strategy_sum_all}`
/// in isolation so the parallel-vs-serial contribution is visible without being
/// dwarfed by CFR traversal cost. Sizes bracket the rayon threshold.
fn bench_dcfr_discount_pass(c: &mut Criterion) {
    use poker_odds::solver::info_set::InfoSetStore;

    let mut group = c.benchmark_group("dcfr_discount_pass");
    for &n_info_sets in &[1_000usize, 50_000, 500_000] {
        let actions: Vec<u8> = vec![6; n_info_sets];
        group.bench_with_input(
            BenchmarkId::from_parameter(n_info_sets),
            &actions,
            |b, actions| {
                b.iter_batched(
                    || {
                        let mut store = InfoSetStore::new(actions);
                        // Seed with a mix of positive and negative regrets so the
                        // branch in `discount_regrets_all` exercises both arms.
                        for (i, r) in store.regrets.iter_mut().enumerate() {
                            *r = if i % 2 == 0 { 1.5 } else { -0.75 };
                        }
                        for s in store.strategy_sum.iter_mut() {
                            *s = 0.25;
                        }
                        store
                    },
                    |mut store| {
                        store.discount_regrets_all(black_box(0.95), black_box(0.5));
                        store.discount_strategy_sum_all(black_box(0.9));
                        black_box(store)
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

fn bench_equity_computation(c: &mut Criterion) {
    use poker_odds::solver::abstraction::EquityBuckets;

    let hole = [
        Card::new(Rank::Ace, Suit::Hearts),
        Card::new(Rank::Ace, Suit::Spades),
    ];
    let board = [
        Card::new(Rank::Two, Suit::Clubs),
        Card::new(Rank::Five, Suit::Diamonds),
        Card::new(Rank::Seven, Suit::Hearts),
        Card::new(Rank::Nine, Suit::Spades),
        Card::new(Rank::Three, Suit::Clubs),
    ];

    c.bench_function("river_ehs_exact", |b| {
        b.iter(|| black_box(EquityBuckets::river_ehs(black_box(hole), black_box(&board))))
    });

    let flop_board = [
        Card::new(Rank::Two, Suit::Clubs),
        Card::new(Rank::Five, Suit::Diamonds),
        Card::new(Rank::Seven, Suit::Hearts),
    ];
    c.bench_function("flop_ehs_monte_carlo_200", |b| {
        b.iter(|| {
            black_box(EquityBuckets::monte_carlo_ehs(
                black_box(hole),
                black_box(&flop_board),
                200,
            ))
        })
    });
}

/// Showdown evaluation microbenchmark.
///
/// The vector-CFR optimisation hinges on being able to evaluate a river
/// showdown terminal given per-combo reach vectors for both players. This
/// benchmark measures both the one-off `ShowdownRanker::new` cost (precomputed
/// per terminal) and the repeated `terminal_ev` cost (paid every iteration).
///
/// Reaches are seeded with a realistic range shape — here ~250 non-zero combos
/// from the "QQ-TT, AK, AQs" pocket-pair/broadway matchup — so the result is
/// representative of a solver workload rather than a degenerate all-zero
/// case. A follow-up O(N log N) implementation of `terminal_ev` will be
/// compared against this baseline.
fn bench_showdown(c: &mut Criterion) {
    use poker_odds::solver::range::HandRange;
    use poker_odds::solver::showdown::{ShowdownRanker, N_COMBOS};

    let board = [
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::King, Suit::Hearts),
        Card::new(Rank::Queen, Suit::Diamonds),
        Card::new(Rank::Seven, Suit::Clubs),
        Card::new(Rank::Two, Suit::Spades),
    ];

    c.bench_function("showdown_ranker_build", |b| {
        b.iter(|| black_box(ShowdownRanker::new(black_box(&board))))
    });

    // Seed realistic reach vectors for the terminal EV bench.
    let oop: HandRange = "QQ-TT,AK,AQs".parse().expect("valid range");
    let ip: HandRange = "JJ-88,AQ,AJs,KQs".parse().expect("valid range");
    let mut reach_p0 = [0.0f32; N_COMBOS];
    let mut reach_p1 = [0.0f32; N_COMBOS];
    for (i, &w) in oop.weights.iter().enumerate() {
        reach_p0[i] = w;
    }
    for (i, &w) in ip.weights.iter().enumerate() {
        reach_p1[i] = w;
    }
    let ranker = ShowdownRanker::new(&board);

    c.bench_function("showdown_terminal_ev_naive", |b| {
        b.iter(|| black_box(ranker.terminal_ev_naive(black_box(&reach_p0), black_box(&reach_p1))))
    });

    c.bench_function("showdown_terminal_ev_fast", |b| {
        b.iter(|| black_box(ranker.terminal_ev(black_box(&reach_p0), black_box(&reach_p1))))
    });

    // Full-range stress: every combo has non-zero reach. Worst case for the
    // fast path's per-card prefix pass (nothing prunable via reach == 0).
    let full_reach = [1.0f32; N_COMBOS];
    c.bench_function("showdown_terminal_ev_fast_fullrange", |b| {
        b.iter(|| black_box(ranker.terminal_ev(black_box(&full_reach), black_box(&full_reach))))
    });
}

criterion_group!(
    benches,
    bench_kuhn_cfr_plus,
    bench_kuhn_dcfr,
    bench_leduc_cfr_plus,
    bench_river_simple,
    bench_river_traversal_hot,
    bench_river_dcfr_hot,
    bench_dcfr_discount_pass,
    bench_flop_cfr_plus,
    bench_exploitability,
    bench_exploitability_river,
    bench_equity_computation,
    bench_showdown,
);
criterion_main!(benches);

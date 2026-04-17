use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use poker_odds::cards::card::{Card, Rank, Suit};
use poker_odds::solver::action::BetSizingConfig;
use poker_odds::solver::cfr::{CfrAlgorithm, CfrSolver, SolverConfig};
use poker_odds::solver::postflop::build_river_tree;
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

criterion_group!(
    benches,
    bench_kuhn_cfr_plus,
    bench_kuhn_dcfr,
    bench_leduc_cfr_plus,
    bench_river_simple,
    bench_exploitability,
    bench_equity_computation,
);
criterion_main!(benches);

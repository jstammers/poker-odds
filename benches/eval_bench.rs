use criterion::{black_box, criterion_group, criterion_main, Criterion};
use poker_odds::cards::card::{Card, Rank, Suit};
use poker_odds::eval::{best_five_of_seven, evaluate_five};

fn bench_eval_five(c: &mut Criterion) {
    let cards = [
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::King, Suit::Spades),
        Card::new(Rank::Queen, Suit::Spades),
        Card::new(Rank::Jack, Suit::Spades),
        Card::new(Rank::Ten, Suit::Spades),
    ];
    c.bench_function("evaluate_five", |b| b.iter(|| evaluate_five(black_box(&cards))));
}

fn bench_eval_seven(c: &mut Criterion) {
    let cards = [
        Card::new(Rank::Ace, Suit::Spades),
        Card::new(Rank::King, Suit::Hearts),
        Card::new(Rank::Queen, Suit::Diamonds),
        Card::new(Rank::Jack, Suit::Clubs),
        Card::new(Rank::Ten, Suit::Spades),
        Card::new(Rank::Nine, Suit::Hearts),
        Card::new(Rank::Two, Suit::Diamonds),
    ];
    c.bench_function("best_five_of_seven", |b| b.iter(|| best_five_of_seven(black_box(&cards))));
}

criterion_group!(benches, bench_eval_five, bench_eval_seven);
criterion_main!(benches);

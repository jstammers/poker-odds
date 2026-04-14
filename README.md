[![Rust](https://img.shields.io/badge/rust-stable-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Deploy](https://github.com/jstammers/poker-odds/actions/workflows/deploy.yml/badge.svg)](https://github.com/jstammers/poker-odds/actions/workflows/deploy.yml)

# poker-odds

A poker odds calculator with two interfaces: a terminal UI (ratatui) and a React/TypeScript web app powered by a WebAssembly build of the same Rust engine. Supports Texas Hold'em, Omaha Hold'em, 7-Card Stud, and 5-Card Draw.

The web app is deployed automatically to GitHub Pages on every push to `main`.

---

## Features

- **Four variants:** Texas Hold'em, Omaha Hold'em, 7-Card Stud, 5-Card Draw
- **Adaptive simulation:** exact enumeration when combinations are 50,000 or fewer; Monte Carlo otherwise
- **Parallel native execution:** Rayon distributes Monte Carlo iterations across all available CPU cores
- **Responsive web UI:** WASM simulation runs in a Web Worker so the page never blocks
- **O(1) hand evaluation:** Cactus Kev lookup tables — no runtime sorting or hashing in the hot path

---

## Getting started

### Prerequisites

- [Rust](https://rustup.rs/) via **rustup** (not Homebrew)
- [`wasm-pack`](https://rustwasm.github.io/wasm-pack/installer/): `cargo install wasm-pack`
- Node.js 18+ and npm

### Terminal UI

```sh
cargo run --release
```

### Web app (development)

First-time setup:

```sh
make setup        # installs JS deps, builds WASM, starts dev server
```

Subsequent runs (after the WASM is already built):

```sh
make dev
```

### Web app (production build)

```sh
make all          # builds WASM then the production JS bundle → web/dist/
```

---

## Makefile reference

| Target | Description |
|---|---|
| `make wasm` | Compile Rust to WASM + JS bindings via wasm-pack |
| `make dev` | Start the Vite dev server (run `make wasm` first) |
| `make build` | Production JS bundle (`web/dist/`) |
| `make test` | Run Rust unit tests (native) |
| `make check` | `cargo check` for the native build |
| `make check-wasm` | `cargo check` targeting `wasm32-unknown-unknown` |
| `make setup` | First-time setup: install JS deps, build WASM, start dev server |
| `make all` | Build WASM then the production JS bundle |
| `make clean` | Remove Cargo build artefacts, `web/wasm/`, `web/dist/`, `web/node_modules/` |

> **Toolchain note:** if both Homebrew rustc and rustup rustc are present, the Makefile automatically prepends the rustup bin directory to `PATH` for all WASM-related targets, ensuring wasm-pack picks up the correct toolchain.

---

## Architecture

```
poker-odds/
├── src/
│   ├── lib.rs                  # crate root — module declarations
│   ├── main.rs                 # native TUI binary entry point
│   ├── wasm.rs                 # wasm-bindgen public API (WASM target only)
│   ├── cards/
│   │   ├── card.rs             # Card, Rank, Suit primitives
│   │   └── deck.rs             # Deck with fast random deal
│   ├── eval/
│   │   ├── evaluator.rs        # evaluate_five, best_five_of_seven, evaluate_omaha
│   │   ├── lookup.rs           # Cactus Kev lookup table construction (lazy_static)
│   │   └── rank.rs             # HandValue, HandCategory ordering
│   ├── game/
│   │   ├── state.rs            # GameState (hole cards, community, opponent count)
│   │   └── variant.rs          # GameVariant, BettingRound enums
│   ├── sim/
│   │   ├── engine.rs           # Monte Carlo + exact enumeration runner
│   │   ├── config.rs           # SimConfig (iterations, threshold, threads)
│   │   └── result.rs           # OddsResult, SimAccumulator, SimMethod
│   └── tui/                    # ratatui TUI (native only)
│       ├── app.rs
│       ├── events.rs
│       ├── screens/            # variant select, hole cards, community, odds display
│       └── widgets/            # card input, card display
├── web/
│   ├── src/
│   │   ├── App.tsx             # top-level React component
│   │   ├── components/         # VariantPicker, CardGrid, CardSlots, OddsDisplay
│   │   ├── workers/
│   │   │   └── sim.worker.ts   # Web Worker — calls WASM calculate_odds()
│   │   └── types/odds.ts       # TypeScript types for the WASM boundary
│   ├── wasm/                   # wasm-pack output (generated, gitignored)
│   ├── vite.config.ts
│   └── package.json
├── benches/
│   ├── eval_bench.rs           # criterion benchmarks for the evaluator
│   └── sim_bench.rs            # criterion benchmarks for the simulator
├── Cargo.toml
└── Makefile
```

---

## How it works

### Hand evaluator (Cactus Kev algorithm)

`evaluate_five` classifies any 5-card hand in O(1) using three pre-built lookup tables that are constructed once at startup:

1. **Flush/straight-flush** — if all five suits match, index `FLUSH_TABLE` with a 13-bit rank bitmask. Straight flushes and regular flushes both resolve here.
2. **Straight / high card** — if the rank bitmask has exactly 5 distinct bits set (no pairs), index `UNIQUE5_TABLE` with the same bitmask. This covers all C(13,5) = 1,287 non-flush combinations.
3. **Paired hands** — multiply together each rank's assigned prime number (2→2, 3→3, 5→5, …, Ace→41). The product is unique per hand category, so `PAIRS_TABLE` is binary-searched in O(log n) to find the Cactus Kev rank.

`best_five_of_seven` hard-codes all 21 index combinations for the C(7,5) enumeration, avoiding itertools overhead. `evaluate_omaha` loops over the 6 × 10 = 60 legal hole+board combinations mandated by Omaha rules.

### Simulation engine

`run_simulation` in `src/sim/engine.rs` first counts the remaining combinations. If the count is at or below `exact_threshold` (default 50,000), it exhaustively evaluates every runout. Otherwise it runs Monte Carlo.

On native builds, Monte Carlo is parallelised with Rayon: work is split into chunks, each chunk is distributed across all CPU cores with independent `Xoshiro256++` RNG streams (seeded by XOR-mixing a chunk offset with a per-thread constant), and per-thread `SimAccumulator` structs are merged without any lock in the hot path.

On WASM, the single-threaded path is used, and `calculate_odds` is called from a Web Worker so the React UI remains responsive.

---

## Performance

Measured on 14-core Apple Silicon (M-series) with `cargo bench`:

| Benchmark | Time |
|---|---|
| `evaluate_five` (single hand) | ~1.5 ns |
| `best_five_of_seven` (21 combos) | ~54 ns |
| 100,000 Texas Hold'em pre-flop iterations (Rayon, 14 cores) | ~9 ms |

The default simulation runs 500,000 iterations, completing in roughly 45 ms natively and ~50 ms in the browser (single-threaded WASM).

---

## Deployment

The GitHub Actions workflow at `.github/workflows/deploy.yml` runs on every push to `main`. It installs the Rust toolchain with the `wasm32-unknown-unknown` target, builds the WASM artefact with wasm-pack, builds the Vite production bundle, and deploys `web/dist/` to GitHub Pages.

Pull requests trigger the build job only (no deploy step).

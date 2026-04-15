[![Rust](https://img.shields.io/badge/rust-stable-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Deploy](https://github.com/jstammers/poker-odds/actions/workflows/deploy.yml/badge.svg)](https://github.com/jstammers/poker-odds/actions/workflows/deploy.yml)

# poker-odds

A poker odds calculator and GTO solver with three interfaces: a native macOS desktop app (Tauri v2), a browser-based web app (WASM), and a terminal UI (ratatui). All three share the same Rust engine. Supports Texas Hold'em, Omaha Hold'em, 7-Card Stud, and 5-Card Draw.

---

## Features

- **Odds calculator** — four game variants, adaptive simulation (exact enumeration or Monte Carlo)
- **GTO solver** — CFR+ and Discounted CFR for heads-up postflop strategy computation (flop, turn, river)
- **Desktop app** — native macOS window via Tauri v2, with solver progress streaming and cancellation
- **Web app** — deploys to GitHub Pages; WASM simulation runs in a Web Worker so the page never blocks
- **Terminal UI** — ratatui-based TUI for quick command-line use
- **O(1) hand evaluation** — Cactus Kev lookup tables with no runtime sorting in the hot path
- **Parallel native execution** — Rayon distributes Monte Carlo iterations across all CPU cores

---

## Quick start

### Prerequisites

| Tool | Install |
|---|---|
| [Rust](https://rustup.rs/) (via rustup) | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| [wasm-pack](https://rustwasm.github.io/wasm-pack/) | `cargo install wasm-pack` |
| Node.js 20+ and npm | [nodejs.org](https://nodejs.org/) or `brew install node` |

### Desktop app (macOS, recommended)

```sh
make tauri-dev        # builds WASM, installs JS deps, launches the Tauri dev window
```

To produce a release `.dmg`:

```sh
make tauri-build      # outputs to web/src-tauri/target/release/bundle/dmg/
```

### Web app (browser)

```sh
make setup            # installs JS deps, builds WASM, starts Vite dev server
```

Or if WASM is already built:

```sh
make dev              # starts Vite dev server on http://localhost:5173
```

### Terminal UI

```sh
cargo run --release
```

### Run tests

```sh
make test             # all Rust unit tests (55 tests)
```

---

## Makefile reference

| Target | Description |
|---|---|
| `make tauri-dev` | Build WASM, install JS deps, launch Tauri desktop app in dev mode |
| `make tauri-build` | Build a release `.dmg` for macOS |
| `make wasm` | Compile Rust to WASM + JS bindings via wasm-pack |
| `make dev` | Start the Vite dev server (run `make wasm` first) |
| `make build` | Production JS bundle (`web/dist/`) |
| `make test` | Run Rust unit tests (native) |
| `make check` | `cargo check` for the native build |
| `make check-wasm` | `cargo check` targeting `wasm32-unknown-unknown` |
| `make check-tauri` | `cargo check` for the Tauri backend crate |
| `make setup` | First-time setup: install JS deps, build WASM, start dev server |
| `make all` | Build WASM then the production JS bundle |
| `make clean` | Remove all build artifacts |

> **Toolchain note:** if both Homebrew rustc and rustup rustc are present, the Makefile automatically prepends the rustup bin directory to `PATH` for all WASM-related targets, ensuring wasm-pack picks up the correct toolchain.

---

## Project structure

```
poker-odds/
├── src/                            # Rust library + binaries
│   ├── lib.rs                      # crate root
│   ├── main.rs                     # native TUI entry point
│   ├── wasm.rs                     # wasm-bindgen API (WASM target only)
│   ├── cards/
│   │   ├── card.rs                 # Card, Rank, Suit primitives
│   │   └── deck.rs                 # Deck with fast random deal
│   ├── eval/
│   │   ├── evaluator.rs            # evaluate_five, best_five_of_seven, evaluate_omaha
│   │   ├── lookup.rs               # Cactus Kev lookup tables (lazy_static)
│   │   └── rank.rs                 # HandValue, HandCategory ordering
│   ├── game/
│   │   ├── state.rs                # GameState (hole, community, opponents)
│   │   └── variant.rs              # GameVariant enum
│   ├── sim/
│   │   ├── engine.rs               # Monte Carlo + exact enumeration
│   │   ├── config.rs               # SimConfig (iterations, threshold, threads)
│   │   └── result.rs               # OddsResult, SimAccumulator
│   ├── solver/                     # GTO solver (native only)
│   │   ├── cfr.rs                  # CFR+ and Discounted CFR iteration
│   │   ├── postflop.rs             # Postflop game tree builder
│   │   ├── game_tree.rs            # Game tree node types
│   │   ├── action.rs               # Poker actions + bet sizing config
│   │   ├── range.rs                # Hand range parser ("AA,AKs,QQ-TT")
│   │   ├── strategy.rs             # Strategy profile extraction
│   │   ├── info_set.rs             # Information set abstraction
│   │   ├── abstraction.rs          # Card abstraction interface
│   │   ├── exploitability.rs       # Exploitability computation
│   │   ├── toy_games.rs            # Kuhn/Leduc poker for testing
│   │   └── upi.rs                  # Universal Poker Interface
│   └── tui/                        # ratatui TUI (native only)
│       ├── app.rs                  # TUI application loop
│       ├── events.rs               # Input event handling
│       ├── screens/                # Variant select, card input, solver config, results
│       └── widgets/                # Card display widgets
│
├── web/                            # React + Vite frontend
│   ├── src/
│   │   ├── App.tsx                 # Root component (tab routing, backend init)
│   │   ├── api/
│   │   │   ├── backend.ts          # Backend interface + runtime detection
│   │   │   ├── wasm-backend.ts     # Web Worker / WASM implementation
│   │   │   └── tauri-backend.ts    # Tauri invoke / event implementation
│   │   ├── pages/
│   │   │   ├── OddsCalculator.tsx  # Odds calculator page
│   │   │   └── GtoSolver.tsx       # GTO solver page (desktop only)
│   │   ├── components/
│   │   │   ├── CardGrid.tsx        # 13x4 interactive card picker
│   │   │   ├── CardSlots.tsx       # Selected card display slots
│   │   │   ├── OddsDisplay.tsx     # Win/tie/lose bars + hand distribution
│   │   │   ├── VariantPicker.tsx   # Game variant selector
│   │   │   ├── TabNav.tsx          # Tab navigation header
│   │   │   ├── RangeInput.tsx      # Range string input with validation
│   │   │   ├── ProgressBar.tsx     # Solver progress bar
│   │   │   └── StrategyDisplay.tsx # Strategy results table
│   │   ├── types/
│   │   │   ├── odds.ts             # Odds calculator TypeScript types
│   │   │   └── solver.ts           # Solver TypeScript types
│   │   ├── workers/
│   │   │   └── sim.worker.ts       # Web Worker for WASM simulation
│   │   └── styles/
│   │       └── index.css           # All styles
│   │
│   ├── src-tauri/                  # Tauri v2 desktop backend
│   │   ├── Cargo.toml              # Depends on poker-odds via path = "../.."
│   │   ├── tauri.conf.json         # Window config, dev server URL, build commands
│   │   ├── capabilities/
│   │   │   └── default.json        # IPC permissions
│   │   └── src/
│   │       ├── main.rs             # Tauri entry point
│   │       └── lib.rs              # Tauri commands (odds calc + solver + range validation)
│   │
│   ├── package.json
│   └── vite.config.ts
│
├── benches/                        # Criterion benchmarks
├── .github/workflows/
│   ├── deploy.yml                  # WASM web app → GitHub Pages
│   └── release.yml                 # Tauri .dmg → GitHub Releases
├── Cargo.toml
└── Makefile
```

---

## Architecture

### Dual-mode frontend

The React frontend runs in two modes using the same codebase:

| Mode | Backend | Solver available | How it runs |
|---|---|---|---|
| **Web** (WASM) | `WasmBackend` — Web Worker calling `wasm-bindgen` exports | No | `npm run dev` or GitHub Pages |
| **Desktop** (Tauri) | `TauriBackend` — `invoke()` IPC to Rust commands | Yes | `npx tauri dev` or `.dmg` |

Runtime detection uses `window.__TAURI_INTERNALS__` to select the correct backend at startup. The GTO Solver tab only appears in desktop mode.

### Hand evaluator (Cactus Kev algorithm)

`evaluate_five` classifies any 5-card hand in O(1) using three pre-built lookup tables:

1. **Flush/straight-flush** — if all five suits match, index `FLUSH_TABLE` with a 13-bit rank bitmask
2. **Straight / high card** — if the rank bitmask has exactly 5 distinct bits, index `UNIQUE5_TABLE`
3. **Paired hands** — multiply each rank's prime number; binary-search the product in `PAIRS_TABLE`

`best_five_of_seven` hard-codes all 21 C(7,5) index combinations. `evaluate_omaha` loops over the 60 legal combinations mandated by Omaha rules.

### Simulation engine

`run_simulation` counts remaining combinations. If at or below `exact_threshold` (default 50,000), it exhaustively evaluates every runout. Otherwise it runs Monte Carlo, parallelised with Rayon on native (independent `Xoshiro256++` RNG per thread, lock-free accumulation).

### GTO solver

The solver uses Counterfactual Regret Minimization to approximate Nash equilibrium strategies for heads-up postflop play:

- **CFR+** clamps negative regrets to zero each iteration (faster convergence)
- **Discounted CFR** applies time-dependent discounting to past regrets and strategies
- Supports flop, turn, and river starting boards with configurable bet sizing (pot fractions)
- Hand ranges use standard notation (`AA,AKs,QQ-TT,A5s-A2s`)
- Exploitability is computed after solving to measure strategy quality (lower = closer to Nash)

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

### Web app (GitHub Pages)

The workflow at `.github/workflows/deploy.yml` runs on every push to `main`. It builds WASM with wasm-pack, bundles with Vite, and deploys `web/dist/` to GitHub Pages.

### Desktop app (GitHub Releases)

The workflow at `.github/workflows/release.yml` triggers on version tags (`v*`). It builds the Tauri app for macOS (aarch64 + x86_64) and uploads `.dmg` files to the GitHub Release.

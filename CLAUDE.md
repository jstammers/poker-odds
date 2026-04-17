# poker-odds

A Rust poker odds calculator with three interfaces: a React/WASM web app, a Tauri desktop app, and a terminal UI.

## Project structure

```
├── src/                  # Core Rust library + TUI binary
│   ├── cards/            # Card, Rank, Suit types and parsing
│   ├── eval/             # Hand evaluator and lookup tables
│   ├── sim/              # Monte Carlo simulation engine
│   ├── solver/           # CFR/GTO solver (info sets, tree building)
│   ├── game/             # Game variants and state (Hold'em, Omaha, Stud)
│   ├── tui/              # Ratatui terminal UI
│   └── wasm.rs           # wasm-bindgen public API for the browser
├── web/                  # React + Vite frontend
│   ├── src/              # React components and backend abstraction
│   └── src-tauri/        # Tauri v2 desktop backend (Rust)
├── benches/              # Criterion benchmarks
├── Cargo.toml            # Workspace root (version = 0.3.0)
└── Makefile              # Common dev tasks
```

The library compiles to both native (for the TUI and Tauri backend) and `wasm32-unknown-unknown` (for the browser). WASM-only dependencies are gated with `cfg(target_arch = "wasm32")` — there is no `wasm` cargo feature.

## Development workflow

### Common commands

```bash
make wasm        # Compile Rust → WASM + JS bindings (wasm-pack, bundler target)
make dev         # Start Vite dev server (run make wasm first)
make build       # Production web bundle
make tauri-dev   # Launch Tauri desktop app in dev mode
make tauri-build # Build release .dmg
make test        # Run Rust unit tests
make check       # cargo check (native)
make check-wasm  # cargo check targeting wasm32
```

### Before every commit

```bash
cargo fmt --all            # Format all Rust code
cargo clippy --all-targets -- -D warnings   # Lint (zero warnings policy)
cargo test                 # All unit tests must pass
```

### Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat: add new hand evaluator
fix(sim): correct edge case in river simulation
chore: update dependencies
docs: add solver design notes
style: apply cargo fmt
```

### CI / GitHub Actions

| Workflow | Trigger | What it does |
|---|---|---|
| `ci.yml` | PRs + `main` | fmt check, clippy, tests, WASM+web build |
| `deploy.yml` | `main` push | Build WASM+web, deploy to GitHub Pages |
| `release.yml` | `v*` tags | Build universal macOS DMG, create GitHub Release |

### Releasing

1. Bump `version` in `Cargo.toml`, `web/package.json`, and `web/src-tauri/tauri.conf.json` to match.
2. Merge to `main`.
3. Push a `vX.Y.Z` tag — the release workflow builds and publishes automatically.

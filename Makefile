# poker-odds — Makefile
#
# This machine has BOTH Homebrew rustc (/opt/homebrew/bin/rustc) AND rustup rustc.
# Homebrew's rustc does NOT have wasm32-unknown-unknown; rustup's does.
# All WASM-related targets force rustup's toolchain via RUSTUP_RUSTC / PATH override.

.PHONY: all wasm dev build install test check check-wasm clean setup

# ── Top-level targets ─────────────────────────────────────────────────────────

## Build WASM then the production JS bundle
all: wasm build

## Start dev server (run `make wasm` first)
dev:
	cd web && npm run dev

## Production JS bundle
build:
	cd web && npm run build

## Preview production build
preview:
	cd web && npm run preview

# ── Rust / WASM ───────────────────────────────────────────────────────────────

# Detect rustup's toolchain bin dir so wasm-pack uses it instead of Homebrew's rustc.
# wasm-pack finds rustc via PATH (ignores $RUSTC), so we prepend the rustup bin dir.
RUSTUP_BIN := $(shell dirname "$$(rustup which rustc 2>/dev/null)")

## Compile Rust → WASM + JS bindings (requires: cargo install wasm-pack)
wasm:
	PATH="$(RUSTUP_BIN):$$PATH" wasm-pack build --target bundler --out-dir web/wasm --release

## Check native build
check:
	cargo check

## Check WASM lib (requires rustup stable with wasm32 target)
check-wasm:
	RUSTC="$(RUSTUP_RUSTC)" cargo check --lib --target wasm32-unknown-unknown

## Run Rust unit tests (native)
test:
	cargo test

# ── JS ────────────────────────────────────────────────────────────────────────

## Install JS dependencies
install:
	cd web && npm install

# ── Setup from scratch ────────────────────────────────────────────────────────

## Full first-time setup: install JS deps → build WASM → start dev server
setup: install wasm dev

# ── Clean ─────────────────────────────────────────────────────────────────────

clean:
	cargo clean
	rm -rf web/wasm web/dist web/node_modules

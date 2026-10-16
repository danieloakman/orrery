# sim-rs developer tasks. Prefer these over ad-hoc cargo flags.
set shell := ["bash", "-euo", "pipefail", "-c"]

mod run

default:
    @just --list

# Format all crates.
fmt:
    cargo fmt --all

# Format check + clippy (warnings denied). Matches CI.
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets -- -D warnings

# Run the full workspace test suite (unit + integration).
test:
    cargo test --workspace

# Debug build of every crate.
build:
    cargo build --workspace

# Release build of every crate.
build-release:
    cargo build --workspace --release

# Start the simulation server only (no auto-advance). Ctrl-C to stop.
serve socket="/tmp/sim.sock":
    cargo run -p sim-rs -- --socket {{socket}}

# Everything CI runs locally.
ci: check test

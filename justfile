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

# Start the simulation server. It owns the session. Ctrl-C to stop.
serve socket="/tmp/sim.sock" kind="game_of_life" config='{"seed":1,"width":16,"height":16,"density":0.3}' state_out="":
    cargo run -p sim-rs -- --socket {{socket}} {{ if state_out != "" { "--state-out '" + state_out + "'" } else { "" } }} create --kind {{kind}} --config '{{config}}'

# Everything CI runs locally.
ci: check test

# Orrery developer tasks. Prefer these over ad-hoc cargo flags.
set shell := ["bash", "-euo", "pipefail", "-c"]

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

# Full-tilt simulation run.
run seed="1" years="100" out="out/seed1":
    cargo run -p orrery -- run --seed {{seed}} --years {{years}} --out {{out}}

# Everything CI runs locally.
ci: check test

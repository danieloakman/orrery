# Orrery

Deterministic, seedable fantasy **history simulator** in Rust. A local server owns the world clock; clients advance time with `advance_tick(n)` and dump a chronicle.

North-star: many agents, each locked to a civ (or god role), sharing one concurrent world through the same server API.

## Quick start

```bash
cargo build --workspace
# ticks-per-day=1 is faster for chronicle batches; default is 24 (hourly)
cargo run -p orrery -- run --seed 1 --years 100 --ticks-per-day 1 --out out/seed1
```

This spawns `orrery-server`, creates a world, advances at full tilt in yearly batches, writes:

- `chronicle.md` — dated legends
- `map.txt` — ASCII square-grid map
- `state.json` — full serde snapshot
- `manifest.json` — seed/config/hash

Same seed + config ⇒ same chronicle (see `cargo test -p orrery-core`).

### Example seeds

```bash
cargo run -p orrery -- run --seed 1 --years 200 --out out/seed1
cargo run -p orrery -- run --seed 42 --years 200 --out out/seed42
```

### Manual server

```bash
cargo run -p orrery-server -- --socket /tmp/orrery.sock
# elsewhere:
cargo run -p orrery -- run --seed 7 --years 50 --socket /tmp/orrery.sock --out out/manual
```

## Clock & API

- Tick defaults to **hourly** (`ticks_per_day=24`); year/day derived.
- Systems run on a **schedule** (yearly civ rules) and integrate elapsed time; queries never mutate.
- Server RPCs (newline JSON on a Unix socket): `create`, `status`, `advance_tick`, `dump`, `shutdown`.
- Blind batches are intentional: you pre-commit to `n` ticks before the next observe/intervene window.
- The server **never** decides to stop (including if all civs die). The runner chooses when to dump/shutdown.

## Workspace

| Crate | Role |
|-------|------|
| `orrery-core` | Map gen, clock, rules, chronicle, IPC types |
| `orrery-server` | Owns one world; Unix socket RPC |
| `orrery` | Full-tilt runner client |

## Non-goals (for now)

God/civ `simctl`, agent skills, egui, hex maps, network auth.

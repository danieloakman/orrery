# Mundus

[![CI](https://github.com/danieloakman/mundus/actions/workflows/ci.yml/badge.svg)](https://github.com/danieloakman/mundus/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Deterministic fantasy **history simulator** in Rust.

Generate a seedable world, tick civilizations for centuries, and export a readable chronicle. A local server owns simulation time; clients advance it with `advance_tick(n)`, the same seam later humans and LLM agents will use to observe and steer.

> North-star: many agents in one shared world, each locked to a separate civ (or a god role), interacting only through sim rules, not co-op on one fort.

## Quick start

```bash
just build

# Server only (clients attach separately)
just serve

# Full-tilt run: spawns the server, simulates, dumps artifacts
just run seed=1 years=100 out=out/seed1
```

**Outputs** in `--out` (from `just run`):

| File | Contents |
|------|----------|
| `chronicle.md` | Dated legends |
| `map.txt` | ASCII square-grid map |
| `state.json` | Full world snapshot |
| `manifest.json` | Seed, config, state hash |

Same seed + config + advance sequence ⇒ same hash (`just test`).

### Example seeds

```bash
just run seed=1 years=200 out=out/seed1
just run seed=42 years=200 out=out/seed42
```

### Agent CLI (observe + advance)

```bash
just serve   # terminal 1 — /tmp/mundus.sock

# terminal 2 — create a world, then observe / step time
cargo run -p mundus -- create --seed 7 --civs 4
cargo run -p mundus -- status
cargo run -p mundus -- civs
cargo run -p mundus -- civ 0          # or exact name
cargo run -p mundus -- advance --years 1
cargo run -p mundus -- chronicle --since 0 --limit 20
cargo run -p mundus -- --json civs    # machine-readable
```

Global flags: `-S/--socket` (default `/tmp/mundus.sock`), `--json`.

### Full-tilt against a running server

```bash
just serve   # terminal 1
cargo run -p mundus -- -S /tmp/mundus.sock run --seed 7 --years 50 --out out/manual
```

## Development

Toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml) (Rust 1.98.1 + rustfmt/clippy/rust-analyzer). That file is the single source of truth for:

- **rustup** / GitHub Actions (`dtolnay/rust-toolchain` reads it)
- **Nix** (`flake.nix` loads it via [rust-overlay](https://github.com/oxalica/rust-overlay))

```bash
# Nix + direnv (recommended)
direnv allow          # or: nix develop

# Without Nix: install rustup, then just
cargo install just    # if needed
```

Common tasks:

| Command | What it does |
|---------|----------------|
| `just check` | `fmt --check` + `clippy -D warnings` |
| `just test` | workspace unit + integration tests |
| `just ci` | `check` then `test` (same as Actions) |
| `just serve` | start server only (no auto-advance) |
| `just run` | full-tilt simulation dump |

License: [MIT](LICENSE).

## How it works

```text
mundus (CLI)  --NDJSON/Unix socket-->  mundus-server  -->  mundus-core
  create / status / list_civs / get_civ / chronicle / advance_tick / dump / shutdown
```

### Time

**1 tick = 1 minute, always.** Not configurable. Most systems still run on coarser periods (e.g. yearly) and integrate elapsed time.

| Advance | Sim time |
|---------|----------|
| `advance_tick(1)` | 1 minute |
| `advance_tick(60)` | 1 hour |
| `advance_tick(1440)` | 1 day |
| `advance_tick(525600)` | 1 year (365×1440) |

The server never auto-ticks; time stands still until a client advances it. Blind batches are intentional: you pre-commit to a time window before the next observe/intervene chance.

- **Systems:** scheduled (e.g. yearly civ growth/war), integrating elapsed time. Queries never mutate state.
- **Stop policy:** the server never decides to end (civs dying is just state). The client chooses when to dump or shut down.

### RPC (newline-delimited JSON)

| Method | Purpose |
|--------|---------|
| `create` | Seed + config → generate world |
| `status` | Read-only clock / civ counts |
| `list_civs` | Summaries of every civilization |
| `get_civ` | `{ "query": id\|name }` → full civ |
| `chronicle` | `{ "since_tick"?, "limit"? }` → events |
| `advance_tick` | `{ "n": u64 }` → step `n` minutes |
| `dump` | Chronicle, map, state JSON |
| `shutdown` | Graceful exit |

## Workspace

| Crate | Role |
|-------|------|
| [`mundus-core`](crates/mundus-core) | Map gen, clock, rules, chronicle, protocol |
| [`mundus-server`](crates/mundus-server) | Owns one world; Unix socket RPC |
| [`mundus`](crates/mundus) | CLI: full-tilt runner + agent observe/advance |

## Sample chronicle

```markdown
**Year 0** (day 0): The age began with 6 peoples.
**Year 0** (day 0): The Corfolk founded a settlement at (91,58).
**Year 6** (day 2190): Among the Dunhold: a twin moon rose and the rivers ran silver for a night
**Year 10** (day 3650): The Belormoor expanded to (42,34).
```

## Status / non-goals

POC: headless fantasy world, server + runner, deterministic dumps.

Not yet: `simctl` god/civ interventions, agent skills, map UI, hex grids, networked multiplayer.

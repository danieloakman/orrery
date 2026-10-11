# Orrery

[![CI](https://github.com/danieloakman/orrery/actions/workflows/ci.yml/badge.svg)](https://github.com/danieloakman/orrery/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Deterministic fantasy **history simulator** in Rust.

Generate a seedable world, tick civilizations for centuries, and export a readable chronicle. A local server owns simulation time; clients advance it with `advance_tick(n)`, the same seam later humans and LLM agents will use to observe and steer.

> North-star: many agents in one shared world, each locked to a separate civ (or a god role), interacting only through sim rules, not co-op on one fort.

## Quick start

```bash
just build

# Full-tilt run: spawns the server, simulates, dumps artifacts
just run seed=1 years=100 out=out/seed1
```

**Outputs** in `--out`:

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

### Manual server

```bash
cargo run -p orrery-server -- --socket /tmp/orrery.sock

cargo run -p orrery -- run --seed 7 --years 50 \
  --socket /tmp/orrery.sock --out out/manual
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
| `just run` | full-tilt simulation dump |

License: [MIT](LICENSE).

## How it works

```text
orrery (runner)  --NDJSON/Unix socket-->  orrery-server  -->  orrery-core
     create / status / advance_tick(n) / dump / shutdown
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
| `advance_tick` | `{ "n": u64 }` → step `n` minutes |
| `dump` | Chronicle, map, state JSON |
| `shutdown` | Graceful exit |

## Workspace

| Crate | Role |
|-------|------|
| [`orrery-core`](crates/orrery-core) | Map gen, clock, rules, chronicle, protocol |
| [`orrery-server`](crates/orrery-server) | Owns one world; Unix socket RPC |
| [`orrery`](crates/orrery) | Full-tilt runner client |

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

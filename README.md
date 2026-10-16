# sim-rs

[![CI](https://github.com/danieloakman/sim-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/danieloakman/sim-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A deterministic simulation host. One server process runs one session. The session can be any simulation that implements the `Simulation` trait. This repo ships two: **mundus**, a fantasy civilisation history, and **Game of Life**.

A client starts the server, advances time, and stops it. `sim-ctl` attaches to a running server and sends the other requests. It does not advance time or shut the server down.

```text
client (mundus-client / life-client)
    spawn, advance, shutdown
        |
        v
sim-server  -- trait -->  mundus | game of life
        ^
        |
sim-ctl: create, actions, call, status, state, restore
```

## Quick start

```bash
just build

# Server only. Clients and sim-ctl attach separately.
just serve

# Mundus for 100 years
just run::mundus seed=1 years=100 out=out/seed1

# Game of Life for 50 generations
just run::life seed=1 ticks=50 out=out/life
```

`state.json` in `--out` is the save point: the simulation (including its random stream), the journal of edits, any calls still scheduled, a hash, and extras such as the chronicle or the grid.

Same seed, config, and edits produce the same hash.

### Watch or edit a running server

```bash
just serve

sim-ctl create --kind game_of_life --config '{"seed":1,"width":16,"height":16,"density":0.3}'
sim-ctl actions
sim-ctl call set_cell --params '{"x":1,"y":1,"alive":true}' --at-tick 10
sim-ctl status
sim-ctl state
```

`--at-tick` files a mutating call for a later tick. The client that owns the server decides when to advance. Agents can read the world where it stops, and schedule edits for ticks inside the next jump that they will not see happen.

`--max-actions-per-tick` on `sim-server` (default unlimited) caps how many mutating calls one connection may make on a single tick. Reads do not count.

## Another simulation

`sim-core` is the trait and the protocol. `sim-server` is the socket host. A different project depends on those two, implements `Simulation`, and passes factories to `serve`:

```rust
sim_server::serve(
    sim_server::ServeConfig {
        socket: "/tmp/sim.sock".into(),
        max_actions_per_tick: 0,
    },
    |kind, config| match kind {
        "mine" => Ok(Box::new(my_sim::create(config)?)),
        other => Err(format!("unknown simulation {other}")),
    },
    |kind, sim| match kind {
        "mine" => Ok(Box::new(my_sim::restore(sim)?)),
        other => Err(format!("unknown simulation {other}")),
    },
)?;
```

`mundus` and `life` are not part of that library. They are plug-ins wired by the `sim-rs` binary in this repo.

## Workspace

| Crate | Role |
| --- | --- |
| `sim-core` | Trait, protocol, IPC client |
| `sim-server` | Socket host library |
| `sim-ctl` | Attach-only request CLI |
| `sim-rs` | `sim-server` binary; registers mundus and Game of Life |
| `mundus` | Fantasy civilisation simulation |
| `life` | Conway's Game of Life |
| `mundus-client` | Run mundus and write a save point |
| `life-client` | Run Game of Life and write a save point |

## Time

The host does not define what a tick means. Mundus uses 1 tick = 1 minute. Game of Life uses 1 tick = 1 generation.

The server does not advance on its own. A client calls `advance`. Scheduled calls run when the world reaches their tick, in the order they were accepted. Ticks that an `advance` jumps over are not shown.

A stopped simulation accepts reads and `state`. It rejects `advance` and mutating calls. Game of Life stops at `max_ticks`. Mundus does not stop itself.

## Development

Toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml).

```bash
direnv allow          # or: nix develop
just ci
```

| Command | What it does |
| --- | --- |
| `just check` | `fmt --check` + `clippy -D warnings` |
| `just test` | workspace tests |
| `just ci` | `check` then `test` |
| `just serve` | server only |
| `just run::mundus` | mundus save point |
| `just run::life` | Game of Life save point |

## Not built

A client that lets a person play, and a client that runs several agents fairly, are the same kind of program as `mundus-client`: they start the server, advance it, and stop it. Agents would use `sim-ctl` to read and to schedule calls. Those clients are not in this repo yet.

License: [MIT](LICENSE).

set shell := ["bash", "-euo", "pipefail", "-c"]

# Run the mundus simulation for a number of years and write a save point.
mundus seed="1" years="100" out="out/seed1":
    cargo run -p mundus-client -- --seed {{seed}} --years {{years}} --out {{out}}

# Run Game of Life for a number of generations and write a save point.
life seed="1" ticks="50" out="out/life":
    cargo run -p life-client -- --seed {{seed}} --ticks {{ticks}} --out {{out}}

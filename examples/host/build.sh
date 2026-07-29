#!/bin/sh
# Builds and runs the desktop ship-tier capstone through the developer CLI.

set -eu

HOST_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
EXAMPLES_DIR=$(CDPATH= cd -- "$HOST_DIR/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "$EXAMPLES_DIR/.." && pwd)
ENGINE_DIR="$EXAMPLES_DIR/engine"
OUT_DIR="$REPO_ROOT/target/examples-host"

cd "$REPO_ROOT"
cargo build --offline --release -p subscript-cli 1>&2
"$REPO_ROOT/target/release/subscript" build \
    --source "$HOST_DIR/game.ts" \
    --mirror "$ENGINE_DIR/engine.generated.d.ts" \
    --host "$ENGINE_DIR/engine.c" \
    --host "$HOST_DIR/main.c" \
    -o "$OUT_DIR" \
    --run

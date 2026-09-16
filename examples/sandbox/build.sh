#!/bin/sh
# Builds and runs the sandbox-profile host through the developer CLI.

set -eu

HOST_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
EXAMPLES_DIR=$(CDPATH= cd -- "$HOST_DIR/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "$EXAMPLES_DIR/.." && pwd)
OUT_DIR="$REPO_ROOT/target/examples-sandbox"

cd "$REPO_ROOT"
cargo build --offline --release -p subscript-cli 1>&2
"$REPO_ROOT/target/release/subscript" build \
    --profile sandbox \
    --source "$HOST_DIR/mod.ts" \
    --host "$HOST_DIR/main.c" \
    -o "$OUT_DIR" \
    --run

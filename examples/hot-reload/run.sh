#!/bin/sh
# Builds the developer CLI and starts the interactive hot-reload demo.

set -eu

DEMO_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
EXAMPLES_DIR=$(CDPATH= cd -- "$DEMO_DIR/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "$EXAMPLES_DIR/.." && pwd)

cd "$REPO_ROOT"
cargo build --offline -p subscript-cli 1>&2
cd "$DEMO_DIR"
exec "$REPO_ROOT/target/debug/subscript" run --watch demo.ts

#!/bin/sh
# Builds and runs the desktop ship-tier capstone.
#
# Requirements: Cargo with the workspace dependencies cached for --offline,
# a release-capable Rust toolchain, a POSIX shell, and clang on PATH (or CC
# naming the platform C compiler). No device SDK is used.
#
# Every source, include, output, and library path is resolved from this
# script's repository-relative location.

set -eu

HOST_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
EXAMPLES_DIR=$(CDPATH= cd -- "$HOST_DIR/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "$EXAMPLES_DIR/.." && pwd)
ENGINE_DIR="$EXAMPLES_DIR/engine"
RUNTIME_DIR="$REPO_ROOT/runtime"
TARGET_DIR="$REPO_ROOT/target"
OUT_DIR="$TARGET_DIR/examples-host"
CC_BIN=${CC:-clang}

cd "$REPO_ROOT"
mkdir -p "$OUT_DIR"

# Emit only the script translation unit; main.c is the host-owned entry.
cargo run --offline --release -p subscript-codegen --bin emit-c -- \
    "$OUT_DIR" \
    --source "$HOST_DIR/game.ts" \
    --mirror "$ENGINE_DIR/engine.generated.d.ts" \
    --no-entry 1>&2

# Build the one runtime archive that the emitted C and host link against.
cargo build --offline --release -p subscript-runtime

HOST_OS=$(uname -s)
EXE_SUFFIX=
SYSTEM_LIBS=
case "$HOST_OS" in
    MINGW* | MSYS* | CYGWIN* | Windows_NT)
        RUNTIME_LIB="$TARGET_DIR/release/subscript_runtime.lib"
        EXE_SUFFIX=.exe
        SYSTEM_LIBS="-lkernel32 -lntdll -luserenv -lws2_32 -ldbghelp"
        ;;
    *)
        RUNTIME_LIB="$TARGET_DIR/release/libsubscript_runtime.a"
        ;;
esac

if [ ! -f "$RUNTIME_LIB" ]; then
    echo "error: runtime static library not found at $RUNTIME_LIB" >&2
    exit 1
fi

# shellcheck disable=SC2086
"$CC_BIN" -std=c11 -O2 -fwrapv -ffp-contract=off \
    -I"$ENGINE_DIR" \
    -I"$RUNTIME_DIR/include" \
    "$OUT_DIR/program.c" \
    "$ENGINE_DIR/engine.c" \
    "$HOST_DIR/main.c" \
    "$RUNTIME_LIB" \
    $SYSTEM_LIBS \
    -o "$OUT_DIR/capstone$EXE_SUFFIX"

"$OUT_DIR/capstone$EXE_SUFFIX"

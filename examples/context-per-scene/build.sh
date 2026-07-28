#!/bin/sh
# Builds and runs the desktop ship-tier Context-per-scene host.
#
# Requirements and platform handling match examples/host/build.sh. Every
# path is resolved from this script's repository-relative location.

set -eu

HOST_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
EXAMPLES_DIR=$(CDPATH= cd -- "$HOST_DIR/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "$EXAMPLES_DIR/.." && pwd)
ENGINE_DIR="$EXAMPLES_DIR/engine"
RUNTIME_DIR="$REPO_ROOT/runtime"
TARGET_DIR="$REPO_ROOT/target"
OUT_DIR="$TARGET_DIR/examples-context-per-scene"
CC_BIN=${CC:-clang}

cd "$REPO_ROOT"
mkdir -p "$OUT_DIR"

# Emit only the script translation unit; main.c is the host-owned entry.
cargo run --offline --release -p subscript-codegen --bin emit-c -- \
    "$OUT_DIR" \
    --source "$HOST_DIR/scene.ts" \
    --mirror "$ENGINE_DIR/engine.generated.d.ts" \
    --no-entry 1>&2

cargo build --offline --release -p subscript-runtime

HOST_OS=$(uname -s)
EXE_SUFFIX=
SYSTEM_LIBS=
HOST_MSVC=
case "$HOST_OS" in
    MINGW* | MSYS* | CYGWIN* | Windows_NT)
        RUNTIME_LIB="$TARGET_DIR/release/subscript_runtime.lib"
        EXE_SUFFIX=.exe
        SYSTEM_LIBS="kernel32.lib ntdll.lib userenv.lib ws2_32.lib dbghelp.lib"
        if [ -z "${CC:-}" ]; then
            HOST_MSVC=1
            cargo build --offline --release -p subscript-codegen --bin msvc-cl
            CC_BIN="$TARGET_DIR/release/msvc-cl.exe"
        fi
        ;;
    *)
        RUNTIME_LIB="$TARGET_DIR/release/libsubscript_runtime.a"
        ;;
esac

if [ ! -f "$RUNTIME_LIB" ]; then
    echo "error: runtime static library not found at $RUNTIME_LIB" >&2
    exit 1
fi

if [ -n "$HOST_MSVC" ]; then
    CONTEXT_EXE=$(cygpath -w "$OUT_DIR/context-per-scene$EXE_SUFFIX")
    CONTEXT_OBJ_DIR=$(cygpath -w "$OUT_DIR")
    # The compiler's stdout is not part of the host program's golden.
    # shellcheck disable=SC2086
    "$CC_BIN" -nologo -std:c11 -O2 -utf-8 -fp:strict \
        -I "$ENGINE_DIR" \
        -I "$RUNTIME_DIR/include" \
        "$OUT_DIR/program.c" \
        "$ENGINE_DIR/engine.c" \
        "$HOST_DIR/main.c" \
        "$RUNTIME_LIB" \
        $SYSTEM_LIBS \
        -Fo:"$CONTEXT_OBJ_DIR\\" \
        -Fe:"$CONTEXT_EXE" -link 1>&2
else
    # shellcheck disable=SC2086
    "$CC_BIN" -std=c11 -O2 -fwrapv -ffp-contract=off \
        -I"$ENGINE_DIR" \
        -I"$RUNTIME_DIR/include" \
        "$OUT_DIR/program.c" \
        "$ENGINE_DIR/engine.c" \
        "$HOST_DIR/main.c" \
        "$RUNTIME_LIB" \
        $SYSTEM_LIBS \
        -o "$OUT_DIR/context-per-scene$EXE_SUFFIX"
fi

"$OUT_DIR/context-per-scene$EXE_SUFFIX"

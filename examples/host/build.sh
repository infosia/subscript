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
HOST_MSVC=
case "$HOST_OS" in
    MINGW* | MSYS* | CYGWIN* | Windows_NT)
        RUNTIME_LIB="$TARGET_DIR/release/subscript_runtime.lib"
        EXE_SUFFIX=.exe
        SYSTEM_LIBS="kernel32.lib ntdll.lib userenv.lib ws2_32.lib dbghelp.lib"
        # On windows-msvc the default C compiler is the native MSVC `cl`
        # driver, reached through a Rust shim that discovers `cl.exe` and its
        # INCLUDE/LIB/PATH environment from the registry (compiler.md §11c);
        # `sh` here does not carry a `vcvars` environment. `$CC`, if set,
        # still overrides it verbatim.
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
    # MSVC `cl` syntax, matching the ship tier (codegen/src/aot.rs). `/fp:strict`
    # is required: the emitted program.c may contain `1.0 / 0.0`, which `cl`
    # rejects under the default `/fp:precise` (C2124). The `-Fe:` output flag
    # must be joined to its path, so unlike the separate-argument inputs it is
    # not path-translated by MSYS; give it a native Windows path.
    CAPSTONE_EXE=$(cygpath -w "$OUT_DIR/capstone$EXE_SUFFIX")
    # Direct the per-source object files into $OUT_DIR (gitignored) so they do
    # not land in the repo root the script `cd`-ed into. `/Fo` for a directory
    # needs a trailing separator, and — like `-Fe:` — the joined argument is
    # not MSYS-path-translated, so it is a native Windows path with a trailing
    # backslash (mirrors codegen/src/aot.rs `msvc_object_directory_arg`).
    CAPSTONE_OBJ_DIR=$(cygpath -w "$OUT_DIR")
    # `cl` echoes each source filename (and a codegen note) to stdout; the
    # capstone's own stdout is what the gate byte-compares, so the compiler's
    # chatter is sent to stderr like the emit-c step above.
    # shellcheck disable=SC2086
    "$CC_BIN" -nologo -std:c11 -O2 -utf-8 -fp:strict \
        -I "$ENGINE_DIR" \
        -I "$RUNTIME_DIR/include" \
        "$OUT_DIR/program.c" \
        "$ENGINE_DIR/engine.c" \
        "$HOST_DIR/main.c" \
        "$RUNTIME_LIB" \
        $SYSTEM_LIBS \
        -Fo:"$CAPSTONE_OBJ_DIR\\" \
        -Fe:"$CAPSTONE_EXE" -link 1>&2
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
        -o "$OUT_DIR/capstone$EXE_SUFFIX"
fi

"$OUT_DIR/capstone$EXE_SUFFIX"

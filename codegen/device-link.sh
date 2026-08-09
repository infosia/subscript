#!/bin/sh
# Ship-tier target link (specs/blocks/compiler.md 11). The ship
# tier is C emission compiled by clang (LLVM); this proves the emitted C
# for a run-set corpus entry compiles and links on both device triples
# and the x86-64 Linux host target, against the real runtime static
# library and generated C host entry. The mobile binaries are not
# executed; the native Linux binary is run as a smoke check. No simulator
# or emulator is involved. This replaces the P3 cranelift-object device
# link (its ship role has ended); the P0.5 kill criterion is unaffected,
# C emission was its pre-registered fallback architecture.
#
# It is not part of `cargo test`: the Android half needs an NDK, which
# an arbitrary machine does not have (headless-first, CLAUDE.md core
# principle 4 — device-dependent runs are gated, never required).
#
# The Android half runs on any host with an NDK (contract §3); the iOS
# half requires a Mac, and the Linux half requires an x86-64 Linux host.
#
# Environment variables:
#   ANDROID_NDK_HOME  (optional) Android NDK installation root. When set,
#                     its prebuilt LLVM toolchain for this host is used:
#                     $ANDROID_NDK_HOME/toolchains/llvm/prebuilt/<host-tag>/bin
#   NDK_HOST_TAG      (optional) overrides the derived <host-tag>.
#   ENTRY_ID          (optional) accept-corpus entry to compile;
#                     defaults to a22-matrix-propagation.
#
# Requirements: clang for the Linux half; rustup target
# aarch64-linux-android (plus aarch64-apple-ios on a Mac), Xcode command
# line tools for the iOS half, and a populated cargo cache (every cargo
# invocation is --offline).
#
# All paths are resolved relative to this script's directory.

set -eu

CODEGEN_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$CODEGEN_DIR/.." && pwd)
OUT_DIR="$CODEGEN_DIR/out"
TARGET_DIR="$REPO_ROOT/target"
ACCEPT_DIR="$REPO_ROOT/corpus/accept"
RUNTIME_LIB=libsubscript_runtime.a
ENTRY_ID=${ENTRY_ID:-a22-matrix-propagation}

cd "$REPO_ROOT"

# 1. Emit the ship-tier C translation unit and the generated C entry.
cargo build --offline --release -p subscript-cli
if [ -d "$ACCEPT_DIR/$ENTRY_ID" ]; then
    ENTRY_SOURCE="$ENTRY_ID/main.ts"
else
    ENTRY_SOURCE="$ENTRY_ID.ts"
fi
(
    cd "$ACCEPT_DIR"
    "$TARGET_DIR/release/subscript" emit "$ENTRY_SOURCE" -o "$OUT_DIR"
)

LINKED=""

# 2. iOS: cross-build the runtime static library and compile+link the
#    emitted C with Xcode clang. -miphoneos-version-min=10.0 matches the
#    Rust static library's minimum OS.
HOST_OS=$(uname -s)
HOST_ARCH=$(uname -m)
if [ "$HOST_OS" = "Darwin" ]; then
    cargo build --offline --release -p subscript-runtime --target aarch64-apple-ios
    xcrun --sdk iphoneos clang --target=arm64-apple-ios -miphoneos-version-min=10.0 \
        -std=c11 -O2 -fwrapv -ffp-contract=off \
        "$OUT_DIR/program.c" \
        "$OUT_DIR/entry.c" \
        "$TARGET_DIR/aarch64-apple-ios/release/$RUNTIME_LIB" \
        -o "$OUT_DIR/$ENTRY_ID-ios"
    LINKED="$LINKED $OUT_DIR/$ENTRY_ID-ios"
else
    echo "note: host is $HOST_OS; the iOS half requires a Mac and is skipped" >&2
fi

# 3. x86-64 Linux: build the host runtime, compile+link the emitted C,
#    report the ELF, and execute it as a native smoke check. This runs
#    before the Android NDK requirement so an ordinary Linux host always
#    exercises its ship target.
if [ "$HOST_OS" = "Linux" ] && [ "$HOST_ARCH" = "x86_64" ]; then
    cargo build --offline --release -p subscript-runtime
    "${CC:-clang}" -std=c11 -O2 -fwrapv -ffp-contract=off \
        "$OUT_DIR/program.c" \
        "$OUT_DIR/entry.c" \
        "$TARGET_DIR/release/$RUNTIME_LIB" \
        -lm -ldl -lpthread -lrt -lutil -lgcc_s -lc \
        -o "$OUT_DIR/$ENTRY_ID-linux"
    LINKED="$LINKED $OUT_DIR/$ENTRY_ID-linux"
    file "$OUT_DIR/$ENTRY_ID-linux"
    echo "x86-64 Linux smoke output:"
    SMOKE_OUTPUT="$OUT_DIR/$ENTRY_ID-linux.stdout"
    if ! "$OUT_DIR/$ENTRY_ID-linux" > "$SMOKE_OUTPUT"; then
        rm -f "$SMOKE_OUTPUT"
        echo "error: the x86-64 Linux smoke binary failed" >&2
        exit 1
    fi
    cat "$SMOKE_OUTPUT"
    if ! cmp -s "$SMOKE_OUTPUT" "$ACCEPT_DIR/$ENTRY_ID.expected"; then
        echo "error: x86-64 Linux smoke output does not match $ENTRY_ID.expected" >&2
        rm -f "$SMOKE_OUTPUT"
        exit 1
    fi
    rm -f "$SMOKE_OUTPUT"
    echo "note: x86-64 Linux smoke output matches $ENTRY_ID.expected"
else
    echo "note: host is $HOST_OS/$HOST_ARCH; the x86-64 Linux target is skipped" >&2
fi

# 4. Android: cross-build the runtime static library and compile+link
#    the emitted C with NDK clang.
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    echo "note: ANDROID_NDK_HOME is not set; the Android half is skipped" >&2
else
    # The NDK names its prebuilt toolchains after the host it runs on. The
    # macOS toolchain is published as darwin-x86_64 on both Intel and Apple
    # silicon; NDK_HOST_TAG overrides the derivation.
    if [ -n "${NDK_HOST_TAG:-}" ]; then
        HOST_TAG="$NDK_HOST_TAG"
    else
        case "$HOST_OS" in
            Darwin) HOST_TAG=darwin-x86_64 ;;
            Linux) HOST_TAG=linux-x86_64 ;;
            MINGW* | MSYS* | CYGWIN* | Windows_NT) HOST_TAG=windows-x86_64 ;;
            *)
                echo "error: unknown host $HOST_OS; set NDK_HOST_TAG to the NDK prebuilt directory name" >&2
                exit 1
                ;;
        esac
    fi
    NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/$HOST_TAG/bin"
    ANDROID_CC="$NDK_BIN/aarch64-linux-android24-clang"
    if [ ! -x "$ANDROID_CC" ]; then
        echo "error: NDK clang not found at $ANDROID_CC" >&2
        exit 1
    fi
    export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_CC"
    export CC_aarch64_linux_android="$ANDROID_CC"
    export AR_aarch64_linux_android="$NDK_BIN/llvm-ar"
    cargo build --offline --release -p subscript-runtime --target aarch64-linux-android
    "$ANDROID_CC" --target=aarch64-linux-android24 -std=c11 -O2 -fwrapv -ffp-contract=off \
        "$OUT_DIR/program.c" \
        "$OUT_DIR/entry.c" \
        "$TARGET_DIR/aarch64-linux-android/release/$RUNTIME_LIB" \
        -o "$OUT_DIR/$ENTRY_ID-android"
    LINKED="$LINKED $OUT_DIR/$ENTRY_ID-android"
fi

# 5. Report the linked ship targets.
# shellcheck disable=SC2086
file $LINKED

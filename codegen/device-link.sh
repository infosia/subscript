#!/bin/sh
# Ship-tier device-triple link (specs/blocks/compiler.md 11). The ship
# tier is C emission compiled by clang (LLVM); this proves the emitted C
# for a run-set corpus entry cross-compiles and links, on both device
# triples, against the real runtime static library cross-built per
# triple and the generated C host entry. Compile+link is the whole
# criterion: no produced binary is executed, and no simulator or
# emulator is involved. This replaces the P3 cranelift-object device
# link (its ship role has ended); the P0.5 kill criterion is unaffected,
# C emission was its pre-registered fallback architecture.
#
# It is not part of `cargo test`: the Android half needs an NDK, which
# an arbitrary machine does not have (headless-first, CLAUDE.md core
# principle 4 — device-dependent runs are gated, never required).
#
# The Android half runs on any host with an NDK (contract §3); the iOS
# half requires a Mac and is skipped elsewhere.
#
# Environment variables:
#   ANDROID_NDK_HOME  (required) Android NDK installation root. Its
#                     prebuilt LLVM toolchain for this host is used:
#                     $ANDROID_NDK_HOME/toolchains/llvm/prebuilt/<host-tag>/bin
#   NDK_HOST_TAG      (optional) overrides the derived <host-tag>.
#   ENTRY_ID          (optional) accept-corpus entry to compile;
#                     defaults to a22-matrix-propagation.
#
# Requirements: rustup target aarch64-linux-android (plus
# aarch64-apple-ios on a Mac), Xcode command line tools for the iOS
# half, and a populated cargo cache (every cargo invocation is
# --offline).
#
# All paths are resolved relative to this script's directory.

set -eu

CODEGEN_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$CODEGEN_DIR/.." && pwd)
OUT_DIR="$CODEGEN_DIR/out"
TARGET_DIR="$REPO_ROOT/target"
RUNTIME_LIB=libsubscript_runtime.a
ENTRY_ID=${ENTRY_ID:-a22-matrix-propagation}

# 1. Emit the ship-tier C translation unit and the generated C entry.
cargo run --offline --release -p subscript-codegen --bin emit-c -- \
    "$OUT_DIR" "$ENTRY_ID"

LINKED=""

# 2. iOS: cross-build the runtime static library and compile+link the
#    emitted C with Xcode clang. -miphoneos-version-min=10.0 matches the
#    Rust static library's minimum OS.
HOST_OS=$(uname -s)
if [ "$HOST_OS" = "Darwin" ]; then
    cargo build --offline --release -p subscript-runtime --target aarch64-apple-ios
    xcrun --sdk iphoneos clang --target=arm64-apple-ios -miphoneos-version-min=10.0 \
        -O2 -ffp-contract=off \
        "$OUT_DIR/$ENTRY_ID.c" \
        "$OUT_DIR/entry.c" \
        "$TARGET_DIR/aarch64-apple-ios/release/$RUNTIME_LIB" \
        -o "$OUT_DIR/$ENTRY_ID-ios"
    LINKED="$LINKED $OUT_DIR/$ENTRY_ID-ios"
else
    echo "note: host is $HOST_OS; the iOS half requires a Mac and is skipped" >&2
fi

# 3. Android: cross-build the runtime static library and compile+link
#    the emitted C with NDK clang.
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    echo "error: ANDROID_NDK_HOME is not set; it must point to an Android NDK installation" >&2
    exit 1
fi
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
"$ANDROID_CC" --target=aarch64-linux-android24 -O2 -ffp-contract=off \
    "$OUT_DIR/$ENTRY_ID.c" \
    "$OUT_DIR/entry.c" \
    "$TARGET_DIR/aarch64-linux-android/release/$RUNTIME_LIB" \
    -o "$OUT_DIR/$ENTRY_ID-android"
LINKED="$LINKED $OUT_DIR/$ENTRY_ID-android"

# 4. Report. The binaries are never executed (compile+link is the criterion).
# shellcheck disable=SC2086
file $LINKED

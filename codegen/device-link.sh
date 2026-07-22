#!/bin/sh
# P3 device-triple AOT link (specs/blocks/compiler.md 8.1). The P0.5
# spike proved a fixed minimal program links; this proves the real
# lowering's output for a run-set corpus entry links, on both device
# triples, against the real runtime static library and the generated C
# entry. Compile+link is the whole criterion: no produced binary is
# executed, and no simulator or emulator is involved.
#
# It is not part of `cargo test`: the Android half needs an NDK, which
# an arbitrary machine does not have (headless-first, CLAUDE.md core
# principle 4 — device-dependent runs are gated, never required).
#
# Environment variables:
#   ANDROID_NDK_HOME  (required) Android NDK installation root. Its
#                     darwin-x86_64 prebuilt LLVM toolchain is used:
#                     $ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin
#   ENTRY_ID          (optional) accept-corpus entry to compile;
#                     defaults to a22-matrix-propagation.
#
# Requirements: rustup targets aarch64-apple-ios and
# aarch64-linux-android, Xcode command line tools (xcrun, iphoneos SDK),
# and a populated cargo cache (every cargo invocation is --offline).
#
# All paths are resolved relative to this script's directory.

set -eu

CODEGEN_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$CODEGEN_DIR/.." && pwd)
OUT_DIR="$CODEGEN_DIR/out"
TARGET_DIR="$REPO_ROOT/target"
RUNTIME_LIB=libsubscript_runtime.a
ENTRY_ID=${ENTRY_ID:-a22-matrix-propagation}

# 1. Emit both device objects and the generated C entry.
cargo run --offline --release -p subscript-codegen --bin emit-object -- \
    "$OUT_DIR" "$ENTRY_ID"

# 2. iOS: cross-build the runtime static library and link with Xcode clang.
#    -miphoneos-version-min=10.0 matches the Rust static library's minimum
#    OS and the build version stamped on the emitted object.
cargo build --offline --release -p subscript-runtime --target aarch64-apple-ios
xcrun --sdk iphoneos clang -target arm64-apple-ios -miphoneos-version-min=10.0 \
    "$OUT_DIR/entry.c" \
    "$OUT_DIR/$ENTRY_ID-aarch64-apple-ios.o" \
    "$TARGET_DIR/aarch64-apple-ios/release/$RUNTIME_LIB" \
    -o "$OUT_DIR/$ENTRY_ID-ios"

# 3. Android: cross-build the runtime static library and link with NDK clang.
if [ -z "${ANDROID_NDK_HOME:-}" ]; then
    echo "error: ANDROID_NDK_HOME is not set; it must point to an Android NDK installation" >&2
    exit 1
fi
NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin"
ANDROID_CC="$NDK_BIN/aarch64-linux-android24-clang"
if [ ! -x "$ANDROID_CC" ]; then
    echo "error: NDK clang not found at $ANDROID_CC" >&2
    exit 1
fi
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$ANDROID_CC"
export CC_aarch64_linux_android="$ANDROID_CC"
export AR_aarch64_linux_android="$NDK_BIN/llvm-ar"
cargo build --offline --release -p subscript-runtime --target aarch64-linux-android
"$ANDROID_CC" \
    "$OUT_DIR/entry.c" \
    "$OUT_DIR/$ENTRY_ID-aarch64-linux-android.o" \
    "$TARGET_DIR/aarch64-linux-android/release/$RUNTIME_LIB" \
    -o "$OUT_DIR/$ENTRY_ID-android"

# 4. Report. The binaries are never executed (compile+link is the criterion).
file "$OUT_DIR/$ENTRY_ID-ios" "$OUT_DIR/$ENTRY_ID-android"

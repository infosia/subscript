#!/bin/sh
# P0.5 mobile link spike — object emission and device-triple link
# (specs/blocks/compiler.md §3). Compile+link is the whole criterion;
# this script never executes the produced binaries.
#
# Environment variables:
#   ANDROID_NDK_HOME  (required) Android NDK installation root. The script
#                     uses its darwin-x86_64 prebuilt LLVM toolchain:
#                     $ANDROID_NDK_HOME/toolchains/llvm/prebuilt/darwin-x86_64/bin
#
# Requirements: rustup targets aarch64-linux-android and aarch64-apple-ios,
# Xcode command line tools (xcrun, iphoneos SDK), and a populated cargo
# cache (all cargo invocations run --offline).
#
# All paths are resolved relative to this script's directory.

set -eu

SPIKE_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SPIKE_DIR/../.." && pwd)
OUT_DIR="$SPIKE_DIR/out"
TARGET_DIR="$REPO_ROOT/target"
STUB_NAME=libsubscript_runtime_stub.a

# 1. Build the emitter and produce both objects into out/.
cargo build --offline --release --manifest-path "$SPIKE_DIR/Cargo.toml"
"$TARGET_DIR/release/mobile-link-spike" "$OUT_DIR"

# 2. Android: cross-build the runtime stub and link with NDK clang.
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
(cd "$SPIKE_DIR/runtime-stub" && cargo build --offline --release --target aarch64-linux-android)
"$ANDROID_CC" \
    "$OUT_DIR/spike-aarch64-linux-android.o" \
    "$SPIKE_DIR/main.c" \
    "$TARGET_DIR/aarch64-linux-android/release/$STUB_NAME" \
    -o "$OUT_DIR/spike-android"

# 3. iOS: cross-build the runtime stub and link with Xcode clang.
(cd "$SPIKE_DIR/runtime-stub" && cargo build --offline --release --target aarch64-apple-ios)
xcrun --sdk iphoneos clang -target arm64-apple-ios -miphoneos-version-min=10.0 \
    "$OUT_DIR/spike-aarch64-apple-ios.o" \
    "$SPIKE_DIR/main.c" \
    "$TARGET_DIR/aarch64-apple-ios/release/$STUB_NAME" \
    -o "$OUT_DIR/spike-ios"

# 4. Report. Binaries are not executed (compile+link is the criterion).
file "$OUT_DIR/spike-android" "$OUT_DIR/spike-ios"

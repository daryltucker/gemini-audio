#!/bin/bash
# Generate Kotlin bindings from Rust FFI
# Called by Gradle task generateKotlinBindings

set -e

PROJECT_DIR="/home/daryl/Projects/NRG/gemini-audio"
ANDROID_DIR="$PROJECT_DIR/android"
UNIFFI="$PROJECT_DIR/target/release/uniffi-bindgen"
LIB_PATH="$PROJECT_DIR/target/release/deps/libgemini_audio_core.so"

# Build Rust core
cd "$PROJECT_DIR/core"
cargo build --release -p gemini-audio-core

# Generate bindings
"$UNIFFI" generate \
    --library "$LIB_PATH" \
    --language kotlin \
    --out-dir "$ANDROID_DIR/app/build/generated/uniffi"

# Copy to source directory
cp -r "$ANDROID_DIR/app/build/generated/uniffi/uniffi/gemini_audio_core/"* \
    "$ANDROID_DIR/app/src/main/java/uniffi/gemini_audio_core/"

echo "Kotlin bindings generated successfully"

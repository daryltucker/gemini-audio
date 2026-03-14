# Gemini Audio Makefile

BINARY_NAME=gemini-audio
PREFIX?=/usr/local
INSTALL_DIR=$(PREFIX)/bin
CONFIG_DIR=$(HOME)/.config/gemini-audio/conversations

GEMINI_DATA_DIR=$(HOME)/.local/share/gemini-audio
LOG_FILE=$(GEMINI_DATA_DIR)/logs/gemini-audio.log

.PHONY: all build check test clean install uninstall run-tui logs android-build android-install android-core android-qr

all: build test

build:
	@echo "Building $(BINARY_NAME) in release mode..."
	cargo build --release -p gemini-audio

check:
	@echo "Checking for compile errors..."
	cargo check --workspace 2>&1

test:
	@echo "Running tests..."
	cargo test --workspace  # agent 

clean:
	@echo "Cleaning build artifacts..."
	cargo clean  # agent 

install: build
	@echo "Installing $(BINARY_NAME) using cargo..."
	@mkdir -p $(CONFIG_DIR)
	cargo install --path app
	@echo "Installation complete. Ensure ~/.cargo/bin is in your PATH."

uninstall:
	@echo "Removing $(BINARY_NAME) using cargo..."
	cargo uninstall $(BINARY_NAME)
	@echo "Uninstallation complete."

run-tui:
	@echo "Launching Gemini Audio TUI..."
	GEMINI_API_KEY=$(GEMINI_API_KEY_FREE) cargo run --release -p gemini-audio -- --tui --log-level DEBUG  # agent

logs:
	@echo "Log: $(LOG_FILE)"
	@tail -f "$(LOG_FILE)" 2>/dev/null || echo "No log file yet — run the app first"

# Help target
help:
	@echo "Usage: make [target]"
	@echo ""
	@echo "Desktop Targets:"
	@echo "  build            Build the desktop binary in release mode"
	@echo "  test             Run unit tests"
	@echo "  install          Build and install the binary"
	@echo "  uninstall        Remove the binary"
	@echo "  clean            Remove build artifacts"
	@echo "  run-tui          Build and launch the TUI directly"
	@echo ""
	@echo "Android Targets:"
	@echo "  android-core       Cross-compile Rust core for arm64-v8a"
	@echo "  android-bindings   Generate Kotlin bindings from Rust FFI"
	@echo "  android-build     Build Android APK (debug)"
	@echo "  android-install   Build and install APK to connected device"
	@echo "  android-qr        Print QR code for API key import (requires qrencode)"

# ── Android ───────────────────────────────────────────────────────────────────

ANDROID_SDK_HOME ?= $(HOME)/.src/android-sdk-linux
ANDROID_NDK_HOME ?= $(ANDROID_SDK_HOME)/ndk/27.2.12479018

android-core:
	@echo "Cross-compiling gemini-audio-core for arm64-v8a..."
	ANDROID_NDK_HOME=$(ANDROID_NDK_HOME) cargo ndk -t arm64-v8a -o android/app/src/main/jniLibs build -p gemini-audio-core --release

android-bindings:
	@echo "Generating Kotlin bindings from Rust FFI..."
	./android/gradlew -p android :app:generateKotlinBindings

android-build: android-core android-bindings
	@echo "Building Android APK..."
	cd android && ANDROID_HOME=$(ANDROID_SDK_HOME) ./gradlew assembleDebug

android-install: android-build
	@echo "Installing APK to device..."
	adb install -r android/app/build/outputs/apk/debug/app-debug.apk

android-qr:
	@echo "Point your phone camera at this QR code to import your API key:"
	@qrencode -t UTF8 "GEMINI_API_KEY=$(GEMINI_API_KEY)"


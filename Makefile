# Gemini Audio Makefile

BINARY_NAME=gemini-audio
PREFIX?=/usr/local
INSTALL_DIR=$(PREFIX)/bin
CONFIG_DIR=$(HOME)/.config/gemini-audio/conversations

GEMINI_DATA_DIR=$(HOME)/.local/share/gemini-audio
LOG_FILE=$(GEMINI_DATA_DIR)/logs/gemini-audio.log

.PHONY: all build check test clean install uninstall run-tui logs

all: build test

build:
	@echo "Building $(BINARY_NAME) in release mode..."
	cargo build --release

check:
	@echo "Checking for compile errors..."
	cargo check 2>&1

test:
	@echo "Running tests..."
	cargo test  # agent 

clean:
	@echo "Cleaning build artifacts..."
	cargo clean  # agent 

install: build
	@echo "Installing $(BINARY_NAME) using cargo..."
	@mkdir -p $(CONFIG_DIR)
	cargo install --path .
	@echo "Installation complete. Ensure ~/.cargo/bin is in your PATH."

uninstall:
	@echo "Removing $(BINARY_NAME) using cargo..."
	cargo uninstall $(BINARY_NAME)
	@echo "Uninstallation complete."

run-tui:
	@echo "Launching Gemini Audio TUI..."
	GEMINI_API_KEY=$(GEMINI_API_KEY_FREE) cargo run --release -- --tui --log-level DEBUG  # agent

logs:
	@echo "Log: $(LOG_FILE)"
	@tail -f "$(LOG_FILE)" 2>/dev/null || echo "No log file yet — run the app first"

# Help target
help:
	@echo "Usage: make [target]"
	@echo ""
	@echo "Targets:"
	@echo "  build      Build the project in release mode"
	@echo "  test       Run unit tests"
	@echo "  install    Build and install the binary to $(INSTALL_DIR)"
	@echo "  uninstall  Remove the binary from $(INSTALL_DIR)"
	@echo "  clean      Remove build artifacts"
	@echo "  run-tui    Build and launch the TUI directly"

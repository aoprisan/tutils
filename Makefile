# tutils - Terminal utilities
# Build system for static binary compilation

.PHONY: all build release clean install test lint fmt check help
.PHONY: static-linux static-linux-arm static-macos static-macos-arm
.PHONY: tmail tweb setup-musl

# Default target
all: release

# Detect OS and architecture
UNAME_S := $(shell uname -s)
UNAME_M := $(shell uname -m)

# Output directories
BUILD_DIR := target
RELEASE_DIR := $(BUILD_DIR)/release
DIST_DIR := dist

# Binaries
BINARIES := tmail tweb

#===============================================================================
# Development builds
#===============================================================================

build:
	cargo build --workspace

release:
	cargo build --release --workspace

#===============================================================================
# Static builds (Linux with musl)
#===============================================================================

# Install musl target
setup-musl:
	rustup target add x86_64-unknown-linux-musl
	rustup target add aarch64-unknown-linux-musl
	@echo "Note: You may need to install musl-tools:"
	@echo "  Debian/Ubuntu: sudo apt-get install musl-tools"
	@echo "  Fedora: sudo dnf install musl-gcc"
	@echo "  Alpine: apk add musl-dev"

static-linux:
	cargo build --release --target x86_64-unknown-linux-musl --workspace
	@mkdir -p $(DIST_DIR)/linux-x86_64
	@for bin in $(BINARIES); do \
		cp $(BUILD_DIR)/x86_64-unknown-linux-musl/release/$$bin $(DIST_DIR)/linux-x86_64/; \
	done
	@echo "Static binaries built in $(DIST_DIR)/linux-x86_64/"

static-linux-arm:
	cargo build --release --target aarch64-unknown-linux-musl --workspace
	@mkdir -p $(DIST_DIR)/linux-aarch64
	@for bin in $(BINARIES); do \
		cp $(BUILD_DIR)/aarch64-unknown-linux-musl/release/$$bin $(DIST_DIR)/linux-aarch64/; \
	done
	@echo "Static binaries built in $(DIST_DIR)/linux-aarch64/"

#===============================================================================
# macOS builds (as static as possible)
#===============================================================================

static-macos:
ifeq ($(UNAME_S),Darwin)
	cargo build --release --workspace
	@mkdir -p $(DIST_DIR)/macos-$(UNAME_M)
	@for bin in $(BINARIES); do \
		cp $(RELEASE_DIR)/$$bin $(DIST_DIR)/macos-$(UNAME_M)/; \
	done
	@echo "Binaries built in $(DIST_DIR)/macos-$(UNAME_M)/"
else
	@echo "macOS builds must be done on macOS"
endif

static-macos-arm:
ifeq ($(UNAME_S),Darwin)
	cargo build --release --target aarch64-apple-darwin --workspace
	@mkdir -p $(DIST_DIR)/macos-aarch64
	@for bin in $(BINARIES); do \
		cp $(BUILD_DIR)/aarch64-apple-darwin/release/$$bin $(DIST_DIR)/macos-aarch64/; \
	done
else
	@echo "macOS builds must be done on macOS"
endif

#===============================================================================
# Individual binary builds
#===============================================================================

tmail:
	cargo build --release -p tmail

tweb:
	cargo build --release -p tweb

#===============================================================================
# Testing and quality
#===============================================================================

test:
	cargo test --workspace

lint:
	cargo clippy --workspace --all-targets -- -D warnings

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

check: fmt-check lint test
	@echo "All checks passed!"

#===============================================================================
# Installation
#===============================================================================

PREFIX ?= /usr/local

install: release
	@mkdir -p $(PREFIX)/bin
	@for bin in $(BINARIES); do \
		install -m 755 $(RELEASE_DIR)/$$bin $(PREFIX)/bin/; \
		echo "Installed $$bin to $(PREFIX)/bin/"; \
	done

install-static: static-linux
	@mkdir -p $(PREFIX)/bin
	@for bin in $(BINARIES); do \
		install -m 755 $(DIST_DIR)/linux-x86_64/$$bin $(PREFIX)/bin/; \
		echo "Installed $$bin to $(PREFIX)/bin/"; \
	done

uninstall:
	@for bin in $(BINARIES); do \
		rm -f $(PREFIX)/bin/$$bin; \
		echo "Removed $$bin from $(PREFIX)/bin/"; \
	done

#===============================================================================
# Cleaning
#===============================================================================

clean:
	cargo clean
	rm -rf $(DIST_DIR)

#===============================================================================
# Help
#===============================================================================

help:
	@echo "tutils - Terminal Utilities Build System"
	@echo ""
	@echo "Development:"
	@echo "  make build          - Debug build"
	@echo "  make release        - Release build"
	@echo "  make tmail          - Build only tmail"
	@echo "  make tweb           - Build only tweb"
	@echo ""
	@echo "Static builds:"
	@echo "  make setup-musl     - Install musl targets"
	@echo "  make static-linux   - Static build for Linux x86_64"
	@echo "  make static-linux-arm - Static build for Linux aarch64"
	@echo "  make static-macos   - Build for macOS (current arch)"
	@echo ""
	@echo "Quality:"
	@echo "  make test           - Run tests"
	@echo "  make lint           - Run clippy"
	@echo "  make fmt            - Format code"
	@echo "  make check          - Run all checks"
	@echo ""
	@echo "Installation:"
	@echo "  make install        - Install to PREFIX (default: /usr/local)"
	@echo "  make install-static - Install static binaries"
	@echo "  make uninstall      - Remove installed binaries"
	@echo ""
	@echo "Other:"
	@echo "  make clean          - Clean build artifacts"
	@echo "  make help           - Show this help"

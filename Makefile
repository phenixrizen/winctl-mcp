SHELL := /bin/bash

# Linux/WSL host tools for building Windows binaries
CARGO ?= cargo
RUSTUP ?= rustup
WINDOWS_TARGET ?= x86_64-pc-windows-gnu
RELEASE ?= 1
VERSION ?= $(shell awk -F\" '/^version = / { print $$2; exit }' Cargo.toml)
DIST_DIR ?= dist/winctl-mcp-$(VERSION)-windows-$(WINDOWS_TARGET)

ifeq ($(RELEASE),1)
  PROFILE_FLAG := --release
  PROFILE_DIR := release
else
  PROFILE_FLAG :=
  PROFILE_DIR := debug
endif

.PHONY: help
help:
	@echo "Targets:"
	@echo "  setup-win-target    Install Rust Windows target"
	@echo "  fmt                 Run rustfmt"
	@echo "  test                Run workspace tests"
	@echo "  build-linux         Build workspace for host (Linux)"
	@echo "  build-win           Build workspace for Windows target"
	@echo "  build-win-server    Build only winctl-mcp-server for Windows"
	@echo "  build-win-tray      Build only winctl-tray for Windows"
	@echo "  build-win-fixture   Build the Windows integration fixture"
	@echo "  package-win         Package Windows binaries, docs, scripts, metadata, and checksums"
	@echo "  check               Run fmt + test + build-linux"
	@echo "  print-artifacts     Show expected Windows artifact paths"

.PHONY: setup-win-target
setup-win-target:
	$(RUSTUP) target add $(WINDOWS_TARGET)

.PHONY: fmt
fmt:
	$(CARGO) fmt --all

.PHONY: test
test:
	$(CARGO) test --workspace

.PHONY: build-linux
build-linux:
	$(CARGO) build --workspace $(PROFILE_FLAG)

.PHONY: build-win
build-win:
	$(CARGO) build --workspace --target $(WINDOWS_TARGET) $(PROFILE_FLAG)

.PHONY: build-win-server
build-win-server:
	$(CARGO) build -p winctl-mcp-server --target $(WINDOWS_TARGET) $(PROFILE_FLAG)

.PHONY: build-win-tray
build-win-tray:
	$(CARGO) build -p winctl-tray --target $(WINDOWS_TARGET) $(PROFILE_FLAG)

.PHONY: build-win-fixture
build-win-fixture:
	$(CARGO) build -p winctl-test-target --target $(WINDOWS_TARGET) $(PROFILE_FLAG)

.PHONY: package-win
package-win: build-win-server build-win-tray
	bash scripts/package-windows-release.sh "$(WINDOWS_TARGET)" "$(PROFILE_DIR)" "$(DIST_DIR)" "$(VERSION)"

.PHONY: check
check: fmt test build-linux

.PHONY: print-artifacts
print-artifacts:
	@echo "Server binary path:"
	@echo "  target/$(WINDOWS_TARGET)/$(PROFILE_DIR)/winctl-mcp-server.exe"
	@echo "Integration fixture binary path:"
	@echo "  target/$(WINDOWS_TARGET)/$(PROFILE_DIR)/winctl-test-target.exe"
	@echo "Package directory:"
	@echo "  $(DIST_DIR)"

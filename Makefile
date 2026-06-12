SHELL := /bin/bash

# Windows release builds use the MSVC target. GNU remains available as a local
# compatibility override when a WSL cross-link path is needed.
CARGO ?= cargo
RUSTUP ?= rustup
NPM ?= npm
WINDOWS_TARGET ?= x86_64-pc-windows-msvc
RELEASE ?= 1
VERSION ?= $(shell awk -F\" '/^version = / { print $$2; exit }' Cargo.toml)
DIST_DIR ?= dist/winctl-mcp-$(VERSION)-windows-$(WINDOWS_TARGET)
MSI ?= dist/winctl-mcp-$(VERSION)-windows-x64.msi

ifeq ($(WINDOWS_TARGET),x86_64-pc-windows-msvc)
  ifneq ($(OS),Windows_NT)
    MSVC_ON_NON_WINDOWS := 1
  endif
endif

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
	@echo "  dashboard-build     Build embedded Vue dashboard assets"
	@echo "  build-linux         Build workspace for host (Linux)"
	@echo "  build-win           Build workspace for Windows target"
	@echo "  build-win-server    Build only winctl-mcp-server for Windows"
	@echo "  build-win-tray      Build only winctl-tray for Windows"
	@echo "  build-win-fixture   Build the Windows integration fixture"
	@echo "  package-win         Package Windows binaries, docs, scripts, metadata, and checksums"
	@echo "  msi                 Build the Windows MSI from the packaged dist (WiX via PowerShell)"
	@echo "  install-msi         Install the locally built MSI (per-machine; triggers UAC)"
	@echo "  uninstall-msi       Uninstall winctl-mcp via its MSI UpgradeCode (triggers UAC)"
	@echo "  check               Run fmt + test + build-linux"
	@echo "  print-artifacts     Show expected Windows artifact paths"

.PHONY: setup-win-target
setup-win-target:
	$(RUSTUP) target add $(WINDOWS_TARGET)

.PHONY: require-windows-linker
require-windows-linker:
ifeq ($(MSVC_ON_NON_WINDOWS),1)
	@echo "WINDOWS_TARGET=$(WINDOWS_TARGET) requires the Windows MSVC linker."
	@echo "Run this build from Windows/MSVC or CI. From WSL/Linux, use cargo check for MSVC validation or set WINDOWS_TARGET=x86_64-pc-windows-gnu for a local compatibility build."
	@exit 1
else
	@true
endif

.PHONY: fmt
fmt:
	$(CARGO) fmt --all

.PHONY: test
test:
	$(CARGO) test --workspace

.PHONY: dashboard-build
dashboard-build:
	cd crates/winctl-mcp-server/dashboard && $(NPM) ci && $(NPM) run build

.PHONY: build-linux
build-linux:
	WINCTL_BUILD_VERSION="$(VERSION)" $(CARGO) build --workspace $(PROFILE_FLAG)

.PHONY: build-win
build-win: require-windows-linker
	WINCTL_BUILD_VERSION="$(VERSION)" $(CARGO) build --workspace --target $(WINDOWS_TARGET) $(PROFILE_FLAG)

.PHONY: build-win-server
build-win-server: require-windows-linker
	WINCTL_BUILD_VERSION="$(VERSION)" $(CARGO) build -p winctl-mcp-server --target $(WINDOWS_TARGET) $(PROFILE_FLAG)

.PHONY: build-win-tray
build-win-tray: require-windows-linker
	WINCTL_BUILD_VERSION="$(VERSION)" $(CARGO) build -p winctl-tray --target $(WINDOWS_TARGET) $(PROFILE_FLAG)

.PHONY: build-win-fixture
build-win-fixture: require-windows-linker
	WINCTL_BUILD_VERSION="$(VERSION)" $(CARGO) build -p winctl-test-target --target $(WINDOWS_TARGET) $(PROFILE_FLAG)

.PHONY: package-win
package-win: require-windows-linker dashboard-build build-win-server build-win-tray
	bash scripts/package-windows-release.sh "$(WINDOWS_TARGET)" "$(PROFILE_DIR)" "$(DIST_DIR)" "$(VERSION)"

# Windows MSI targets. These shell out to Windows tools (PowerShell, WiX, msiexec)
# via WSL interop, converting paths with `wslpath -w`. The MSI is authored
# Scope="perMachine", so install/uninstall run elevated and trigger a UAC prompt.
# On WSL the binary build needs the GNU toolchain, e.g.:
#   WINDOWS_TARGET=x86_64-pc-windows-gnu make msi
.PHONY: msi
msi: package-win
	powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$$(wslpath -w scripts/build-windows-msi.ps1)" \
		-DistDir "$$(wslpath -w '$(DIST_DIR)')" -Version "$(VERSION)" -OutputPath "$$(wslpath -w '$(MSI)')"

.PHONY: install-msi
install-msi:
	powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$$(wslpath -w scripts/windows-msi.ps1)" \
		-Action install -MsiPath "$$(wslpath -w '$(MSI)')"

.PHONY: uninstall-msi
uninstall-msi:
	powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$$(wslpath -w scripts/windows-msi.ps1)" \
		-Action uninstall

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

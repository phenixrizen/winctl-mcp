SHELL := /bin/bash

# Windows builds target MSVC: winctl-tray gates the dashboard WebView window behind
# target_env="msvc", so a GNU build cannot open the dashboard. On a Windows host the
# cargo on PATH already is the MSVC toolchain; on WSL/Linux we drive the
# Windows-native cargo through PowerShell interop so we still get real MSVC binaries.
# The GNU target stays selectable as a fast local Linux-cargo compile-check, but its
# build cannot open the dashboard window.
CARGO ?= cargo
RUSTUP ?= rustup
NPM ?= npm
WINDOWS_TARGET ?= x86_64-pc-windows-msvc
RELEASE ?= 1
VERSION ?= $(shell awk -F\" '/^version = / { print $$2; exit }' Cargo.toml)
DIST_DIR ?= dist/winctl-mcp-$(VERSION)-windows-$(WINDOWS_TARGET)
MSI ?= dist/winctl-mcp-$(VERSION)-windows-x64.msi

ifeq ($(RELEASE),1)
  PROFILE_FLAG := --release
  PROFILE_DIR := release
else
  PROFILE_FLAG :=
  PROFILE_DIR := debug
endif

# $(call win_cargo,<cargo args>) runs cargo for a Windows-target build:
#  - Windows host: the cargo on PATH already is the Windows toolchain.
#  - WSL + MSVC target (default): the Windows-native cargo via PowerShell interop,
#    with the working dir set to this repo's Windows (\\wsl.localhost\...) path.
#  - WSL + GNU target: the local Linux cargo links GNU fine (fast compile-check).
ifeq ($(OS),Windows_NT)
  win_cargo = WINCTL_BUILD_VERSION="$(VERSION)" $(CARGO) $(1)
else ifeq ($(WINDOWS_TARGET),x86_64-pc-windows-msvc)
  REPO_WIN := $(shell wslpath -w .)
  win_cargo = powershell.exe -NoProfile -Command 'Set-Location "$(REPO_WIN)"; $$env:WINCTL_BUILD_VERSION="$(VERSION)"; cargo $(1)'
else
  win_cargo = WINCTL_BUILD_VERSION="$(VERSION)" $(CARGO) $(1)
endif

.PHONY: help
help:
	@echo "Targets:"
	@echo "  setup-win-target    Add the Rust Windows target to the Linux rustup (for cargo check)"
	@echo "  fmt                 Run rustfmt"
	@echo "  test                Run workspace tests"
	@echo "  dashboard-build     Build embedded Vue dashboard assets"
	@echo "  build-linux         Build workspace for host (Linux)"
	@echo "  build-win           Build workspace for Windows (MSVC via Windows cargo on WSL)"
	@echo "  build-win-server    Build only winctl-mcp-server for Windows"
	@echo "  build-win-tray      Build only winctl-tray for Windows"
	@echo "  build-win-fixture   Build the Windows integration fixture"
	@echo "  package-win         Package Windows binaries, docs, scripts, metadata, and checksums"
	@echo "  msi                 Build the Windows MSI from the packaged dist (WiX via PowerShell)"
	@echo "  install-msi         Rebuild and (re)install the MSI in one elevation (UAC)"
	@echo "  uninstall-msi       Uninstall winctl-mcp via its MSI UpgradeCode (triggers UAC)"
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

.PHONY: dashboard-build
dashboard-build:
	cd crates/winctl-mcp-server/dashboard && $(NPM) ci && $(NPM) run build

.PHONY: build-linux
build-linux:
	WINCTL_BUILD_VERSION="$(VERSION)" $(CARGO) build --workspace $(PROFILE_FLAG)

.PHONY: build-win
build-win:
	$(call win_cargo,build --workspace --target $(WINDOWS_TARGET) $(PROFILE_FLAG))

.PHONY: build-win-server
build-win-server:
	$(call win_cargo,build -p winctl-mcp-server --target $(WINDOWS_TARGET) $(PROFILE_FLAG))

.PHONY: build-win-tray
build-win-tray:
	$(call win_cargo,build -p winctl-tray --target $(WINDOWS_TARGET) $(PROFILE_FLAG))

.PHONY: build-win-fixture
build-win-fixture:
	$(call win_cargo,build -p winctl-test-target --target $(WINDOWS_TARGET) $(PROFILE_FLAG))

.PHONY: package-win
package-win: dashboard-build build-win-server build-win-tray
	bash scripts/package-windows-release.sh "$(WINDOWS_TARGET)" "$(PROFILE_DIR)" "$(DIST_DIR)" "$(VERSION)"

# Windows MSI targets. On WSL these drive the Windows-native tools (cargo via the
# build-win-* targets, plus WiX and msiexec) through PowerShell interop, converting
# paths with `wslpath -w`, so the installed build is a real MSVC build (the dashboard
# WebView requires it). The MSI is Scope="perMachine", so install/uninstall run
# elevated and trigger a UAC prompt.
.PHONY: msi
msi: package-win
	powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$$(wslpath -w scripts/build-windows-msi.ps1)" \
		-DistDir "$$(wslpath -w '$(DIST_DIR)')" -Version "$(VERSION)" -OutputPath "$$(wslpath -w '$(MSI)')"

# Depends on `msi` so it always installs a freshly built MSI (never a stale one left
# in dist/). The helper forces a full reinstall so the freshly built files land.
.PHONY: install-msi
install-msi: msi
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

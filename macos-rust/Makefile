# Makefile - TokenMon Rust macOS client (beta; issue #89). Separate bundle id,
# data directory and Keychain items from the Swift app in ../macos.
.PHONY: default help build run stop test pkg clean

default: help

ROOT       := $(abspath $(CURDIR)/..)
APP_NAME   := TokenMon (Rust beta)
BUNDLE_ID  := com.modelmonitor.app.rust
OUT_DIR    := $(ROOT)/target/rust-beta
APP        := $(OUT_DIR)/$(APP_NAME).app
VERSION    ?= $(shell sed -n 's/^version *= *"\([^"]*\)".*/\1/p' $(ROOT)/Cargo.toml | head -1)
VERSION    := $(if $(VERSION),$(VERSION),0.0.0)
PKG        := $(OUT_DIR)/TokenMon-RustBeta-$(VERSION).pkg
# Optional: set DEVELOPER_ID_INSTALLER to sign the .pkg (unsigned otherwise).

help: ## List targets
	@grep -E '^[a-z-]+:.*##' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-8s %s\n", $$1, $$2}'

build: ## Build the beta .app (ad-hoc signed) into target/rust-beta
	@$(CURDIR)/scripts/bundle-rust-beta.sh

test: ## Run the macOS client crate tests
	@cd $(ROOT) && cargo test -p tokenmon-macos

run: build ## Build and launch the beta against a local mock OpenRouter (no real credentials)
	@pkill -f "$(OUT_DIR)" 2>/dev/null || true
	@"$(APP)/Contents/MacOS/tokenmon-macos" --mock-openrouter >/tmp/tokenmon-rust-beta.log 2>&1 &
	@printf "Rust beta running in the menu bar (log: /tmp/tokenmon-rust-beta.log). Stop it with: make -C macos-rust stop\n"

stop: ## Quit the beta
	@pkill -f "$(OUT_DIR)" 2>/dev/null || true

pkg: build ## Build an installer .pkg (unsigned unless DEVELOPER_ID_INSTALLER is set)
	@rm -rf "$(OUT_DIR)/pkgroot" && mkdir -p "$(OUT_DIR)/pkgroot/Applications"
	@ditto "$(APP)" "$(OUT_DIR)/pkgroot/Applications/$(APP_NAME).app"
	@pkgbuild --analyze --root "$(OUT_DIR)/pkgroot" "$(OUT_DIR)/component.plist" >/dev/null
	@plutil -replace 0.BundleIsRelocatable -bool NO "$(OUT_DIR)/component.plist"
	@if [ -n "$(DEVELOPER_ID_INSTALLER)" ]; then \
		pkgbuild --root "$(OUT_DIR)/pkgroot" --component-plist "$(OUT_DIR)/component.plist" \
			--identifier "$(BUNDLE_ID)" --version "$(VERSION)" --install-location "/" \
			--sign "$(DEVELOPER_ID_INSTALLER)" "$(PKG)"; \
	else \
		pkgbuild --root "$(OUT_DIR)/pkgroot" --component-plist "$(OUT_DIR)/component.plist" \
			--identifier "$(BUNDLE_ID)" --version "$(VERSION)" --install-location "/" "$(PKG)"; \
		echo "Package is unsigned (no DEVELOPER_ID_INSTALLER); the app inside is ad-hoc signed."; \
	fi
	@rm -rf "$(OUT_DIR)/pkgroot" "$(OUT_DIR)/component.plist"
	@echo "Package: $(PKG)"

clean: ## Remove target/rust-beta
	@rm -rf "$(OUT_DIR)"

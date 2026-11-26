# Makefile — the shared Rust code of TokenMon's desktop ports (common/)
#
# Builds, tests and checks the whole Cargo workspace at the repository root
# (the shared crates in common/crates plus the per-OS crates that compile on
# this host), without any OS-specific packaging. Run it from this directory:
#
#   cd common
#   make            # list every target
#   make clippy     # the gate before every push
#   make test
#
# The Windows app has its own entry point in windows/Makefile (run, installer,
# MSI, zip); its `clippy`, `test` and `coverage` targets do the same work as
# the ones here.

.DEFAULT_GOAL := help
SHELL   := /bin/sh
.SHELLFLAGS := -ec

CARGO  ?= cargo
PYTHON ?= python3
LOCKED := --locked
ROOT          := ..
TARGET_DIR    := $(ROOT)/target
SCRIPTS       := scripts
FUZZ_MANIFEST := fuzz/Cargo.toml
COV_DIR       := $(TARGET_DIR)/llvm-cov

.PHONY: help test clippy lint fmt fuzz-check parity coverage

help: ## Show this help (default)
	@printf "\nTokenMon shared Rust code (common/)\n\n"
	@awk 'BEGIN { FS = ":.*## " } /^[a-zA-Z_-]+:.*## / { printf "  %-12s %s\n", $$1, $$2 }' $(MAKEFILE_LIST)
	@printf "\nWindows packaging and running the app: windows/Makefile\n\n"

test: ## cargo test --workspace
	$(CARGO) test $(LOCKED) --workspace

clippy: ## cargo fmt, then clippy -D warnings on every target, fuzz harnesses included
	$(CARGO) fmt --all
	$(CARGO) fmt --manifest-path $(FUZZ_MANIFEST)
	$(CARGO) clippy $(LOCKED) --workspace --all-targets -- -D warnings
	$(CARGO) clippy --manifest-path $(FUZZ_MANIFEST) --all-targets -- -D warnings
	@echo "ok  clippy clean"

lint: ## fmt check, clippy -D warnings (fuzz harnesses too), cargo deny (if installed)
	$(CARGO) fmt --all -- --check
	$(CARGO) fmt --manifest-path $(FUZZ_MANIFEST) -- --check
	$(CARGO) clippy $(LOCKED) --workspace --all-targets -- -D warnings
	$(CARGO) clippy --manifest-path $(FUZZ_MANIFEST) --all-targets -- -D warnings
	@if $(CARGO) deny --version >/dev/null 2>&1; then \
		$(CARGO) deny --manifest-path $(ROOT)/Cargo.toml check advisories bans licenses sources; \
	else \
		echo "cargo-deny not installed; skipped (cargo install cargo-deny --locked)"; \
	fi
	@echo "ok  lint clean"

fmt: ## cargo fmt --all
	$(CARGO) fmt --all

fuzz-check: ## Type-check the fuzz crate on stable (cargo check fuzz --bins)
	$(CARGO) check --manifest-path $(FUZZ_MANIFEST) --bins

parity: ## Check the Swift-to-Rust test parity ledger (windows/docs/PARITY.md)
	$(PYTHON) -I $(SCRIPTS)/parity/check_parity.py

coverage: ## Line coverage per crate like CI, minus the glue in .config/coverage-excludes.txt
	@$(CARGO) llvm-cov --version >/dev/null 2>&1 || { \
		echo "cargo-llvm-cov is not installed: rustup component add llvm-tools-preview && cargo install cargo-llvm-cov --locked" >&2; exit 1; }
	@$(PYTHON) -I $(SCRIPTS)/ci/coverage_excludes.py check --root $(ROOT)
	@$(PYTHON) -I $(SCRIPTS)/ci/prune_stale_test_binaries.py $(TARGET_DIR)/llvm-cov-target
	CARGO_INCREMENTAL=0 $(CARGO) llvm-cov $(LOCKED) --workspace --no-report
	@mkdir -p "$(COV_DIR)"
	@ignore=$$($(PYTHON) -I $(SCRIPTS)/ci/coverage_excludes.py regex --root $(ROOT)) && \
	$(CARGO) llvm-cov report --ignore-filename-regex "$$ignore" --json --summary-only \
		--output-path "$(COV_DIR)/cov.json" && \
	$(CARGO) llvm-cov report --ignore-filename-regex "$$ignore" --html --output-dir "$(COV_DIR)"
	$(PYTHON) -I $(SCRIPTS)/ci/render_coverage.py --json "$(COV_DIR)/cov.json" --root $(ROOT) \
		--floors $(ROOT)/.config/coverage-floors.toml --excludes $(ROOT)/.config/coverage-excludes.txt \
		--title "Coverage (local)"
	@echo "ok  HTML report: $(COV_DIR)/html/index.html"

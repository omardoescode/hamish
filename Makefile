# Hamish developer commands. Run `make help` for the list.
CARGO ?= cargo
APP := hamish

.DEFAULT_GOAL := help
.PHONY: help build build-release run run-release install uninstall test test\:unit test\:integration test\:doc \
	fmt fmt\:check lint check deny shear guards bench dist clean ci

help: ## Show this list
	@grep -E '^[a-zA-Z_\\:-]+:.*## ' $(MAKEFILE_LIST) | sed -E 's/^(([^:\\]|\\:)+):.*## /\1\t/; s/\\:/:/g' | awk -F'\t' '{printf "  %-18s %s\n", $$1, $$2}'

build: ## Debug build
	$(CARGO) build --workspace

build-release: ## Release build
	$(CARGO) build --workspace --release

run: ## Run the app (debug)
	$(CARGO) run -p $(APP)

run-release: ## Run the app (release)
	$(CARGO) run -p $(APP) --release

install: ## Install the hamish binary into ~/.cargo/bin
	$(CARGO) install --path crates/app --locked

uninstall: ## Remove the installed hamish binary
	$(CARGO) uninstall $(APP)

test: ## All tests
	$(CARGO) test --workspace

test\:unit: ## Unit tests (lib and bin)
	$(CARGO) test --workspace --lib --bins

test\:integration: ## Integration tests (tests/)
	$(CARGO) test --workspace --test '*'

test\:doc: ## Doc tests
	$(CARGO) test --workspace --doc

fmt: ## Format the code
	$(CARGO) fmt --all

fmt\:check: ## Check the format
	$(CARGO) fmt --all -- --check

lint: ## Clippy, warnings are errors
	$(CARGO) clippy --workspace --all-targets -- -D warnings

check: ## Type-check
	$(CARGO) check --workspace --all-targets

deny: ## Licenses, bans, advisories
	$(CARGO) deny check

shear: ## Unused dependencies
	$(CARGO) shear

guards: ## Source guards (comments, glossary, file size)
	perl scripts/source-guards.pl

bench: ## Benchmarks
	$(CARGO) bench --workspace

dist: ## Build release archives for this machine with cargo-dist
	dist build

clean: ## Remove build output
	$(CARGO) clean

ci: fmt\:check lint test guards deny shear ## All checks, as in CI and the git hooks

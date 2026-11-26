# Hamish developer commands. Run `make help` for the list.
CARGO ?= cargo
APP := hamish
PREFIX ?= $(HOME)/.local
OS := $(shell uname -s)

.DEFAULT_GOAL := help
.PHONY: help build build-release run run-release install uninstall icons test test\:unit test\:integration test\:doc \
	test\:packaging fmt fmt\:check lint check deny shear guards bench dist clean ci

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

ifeq ($(OS),Darwin)
VERSION := $(shell grep -m 1 '^version = ' Cargo.toml | cut -d '"' -f 2)

install:
	$(CARGO) build -p $(APP) --release --locked
	sh packaging/macos/make-app.sh target/release/$(APP) "$(HOME)/Applications" "$(VERSION)"

uninstall:
	rm -rf "$(HOME)/Applications/Hamish.app"
else
install: ## Install Hamish as a desktop app (Linux: into PREFIX, default ~/.local; macOS: ~/Applications/Hamish.app)
	$(CARGO) build -p $(APP) --release --locked
	PREFIX="$(PREFIX)" HAMISH_BIN=target/release/$(APP) HAMISH_DESKTOP=packaging/linux/hamish.desktop \
		HAMISH_ICONS=packaging/icons sh packaging/linux/install.sh

uninstall: ## Remove the desktop app that install made
	PREFIX="$(PREFIX)" sh packaging/linux/install.sh --uninstall
endif

icons: ## Render the PNG icons and the macOS .icns from packaging/icons/hamish.svg
	sh scripts/render-icons.sh

test: ## All tests
	$(CARGO) test --workspace

test\:unit: ## Unit tests (lib and bin)
	$(CARGO) test --workspace --lib --bins

test\:integration: ## Integration tests (tests/)
	$(CARGO) test --workspace --test '*'

test\:doc: ## Doc tests
	$(CARGO) test --workspace --doc

test\:packaging: ## Desktop packaging (install script, Hamish.app) with a stub binary
	sh scripts/packaging-test.sh

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

ci: fmt\:check lint test test\:packaging guards deny shear ## All checks, as in CI and the git hooks

.PHONY: help build test bench fmt check clean doc run-example install-tools

# Default target
help:
	@echo "Matching Engine - Development Commands"
	@echo ""
	@echo "Building:"
	@echo "  make build          - Build the project"
	@echo "  make build-release  - Build with optimizations"
	@echo ""
	@echo "Testing:"
	@echo "  make test           - Run all tests"
	@echo "  make test-unit      - Run unit tests only"
	@echo "  make test-int       - Run integration tests"
	@echo ""
	@echo "Benchmarking:"
	@echo "  make bench          - Run all benchmarks"
	@echo ""
	@echo "Code Quality:"
	@echo "  make fmt            - Format code with rustfmt"
	@echo "  make fmt-check      - Check code formatting"
	@echo "  make check          - Check code compiles"
	@echo "  make clippy         - Run clippy linter"
	@echo "  make fix            - Auto-fix clippy warnings"
	@echo ""
	@echo "Documentation:"
	@echo "  make doc            - Generate documentation"
	@echo "  make doc-open       - Generate and open docs"
	@echo ""
	@echo "Examples:"
	@echo "  make run-example    - Run basic usage example"
	@echo ""
	@echo "Cleanup:"
	@echo "  make clean          - Remove build artifacts"
	@echo ""
	@echo "Setup:"
	@echo "  make install-tools  - Install development tools"

# Build targets
build:
	cargo build

build-release:
	cargo build --release

# Test targets
test:
	cargo test --all-features

test-unit:
	cargo test --lib

test-int:
	cargo test --test '*'

test-verbose:
	cargo test --all-features -- --nocapture

# Benchmarking
bench:
	cargo bench

# Formatting
fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

# Code quality checks
check:
	cargo check --all-features

clippy:
	cargo clippy --all-features -- -D warnings

clippy-pedantic:
	cargo clippy --all-features -- -W clippy::pedantic

fix:
	cargo fix --allow-dirty --allow-staged
	cargo clippy --fix --allow-dirty --allow-staged

# Documentation
doc:
	cargo doc --no-deps --all-features

doc-open:
	cargo doc --no-deps --all-features --open

# Examples
run-example:
	cargo run --example basic_usage

# Cleanup
clean:
	cargo clean
	rm -rf target/

# Development tools installation
install-tools:
	@echo "Installing Rust development tools..."
	rustup component add rustfmt
	rustup component add clippy
	cargo install cargo-watch
	cargo install cargo-edit
	@echo "Tools installed successfully!"

# Watch mode (requires cargo-watch)
watch:
	cargo watch -x check -x test -x run

# CI target - runs all checks
ci: fmt-check clippy test
	@echo "All CI checks passed!"

# Pre-commit target
pre-commit: fmt clippy test
	@echo "Pre-commit checks passed!"

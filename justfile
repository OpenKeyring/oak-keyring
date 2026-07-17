# oak-keyring development tasks
# Usage: just <recipe>

# List all available recipes
default:
    @just --list

# Build debug version
build:
    cargo build

# Build release version
release:
    cargo build --release

# Run all tests
test:
    cargo test

# Run integration tests only
test-integration:
    cargo test --test integration

# lint: format check + clippy
lint:
    cargo fmt --check
    cargo clippy -- -D warnings

# One-stop pre-PR check (lint + test)
check: lint test

# Format code
fmt:
    cargo fmt

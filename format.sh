#!/bin/bash
set -e

echo "Running cargo fmt..."
cargo fmt

echo "Running cargo clippy --all..."
cargo clippy --all -- -D warnings

echo "Running cargo test..."
cargo test

echo "All checks passed!"

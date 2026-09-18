#!/usr/bin/env sh
# Vérifications à passer avant chaque commit : formatage, lints, tests.
set -eu
cd "$(dirname "$0")"

cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

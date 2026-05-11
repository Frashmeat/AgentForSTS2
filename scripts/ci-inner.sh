#!/usr/bin/env bash
# scripts/ci-inner.sh — bash side of the CI mirror.
# Called inside the Docker container by scripts/ci-local.ps1.
#
# Usage:
#   bash ci-inner.sh full      # all CI steps (check + test + tsc + build)
#   bash ci-inner.sh check     # cargo check only
#   bash ci-inner.sh clippy    # cargo clippy only

set -ex

MODE="${1:-full}"

rustc --version
cargo --version

case "$MODE" in
    check)
        cargo check --workspace --all-targets
        ;;
    clippy)
        cargo clippy --workspace --all-targets
        ;;
    full)
        node --version
        cargo check --workspace --all-targets
        cargo test --workspace --all-targets
        npm ci
        npx tsc --noEmit
        npm run build:web
        ;;
    *)
        echo "unknown mode: $MODE (expected: full / check / clippy)" >&2
        exit 2
        ;;
esac

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
        # ats-web rust-embed needs dist/. Build it if missing.
        if [ ! -d dist ]; then
            npm ci
            npm run build:web
        fi
        cargo check --workspace --all-targets
        ;;
    clippy)
        if [ ! -d dist ]; then
            npm ci
            npm run build:web
        fi
        cargo clippy --workspace --all-targets -- -D warnings
        ;;
    full)
        node --version
        # Remove dist/ first so a stale local build can't hide a CI failure.
        # ats-web rust-embed needs dist/ present, so npm steps must run before cargo.
        rm -rf dist
        npm ci
        npx tsc --noEmit
        npm run build:web
        cargo check --workspace --all-targets
        cargo test --workspace --all-targets
        # clippy 是 GitHub CI 的独立 job (-D warnings)。本地必须跑，否则会出现
        # "本地 ci-local PASSED 但远端 CI fail" 的情况（参见 commit 1f1c5ce/0a40086）。
        cargo clippy --workspace --all-targets -- -D warnings
        ;;
    *)
        echo "unknown mode: $MODE (expected: full / check / clippy)" >&2
        exit 2
        ;;
esac

#!/usr/bin/env bash
# Build the sample strategy library and run a portfolio/optimizer configuration.
# Strategy library naming is platform-specific: Cargo emits libstrategy_lib.so on Linux,
# libstrategy_lib.dylib on macOS and strategy_lib.dll on Windows/MSVC.
set -e
cd "$(dirname "$0")"
cargo build -p strategy_lib --release
cargo run --release -- --config portfolios/lshade_si_25_apr_dd.json

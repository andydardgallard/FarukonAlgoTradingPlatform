#!/usr/bin/env bash
# Build the sample strategy library and run a portfolio/optimizer configuration.
# Strategy library naming is platform-specific: Cargo emits libstrategy_lib.so on Linux,
# libstrategy_lib.dylib on macOS and strategy_lib.dll on Windows/MSVC.
set -e
cd "$(dirname "$0")"

# The committed portfolio configs carry the Windows library path. Pick the file name Cargo
# actually emits here so a Linux/macOS run loads the library it just built instead of the DLL.
case "$(uname -s)" in
    Linux*)  strategy_lib="libstrategy_lib.so" ;;
    Darwin*) strategy_lib="libstrategy_lib.dylib" ;;
    *)       strategy_lib="strategy_lib.dll" ;;
esac
strategy_path="target/release/${strategy_lib}"
portfolio_config="portfolios/lshade_si_25_apr_dd.json"

cargo build -p strategy_lib --release
if [ ! -f "$strategy_path" ]; then
    echo "error: the build did not produce $strategy_path" >&2
    exit 1
fi

# Patch only the strategy_path values of a throwaway copy next to the committed config, so the
# committed files stay untouched and their relative paths (data, commission plans) keep resolving
# exactly as before. On Windows the replacement is a no-op.
runtime_config="portfolios/.runtime_$(basename "$portfolio_config")"
trap 'rm -f "$runtime_config"' EXIT
sed "s#\(\"strategy_path\"[[:space:]]*:[[:space:]]*\)\"[^\"]*\"#\1\"${strategy_path}\"#g" \
    "$portfolio_config" > "$runtime_config"

cargo run --release -- --config "$runtime_config"

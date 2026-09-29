#!/usr/bin/env bash
# Code Factory launcher — created by prepare_factory.sh. Run it with no extra commands:
#   ./start.sh          — interactive (in the chat: /skill:code-factory)
#   ./start.sh --auto   — fully autonomous
set -euo pipefail
# Subagent model split (primary/secondary) is enabled here automatically.
export KIMI_CODE_EXPERIMENTAL_SECONDARY_MODEL=1
exec kimi "$@"

#!/usr/bin/env python3
"""
Run the Code Factory programmatically via the Kimi Agent SDK.

Two modes:
  * AUTO (default): yolo=True — everything is approved/assumed automatically.
  * HITL: yolo=False + approval_handler_fn — tool approvals are shown in the
    terminal. NOTE: interactive business questions (AskUserQuestion) are best
    handled in the CLI (`/flow:code-factory`); in SDK mode use auto mode for
    fully unattended runs, or the handler below for tool-approval prompts.

Usage:
  python3 .agents/examples/run_factory_sdk.py [task.yaml]
  # without arguments, reads TASK text below
"""
from __future__ import annotations

import asyncio
import sys
from pathlib import Path

from kaos.path import KaosPath
from kimi_agent_sdk import ApprovalRequest, prompt

PROJECT_DIR = Path(__file__).resolve().parents[2]  # project root (2 levels up from .agents/examples)

FACTORY_AGENT = PROJECT_DIR / ".agents" / "agents" / "code-factory.yaml"
SKILLS_DIR = KaosPath(PROJECT_DIR / ".agents" / "skills")

# Default task text (used when no task.yaml argument is given).
TASK_TEXT = """Fix the incorrect operation of the SYMI_Ch_SMA_up_lmt strategy
for instruments with fractional prices (CNY). See task.yaml in the project for details."""


def read_task(task_path: Path) -> str:
    return task_path.read_text(encoding="utf-8")


def make_approval_handler():
    """HITL approval handler: asks the user in the terminal before approving a tool call."""

    def handler(request: ApprovalRequest) -> None:
        print("\n--- Approval required ---")
        print(f"Tool: {request.sender}")
        print(f"Action: {request.description}")
        answer = input("Approve? [y/N] ").strip().lower()
        if answer in ("y", "yes"):
            request.resolve("approve")
        else:
            request.resolve("reject")

    return handler


async def main() -> None:
    if len(sys.argv) > 1:
        task_path = Path(sys.argv[1]).resolve()
        if not task_path.is_file():
            sys.exit(f"Task file not found: {task_path}")
        user_input = read_task(task_path)
        print(f"Running factory with task: {task_path}")
    else:
        user_input = TASK_TEXT
        print("Running factory with default task text.")

    # AUTO mode (fully unattended). Switch yolo=False and pass
    # approval_handler_fn=make_approval_handler() for HITL tool approvals.
    async for message in prompt(
        user_input,
        work_dir=KaosPath(PROJECT_DIR),
        model="kimi",
        yolo=True,
        agent_file=FACTORY_AGENT,
        skills_dir=SKILLS_DIR,
        final_message_only=False,
    ):
        text = message.extract_text()
        if text:
            print(text, end="", flush=True)


if __name__ == "__main__":
    asyncio.run(main())

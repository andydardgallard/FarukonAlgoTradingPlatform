#!/usr/bin/env python3
"""
Generate `.code-factory/report_code_changes.md` — a visual "was → became" report
of file changes in a git commit (only changed lines, no context).

Usage:
  python3 scripts/gen_code_changes_report.py [--repo <path>] [--commit <sha|HEAD>] [--out <path>]

Defaults:
  --repo    current working directory
  --commit  HEAD (last commit)
  --out     <repo>/.code-factory/report_code_changes.md

The report is deterministic (parses `git show --unified=0`), costs zero LLM tokens,
and is written next to report.md so the factory run is fully auditable.
"""
from __future__ import annotations

import argparse
import pathlib
import re
import subprocess

MAX_BLOCK = 30  # max lines shown for a pure add/remove block


def git_show_diff(repo: pathlib.Path, commit: str) -> str:
    """Return the diff of a commit with zero context (only changed lines)."""
    proc = subprocess.run(
        ["git", "show", commit, "--unified=0", "--format="],
        cwd=repo, capture_output=True, text=True,
    )
    if proc.returncode != 0:
        raise RuntimeError(f"git show {commit} failed: {proc.stderr.strip()}")
    return proc.stdout


def parse_diff(diff: str) -> list[dict]:
    """Parse a zero-context diff into per-file lists of ('-'|'+', text)."""
    files: list[dict] = []
    cur: dict | None = None
    for raw in diff.splitlines():
        if raw.startswith("diff --git "):
            if cur:
                files.append(cur)
            m = re.match(r"diff --git a/(.*?) b/(.*)$", raw)
            cur = {"path": m.group(2) if m else raw, "lines": []}
        elif raw.startswith("new file"):
            cur["status"] = "new"
        elif raw.startswith("deleted file"):
            cur["status"] = "deleted"
        elif raw.startswith("index ") or raw.startswith("@@") or raw.startswith("diff "):
            continue
        elif raw.startswith("--- ") or raw.startswith("+++ "):
            continue
        else:
            if raw.startswith("-") and not raw.startswith("---"):
                cur["lines"].append(("-", raw[1:]))
            elif raw.startswith("+") and not raw.startswith("+++"):
                cur["lines"].append(("+", raw[1:]))
    if cur:
        files.append(cur)
    return files


def build_report(files: list[dict], commit: str, branch: str) -> str:
    out: list[str] = [
        "# Report of Code Changes — commit `%s`" % commit,
        "",
        "**Branch:** `%s`  ·  **Files:** %d  ·  Generated from `git show %s --unified=0`"
        % (branch, len(files), commit),
        "",
        "Показаны только изменённые строки (без контекста): было → стало.",
        "",
        "---",
        "",
    ]
    for f in files:
        out.append("## `%s`" % f["path"])
        status = f.get("status")
        lines = f["lines"]

        if status == "new":
            out += ["", "**Новый файл** (%d строк добавлено)" % len(lines), "", "```"]
            shown = lines[:MAX_BLOCK]
            out += [l for _, l in shown]
            if len(lines) > MAX_BLOCK:
                out.append("... (+%d строк, полный код в git)" % (len(lines) - MAX_BLOCK))
            out += ["```", "", "---", ""]
            continue

        if status == "deleted":
            out += ["", "**Файл удалён** (%d строк)" % len(lines), "", "---", ""]
            continue

        # group consecutive '-' '+' into pairs
        pairs: list[tuple[str, str]] = []
        removes: list[str] = []
        adds: list[str] = []
        i, n = 0, len(lines)
        while i < n:
            kind, text = lines[i]
            if kind == "-" and i + 1 < n and lines[i + 1][0] == "+":
                pairs.append((text, lines[i + 1][1]))
                i += 2
            elif kind == "-":
                removes.append(text)
                i += 1
            else:
                adds.append(text)
                i += 1

        # Skip trivial pairs (identical text = only newline/EOL change)
        pairs = [(o, nw) for o, nw in pairs if o != nw]

        if pairs:
            out += ["", "### Изменённые строки (было → стало)", "",
                    "| Было | Стало |", "|------|-------|"]
            for old, new in pairs:
                out.append("| `%s` | `%s` |" % (old.strip(), new.strip()))
            out.append("")

        if removes:
            out += ["", "### Удалено", "", "```"]
            shown = removes[:MAX_BLOCK]
            out += shown
            if len(removes) > MAX_BLOCK:
                out.append("... (-%d строк)" % (len(removes) - MAX_BLOCK))
            out += ["```", ""]

        if adds:
            out += ["", "### Добавлено", "", "```"]
            shown = adds[:MAX_BLOCK]
            out += shown
            if len(adds) > MAX_BLOCK:
                out.append("... (+%d строк, полный код в git)" % (len(adds) - MAX_BLOCK))
            out += ["```", ""]

        out += ["---", ""]
    return "\n".join(out)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--repo", default=".", help="Path to the git repository (default: cwd)")
    ap.add_argument("--commit", default="HEAD", help="Commit to report (default: HEAD)")
    ap.add_argument("--out", default=None, help="Output path (default: <repo>/.code-factory/report_code_changes.md)")
    args = ap.parse_args()

    repo = pathlib.Path(args.repo).resolve()
    if not (repo / ".git").exists():
        raise SystemExit(f"Not a git repository: {repo}")

    branch = subprocess.run(
        ["git", "rev-parse", "--abbrev-ref", "HEAD"],
        cwd=repo, capture_output=True, text=True,
    ).stdout.strip() or "(detached)"

    diff = git_show_diff(repo, args.commit)
    files = parse_diff(diff)
    report = build_report(files, args.commit, branch)

    out = pathlib.Path(args.out) if args.out else repo / ".code-factory" / "report_code_changes.md"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(report, encoding="utf-8")
    print(f"Written: {out} ({len(files)} files, {out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()

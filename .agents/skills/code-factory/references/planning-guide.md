# Planning Guide

Goal: turn a business task into a precise, minimal development plan WITHOUT involving the user in
technical details. The user is a business person — communicate in business terms only.

## 1. Understand the task (business language)

Read the task and answer in your own words:

- What business problem is being solved?
- What is the desired end state ("done" looks like what)?
- What are the acceptance criteria in business terms?

If anything is genuinely ambiguous **for the business outcome** (not for the code), ask the user
via `AskUserQuestion`. Example good questions:

- "For which instruments should the strategy now work? Only CNY, or all fractional-price instruments?"
- "What should happen when the channel width is zero — skip the bar, or still allow entry?"
- "Should the new report cover monthly or quarterly periods?"
- "What data files should we validate against?"

Never ask: "Should I change type i64 to f64?" — the factory decides technical details.

## 2. Analyze the project

Delegate heavy exploration to `factory-analyzer` subagents (parallel, isolated contexts). Each
returns a concise summary. Combine into one picture:

- Tech stack + how to test/build/run (see `tech-stack-detection.md`)
- Key modules relevant to the task (entry points, data flow, config)
- Existing tests that cover the affected area
- Regression baseline result (`.code-factory/logs/baseline.md`)

## 2.5 Repo-mismatch gate

After analysis, verify that everything the task references actually exists in the repo
(files, symbols, configs, data paths, entry points). Common mismatch examples:

- Task says "fix strategy X" but there is no strategy X (different name, different file).
- Task references data files (e.g. `CNY-3.23.txt`) that are absent.
- Task references a config path that points nowhere.

In **hitl mode** STOP and ask the user (business language): "the task mentions X, but the repo
has Y — what should I use?" Let the user provide the missing files/context, then re-analyze.
In **auto mode** record an explicit assumption in the plan ("repo is the source of truth; the
task's missing reference X is ignored/interpreted as Y") and continue.

Never silently skip missing inputs — the factory must not guess that the task is wrong.

## 3. Design the solution (minimal intrusion)

- Prefer the smallest change that fully satisfies the task.
- Follow existing architecture and coding style; reuse existing utilities.
- For a brand-new project: choose the stack only if the task implies one; otherwise ask the user
  a business question (e.g., "Should the report be a web page or a console printout?").
- If the task needs new skills/scripts/plugins/tools inside the project (e.g., a test-data
  generator, a plotting script for the business report), plan them as deliverables too.

## 4. DAG plan template

Write `.code-factory/state/plan.md` with the high-level structure below, PLUS a DAG task
list (section "Tasks") that drives implementation. Every task must be independently
verifiable, have a clear file scope, a verification command, and explicit dependencies.

```markdown
# Plan: <task title>

## Goal (business)
<one paragraph, business language>

## Assumptions (if mode=auto)
- <assumption 1>
- <assumption 2>

## Changes
### Modified files
- <path>: <what changes and why>
### New files
- <path>: <what is created and why>

## Test strategy
- Integration: <tests for changed modules>
- Regression: <full suite command; baseline in logs/baseline.md>
- Business: <scenario, configs, expected results — filled below>

## Business tests
- Scenario: <user story, e.g., "Run strategy on CNY-3.23.txt">
- Config / inputs: <exact files/commands the user specified>
- Expected business results: <e.g., ">=5 LONG/SHORT signals, positive equity, Si unchanged">

## Tasks (DAG)
- task_01: <logical change>; files: [...]; deps: []; verification: <command>
- task_02: <logical change>; files: [...]; deps: [task_01]; verification: <command>
- biztest_<name>: <user story scenario>; files: []; deps: [task_0X]; verification: <run + assert>
- final_integration: <full suite must pass>; deps: [all tasks]; verification: <full build+test>

## Risks
- <anything that could fail and its mitigation>
```

## 4.1 Decomposition rules (logical, not formal)

- **Small tasks** (bug fixes): aim for 3–5 logically complete tasks.
- **Large projects** (new features, refactoring, green-field): decompose as needed, using one of:
  - Layer-based (foundation → core logic → integration → testing → docs);
  - Feature-based (vertical slices, feature by feature);
  - Dependency-based (independent foundation → dependent core → integration → final validation).
- Each task must be independently verifiable (single verification command preferred — clearer
  error attribution).
- **Parallelism**: mark independent tasks (different files, no dependencies) as parallel;
  dependent tasks must be sequential with explicit `deps`.
- **Business tests are tasks**: for EVERY user story in "Business tests", create a dedicated
  `biztest_<name>` task that runs the exact command from the story and asserts the success
  criteria against the expected output.
- **Final integration task**: EVERY plan ends with `final_integration` that runs AFTER all
  other tasks, depends on all of them, and verifies the whole project still works
  (full build + full test suite). It catches regressions.
- Derive verification commands from the stack config files (Cargo.toml, pyproject.toml,
  package.json, go.mod...) — do NOT guess package names from folder names.

## 5. Business-test definition (HITL mode — mandatory)

In HITL mode you MUST ask the user (via `AskUserQuestion`, max 4 questions per call) before
presenting the final plan:

1. **User story / scenario** — "What exact scenario should work after the fix? Describe it in one
   or two sentences as a user would."
2. **Configs & input data** — "Which configuration files or input data should we run the program
   with? Give file names or paths if you know them."
3. **Expected business results** — "What concrete result proves the task is done? For example:
   'at least 5 trading signals and no losses' or 'the report shows the correct monthly total'."
4. (optional) **Priority/edge cases** — "Are there any specific edge cases that must keep working
   (e.g., zero values, empty data, very small numbers)?"

Keep answers verbatim in the plan so the tester can check them later.

In **auto mode** skip the questions; derive the scenario, configs and expected results from the
task description, and write them into the plan marked as **assumptions**.

## 6. Present the plan

HITL: write the plan file, call `EnterPlanMode`, then `ExitPlanMode`. The user may approve,
reject or revise. On revision, update the plan and re-present.

Auto: briefly present the plan in the main thread (assumptions highlighted) and continue
immediately.

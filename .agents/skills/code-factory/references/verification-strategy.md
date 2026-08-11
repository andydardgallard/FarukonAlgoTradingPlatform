# Verification Strategy & Rollback

Goal: prove the task works on three levels (integration, regression, business) for ANY project
type (frontend, backend, CLI, library, green-field) and, on ANY failure, safely return the
project to its pre-change state before retrying.

## 0. Order of execution (fixed)

1. **Artifacts first** — before any source change, `.code-factory/` must contain:
   `state/task.yaml`, `state/plan.md`, `logs/baseline.md`, `backups/`, `manifest.json`.
   (Factory state lives in files, not in the conversation.)
2. Baseline (before changes) → `logs/baseline.md`
3. Integration tests (changed modules)
4. Regression tests (full existing suite)
5. Business tests (run the real program, compare business results)
6. Acceptance check (verify acceptance_criteria)

If a stage fails, roll back and retry implementation with the error context. Do not proceed to a
later stage while an earlier one is failing.

## 1. Integration tests

- Focused tests that verify the changed modules work together correctly (new function + its
  callers, new module + platform API, etc.).
- Use the project's existing test framework. Follow the style of existing tests.
- Include edge cases relevant to the task (e.g., fractional prices, zero/empty input, boundary
  values).
- Do NOT delete or weaken existing tests.

## 2. Regression tests

- Run the full existing test suite with the canonical command from `tech-stack-detection.md`.
- Compare against the baseline: any previously-passing test that now fails is a regression and
  triggers rollback.

## 3. Business tests

Business test = run the actual (built/started) program with the user-specified configs and input
data, and check that the **business results** match what the user expected.

1. Build/start the program exactly as it will be used (see plan: "Config / inputs").
2. Collect the actual business results (program output, generated files, logs, metrics — whatever
   the domain produces).
3. Compare with the expected business results from the plan.
4. Record both actual and expected in `logs/business-tests.md` with PASS/FAIL per item.
5. If they differ — show the user the actual vs expected (HITL) or auto-decide (auto): the
   difference may mean (a) the code is wrong → fix; or (b) the expectation was described
   inaccurately → revise the expectation with the user and re-run.

### Practical notes (learned on a real run)

- **Output location**: configs often have an `exit_results_path`; the factory should read it
  from the config and verify the generated report files exist and have expected row counts.
- **Distinct-outcomes check**: if the user says "the wrong result was only N variants", verify
  the new run produces MORE than N distinct result rows (e.g., count unique metric tuples in the
  output CSV). This is a strong, objective business-test check.
- **Distinguish "filter works" from "broken"**: a scenario may legitimately produce 0 deals
  (e.g., a CNY-tuned threshold on an expensive Si instrument). Prove the filter is working by
  re-running with a price-proportional threshold and showing deals appear — not by forcing deals.
- **Build then run**: for compiled stacks, business tests must use the exact commands the user
  gave (e.g., `cargo build -p <lib> --release` then `cargo run -- -c <config>`). Record which
  artifacts the run consumes (e.g., which `.so`/binary) and verify they exist.
- **Sandbox side effects**: a run may create output dirs (e.g., `opt_results/`). Treat them as
  temporary; remove them after verifying, or add them to `.gitignore`.


## 4. Rollback mechanism (critical)

Trigger: ANY test failure (integration, regression, business) or acceptance-criteria miss.

### 4.1 Prepare before changes (done in Phase 4 of the flow)

```bash
# Record git state
git rev-parse HEAD > .code-factory/state/git-head.txt
git status --porcelain > .code-factory/state/git-status-before.txt

# Backup every file that will be modified (preserve relative paths)
mkdir -p .code-factory/backups
# example: cp --parents src/lib.rs .code-factory/backups/
# (copy each changed file into .code-factory/backups/ keeping its path)
```

### 4.2 Manifest

Maintain `.code-factory/manifest.json`:

```json
{
  "changed_files": ["src/lib.rs", "configs/trading.toml"],
  "created_files": ["tests/integration_fractional.rs", "scripts/gen_report.py"],
  "backup_root": ".code-factory/backups"
}
```

Update it after every file operation by the factory.

### 4.3 Rollback steps (on failure)

```bash
# 1. Remove files created by the factory
while read -r f; do rm -f "$f"; done < <(python3 -c "import json;print('\n'.join(json.load(open('.code-factory/manifest.json'))['created_files']))")

# 2. Restore modified files from backups
#    for each changed file: cp .code-factory/backups/<path> <path>

# 3. If the project uses git and backups are unreliable, restore from git
#    (only for tracked files, and only if HEAD was not moved):
#    git checkout -- <changed files>   (created/untracked files must be removed manually)

# 4. Verify: git status should now match .code-factory/state/git-status-before.txt
```

After rollback the project MUST be byte-identical to the pre-change state.

### 4.4 Iteration

- Append the failure context to `.code-factory/logs/errors.md` (what test failed, the error
  output, what was rolled back).
- Pass the error context back to the coder subagent: "the previous attempt failed on X, rollback
  was done, fix accordingly."
- Limit retries per root cause by the retry budgets in `error-routing.md` (coder=1, ba=2,
  planner=2). On exhaustion, escalate to the Diagnostician instead of looping.

## 5. Reporting

Write `.code-factory/logs/test-results.md`:

| Stage | Command/Scenario | Result | Evidence |
|-------|------------------|--------|----------|
| Baseline | `cargo test` | PASS (42) | logs/baseline.md |
| Integration | ... | PASS | ... |
| Regression | `cargo test` | PASS (42) | ... |
| Business | run on CNY-3.23.txt | PASS | actual==expected |

Keep the report short — evidence by file reference, not by pasting full logs.

## 6. Final report (.code-factory/report.md)

On finish (success OR FAILED) write `.code-factory/report.md` — ONE self-contained file that
summarizes the whole run. It is the hand-off artifact to the factory developer; it must allow
analyzing the run without reading the rest of `.code-factory/`.

```markdown
# Factory Report: <task title>

## Result
SUCCESS | FAILED

## Task (business)
<parsed task: title, description, priority, acceptance criteria>

## What changed
- <file>: <what and why>  (from manifest + plan)

## Test results
| Stage | Command/Scenario | Result | Evidence |

## Business results
<actual vs expected per user story>

## Errors & diagnosis
- errors.md summary (each failure: stage, root cause, resolution)
- diagnostic.md summary (if Diagnostician ran: root_cause, recommended_role, action)

## Acceptance check
|criterion|status|evidence|
```


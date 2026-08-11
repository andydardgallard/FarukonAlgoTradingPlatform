# Error Routing

Goal: on ANY test/build failure, route the fix to the right role deterministically (~90% of
cases, zero LLM tokens), and only escalate to the Diagnostician (1 LLM call) for the rest.
The factory NEVER crashes silently — unknown errors always fall through to the Diagnostician,
then Human, then FAILED with a full log.

## 1. Classification (deterministic, regex-based)

Match the combined error output (stdout + stderr) against these patterns IN ORDER — more
specific first. First match wins.

| # | Category | Match patterns (regex, case-insensitive) | Route to |
|---|----------|-------------------------------------------|----------|
| 1 | MISSING_INPUT | `Task file not found\|Work dir not found` | HUMAN |
| 2 | WRONG_RESULTS | `zero Total_Return\|no trades occurred\|all combinations produced identical\|expected .* but got\|regression\|no meaningful signals\|no nonzero\|zero return` | DIAGNOSTICIAN |
| 3 | INFRASTRUCTURE | `Merge conflict in (target/|node_modules/|__pycache__/|build/|dist/)` | INFRASTRUCTURE (auto-fix) |
| 4 | INFRASTRUCTURE | `(build artifacts|target/|node_modules/) (tracked|committed) in git` | INFRASTRUCTURE (auto-fix) |
| 5 | MISSING_FILE | `No such file or directory\|cannot open file\|file not found\|does not exist` | BA |
| 6 | MISSING_FILE | `ModuleNotFoundError\|ImportError.*No module named` | BA |
| 7 | BAD_COMMAND | `command not found\|: not found\|No such command\|unknown option\|syntax error near unexpected token` | PLANNER |
| 8 | COMPILE_ERROR | `error[E\d+]\|^error: \|cannot find\|failed to compile\|no member named` | CODER |
| 9 | COMPILE_ERROR | `SyntaxError\|IndentationError\|NameError.*not defined` | CODER |
| 10 | COMPILE_ERROR | `npm ERR!\|error TS\d+\|Type error:\|cannot find module` | CODER |
| 11 | COMPILE_ERROR | `go: cannot find\|cannot find package\|\.go:\d+:\d+: .* undefined` | CODER |
| 12 | COMPILE_ERROR | `cannot find symbol\|\.java:\d+: error:` | CODER |
| 13 | LINK_ERROR | `linking\|undefined reference\|cannot find -l\|ld returned\|library not found` | CODER |
| 14 | DEPENDENCY | `cargo.*could not compile.*dependency\|npm ERR! 404\|pip.*No matching distribution` | INFRASTRUCTURE |
| 15 | PERMISSION | `Permission denied\|EACCES\|operation not permitted\|sudo required` | HUMAN |
| 16 | TIMEOUT | `timed?\s*out\|timeout\|TimeoutExpired\|operation timed out` (with "test" in output) | PLANNER |
| 17 | TIMEOUT | `timed?\s*out\|timeout\|TimeoutExpired\|operation timed out` | CODER |
| 18 | RUNTIME_CRASH | `thread .* panicked\|Segmentation fault\|core dumped\|Fatal error\|Traceback (most recent call last)\|panic:` | CODER |
| 19 | ASSERTION | `AssertionError\|assert .* failed\|test.*failed\|FAILED.*test` | CODER |
| 20 | UNKNOWN | (no pattern matched) | DIAGNOSTICIAN |

Notes:
- Patterns are language-agnostic; add more regexes over time as new error families appear.
- The "wrong business results" pattern (row 2) MUST be checked before generic assertion/compile
  patterns, because business markers may appear inside an assertion message.

## 2. Routing decision

After classification, decide who retries:

| Category | Route to | What happens |
|----------|----------|--------------|
| WRONG_RESULTS (without "regression") | DIAGNOSTICIAN | deep analysis needed |
| WRONG_RESULTS (with "regression") | CODER | code changed behavior incorrectly |
| INFRASTRUCTURE | auto-fix | run the auto-fix command, then re-run verification |
| COMPILE_ERROR / LINK_ERROR | CODER | coder fixes code with the error output |
| MISSING_FILE | BA | BA re-studied configs incorrectly |
| BAD_COMMAND | PLANNER | planner rewrites the verification command |
| TIMEOUT (test) | PLANNER | lighter verification command |
| TIMEOUT (other) | CODER | likely infinite loop / perf bug |
| RUNTIME_CRASH | CODER | fix the crash |
| ASSERTION | CODER | fix code to pass tests |
| PERMISSION | HUMAN | manual fix required |
| UNKNOWN | DIAGNOSTICIAN | fallback |

## 3. Retry budgets (deterministic policy)

| Role | Max retries before escalation |
|------|-------------------------------|
| CODER | 1 |
| BA | 2 |
| PLANNER | 2 |
| DIAGNOSTICIAN | 1 |
| INFRASTRUCTURE | 3 |
| HUMAN | 0 (cannot auto-retry) |

- Each retry increments the per-role counter in `.code-factory/state/pipeline.yaml`.
- When a role's budget is exhausted → escalate to DIAGNOSTICIAN (append the attempt history).
- DIAGNOSTICIAN, after its analysis, RESETS the recommended role's counter to 0 and re-runs it
  with the diagnostic context (do NOT repeat the failed approach).

## 4. Diagnostician (LLM fallback)

When routing says DIAGNOSTICIAN (unknown errors, wrong business results, exhausted budgets):

1. Launch the `factory-diagnostician` subagent with:
   - the full error output (last ~3000 chars),
   - the failed role and attempt history,
   - the diagnostic pre-processing text (below).
2. The subagent returns a report: `root_cause`, `recommended_role` (coder | planner | ba |
   infrastructure | human), `recommended_action`, `context_for_retry`, `confidence`.
3. Main agent writes it to `.code-factory/logs/diagnostic.md`.
4. Route by `recommended_role`: coder → implement again; planner → re-plan; ba → re-analyze;
   human → ask the user; infrastructure → auto-fix.

### Diagnostic pre-processing (cheap, deterministic)
Before calling the LLM, run these cheap checks and include their results in the prompt:
- Was the command found? (BAD_COMMAND) — check `command not found`.
- Do referenced files exist? (MISSING_FILE) — check paths in the error.
- Did compilation succeed? (COMPILE_ERROR) — check for `error[E...]` / `cannot find`.
- Is the `.so`/binary present? (for compiled stacks) — `ls target/release/*.so`.
- Is it a wrong build order? (e.g. a later build overwrites the `.so` built with a feature flag).

This is the same trick the Python factory used; it makes the Diagnostician ~10x more reliable.

## 5. Escalation ladder (never silent crash)

1. Deterministic regex → ~90% of errors, 0 tokens.
2. Diagnostician (LLM) → remaining ~10%, 1 LLM call.
3. HUMAN (HITL) → if Diagnostician recommends human, ask the user.
4. FAILED → only when ALL budgets are exhausted; produce a full report in
   `.code-factory/logs/errors.md` with the transition history.

---
name: code-factory
description: Autonomous code factory that accepts business tasks in plain language from non-technical users, analyzes the project, plans changes, asks only business-logic questions, obtains plan approval, then implements code and runs integration / regression / business tests with deterministic error routing and LLM diagnosis on failure, checkpoint/resume, and rollback, finally validating acceptance criteria. Works with any programming language or combination of languages. Use when the user says "run the code factory", "solve this business task", "implement this feature", "fix this bug", "build this project", or provides a business task file (task.yaml) / description.
type: flow
---

# Code Factory

Turns business tasks (described by non-technical users) into working, tested code.

## Reference files — read the relevant one when you reach its phase

- `references/tech-stack-detection.md` — deterministic stack detection + Scout pipeline (Phase 1)
- `references/planning-guide.md` — analysis checklist, DAG plan template, business-test questions (Phases 2–3)
- `references/verification-strategy.md` — integration / regression / business tests and rollback (Phases 4–9)
- `references/error-routing.md` — deterministic error classification, retry budgets, Diagnostician fallback (on any failure)
- `agents/models.yaml` — per-role model configuration (change providers/families anytime)
- `assets/task-template.yaml` — business task template (Phase 0)

## Runtime state (inside the project)

All runtime artifacts live in `.code-factory/`:

- `state/` — parsed task, plan, pipeline.yaml (checkpoint/resume), acceptance
- `backups/` — backups of files before modification
- `manifest.json` — list of changed / created files (used for rollback)
- `logs/` — baseline, errors.md, diagnostic.md, test-results.md

The factory may create any files, skills, scripts or plugins inside the project that are needed to solve the task. The factory must keep the total token spend minimal: use subagents for heavy context (analysis, coding, testing, diagnosis), return only concise structured results, and avoid loading large files into the main context.

```mermaid
flowchart TD
    A([BEGIN]) --> B[Accept the task: read the user's message or task.yaml. Extract: title, repo_path, description, priority, mode (hitl/auto), acceptance_criteria, commit_exclude, models. Save the parsed task to .code-factory/state/task.yaml. If no task file exists, treat the user's message as the task. Checkpoint: if .code-factory/state/pipeline.yaml exists and the task is unchanged, resume from the recorded phase.]
    B --> C{Is the business task clear enough?}
    C -->|No| D[Ask the user business-level clarifying questions via AskUserQuestion. Ask ONLY business logic and expectations, never coding questions. Then update the parsed task.]
    D --> B
    C -->|Yes| E[Analyze the project: launch factory-analyzer subagents in parallel to determine tech stack, structure, entry points, test setup. Detect the stack deterministically using references/tech-stack-detection.md. Run the existing test suite to establish a regression baseline and save it to .code-factory/logs/baseline.md.]
    E --> SC[Scout pipeline: refine the project model with the analyzer report; generate AGENTS.md with the 8 standard sections; run native /init kimi so Kimi adapts AGENTS.md to itself. Skip if AGENTS.md already exists and the project is unchanged.]
    SC --> C2{Does the task match the analyzed repo? Check that files, symbols, configs and data referenced by the task actually exist.}
    C2 -->|No| R1[Ask the user for the missing files or context via AskUserQuestion in hitl mode, or in auto mode record an assumption that the repo is the source of truth and continue. Then re-analyze.]
    R1 --> E
    C2 -->|Yes| F[Create the development plan following references/planning-guide.md: DAG of tasks with dependencies and per-task verification commands; business tests are first-class tasks in the plan. If the task needs new in-project skills, scripts or plugins, include them in the plan. Save the plan to .code-factory/state/plan.md.]
    F --> G{Task mode?}
    G -->|hitl| H[Business test definition: ask the user via AskUserQuestion for 1 a concrete business scenario (user story), 2 which configs and input data to run, 3 expected business results. Only business-logic questions. Store answers in the plan.]
    H --> I[Present the full plan for approval: write it to the plan file and call EnterPlanMode then ExitPlanMode. Wait for approval or revision comments.]
    I --> J{Plan approved?}
    J -->|Revise| F
    J -->|Approve| K
    G -->|auto| L[Make reasonable business assumptions from the task description. Record every assumption explicitly in the plan.]
    L --> PF[Pre-flight git check: if the project has no git repository, run git init. Working tree must be clean: auto-untrack build artifacts (target/, node_modules/, __pycache__/ etc.) and commit factory artifacts (AGENTS.md). Record git HEAD and git status in .code-factory/state/.]
    PF --> K[Backup the current state: copy every file that will be modified to .code-factory/backups/ preserving relative paths. Track created files in .code-factory/manifest.json. Write the checkpoint .code-factory/state/pipeline.yaml after every phase for resume.]
    K --> M[Implement: launch factory-coder subagents for the plan tasks, respecting dependencies; independent tasks can run in parallel. MANDATORY: pass model= to every subagent from models.yaml by role, and log the used model for each role to pipeline.yaml after each run. Each coder follows the plan and the project coding style in an isolated context. After every change, update manifest.json. Also create any new skills/scripts/plugins defined in the plan.]
    M --> N[Integration tests: write and run per-task tests for the changed modules following references/verification-strategy.md.]
    N --> O{Tests passed?}
    O -->|No| RR[Route the failure per references/error-routing.md: classify deterministically by regex, record the error context in .code-factory/logs/errors.md, and update retry counters in pipeline.yaml.]
    O -->|Yes| Q[Regression tests: run the full existing test suite of the project.]
    RR --> RR2{Classification result}
    RR2 -->|coder| P[Rollback: restore backups, remove files created by the factory, restore git state per references/verification-strategy.md. Return the error context to the coder.]
    P --> M
    RR2 -->|ba| P2[Rollback + update the plan with the error context.]
    P2 --> F
    RR2 -->|planner| P2
    RR2 -->|infrastructure| AI[Run the deterministic auto-fix command from error-routing.md, then re-run the failed verification.]
    AI --> M
    RR2 -->|diagnostician| DG[Run the factory-diagnostician subagent: deep LLM analysis of the error output and attempt history. Write the report to .code-factory/logs/diagnostic.md.]
    DG --> DR{Diagnostician recommendation}
    DR -->|coder| P
    DR -->|ba| P2
    DR -->|planner| P2
    DR -->|infrastructure| AI
    DR -->|human| U2[Show the user the diagnosis and ask how to proceed.]
    U2 --> V2{User decision}
    V2 -->|fix code| M
    V2 -->|revise plan| F
    V2 -->|stop| Y
    Q --> R{Tests passed?}
    R -->|No| RR
    R -->|Yes| S[Business tests: run the program with the user-provided configs and input data. Collect actual business results and compare them with the expected business results.]
    S --> T{Business results match?}
    T -->|No| U[Show the user actual vs expected results. Ask via AskUserQuestion: fix the code, or revise the expectations?]
    U --> V{User decision}
    V -->|Fix the code| M
    V -->|Revise the expectations| S
    T -->|Yes| W[Acceptance check: verify every acceptance_criteria item against the actual results and document evidence for each. Save the verification to .code-factory/state/acceptance.md.]
    W --> X{All criteria met?}
    X -->|No| RR
    X -->|Yes| Y[Finish: remove backups, produce the final report (what changed, test results, business results, acceptance evidence, models_used per role). Commit changes to a feature branch respecting commit_exclude. Update project documentation if the task requires it. Write the self-contained .code-factory/report.md with the full history (task, plan, errors, diagnosis, results, manifest, models_used). Generate .code-factory/report_code_changes.md next to it with the scripts/gen_code_changes_report.py script (was-became per changed line). Present the report to the user.]
    Y --> Z([END])
```

Rules that always apply:

- **Language-agnostic**: detect the stack; never assume a language. The factory serves any
  project type (frontend, backend, CLI, library, green-field) — verification is framework-agnostic.
- **Token efficiency**: parallel subagents, isolated contexts, concise results, progressive
  reference loading, and deterministic error routing (zero-LLM) before LLM diagnosis.
- **Rollback safety**: every test failure triggers error routing → rollback before retrying.
- **Business-first**: communicate with the user only in business terms.
- **Artifacts first**: before touching any source file, materialize into `.code-factory/` the
  parsed task, the plan, the baseline, the backup of changed files and the manifest. Do not keep
  factory state only in the conversation.
- **Repo-mismatch gate**: if the task references files, symbols, configs or data that are absent
  in the repo, stop in hitl mode and ask the user for them (or record an assumption in auto
  mode). Never silently skip missing inputs.
- **Escalation ladder**: deterministic regex → Diagnostician (LLM) → Human (HITL) → FAILED with
  a full log. The factory never crashes silently.
- **Retry budgets**: coder=1, ba=2, planner=2, diagnostician=1, infrastructure=3. When a role's
  budget is exhausted, escalate to the Diagnostician, which resets the counter of its
  recommended role and re-runs it with the diagnostic context.
- **Checkpoint/resume**: after every phase write `.code-factory/state/pipeline.yaml` (current
  phase, retry counters, plan fingerprint). On restart, resume from the recorded phase.
- **Report**: on finish (success OR FAILED) write `.code-factory/report.md` — one
  self-contained file with the full history (task, plan, errors, diagnosis, results) so it can
  be handed to the factory developer for analysis without reading the whole `.code-factory/`.
  Also write `.code-factory/report_code_changes.md` (next to it) via
  `scripts/gen_code_changes_report.py` — a deterministic was-became diff report of the commit
  (zero LLM tokens).
- **Models**: consult `.agents/agents/models.yaml` and the task `models:` override. MANDATORY:
  pass the per-role `model=` to every subagent launch via the Agent tool; log the actual model
  to `.code-factory/state/pipeline.yaml` (`models_used`) and include it in `report.md` so the
  run is auditable. The main agent's own model (used for planning) is the session model
  (set via `kimi -m` / `/model`); record it too.
- **Commit policy**: respect `commit_exclude` from the task — never commit matching files
  (e.g. personal strategy code); stage everything EXCEPT the excluded patterns.
- **Git-native**: if the project has no git repository, run `git init`. All changes flow through
  git (feature branch per task), with rollback to the base commit on failure.

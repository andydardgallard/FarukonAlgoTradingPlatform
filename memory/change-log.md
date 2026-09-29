<!-- code-factory-memory: change-log -->

## 2026-09-10 — LSHADE доработка, кэш и обработчики результатов

title: LSHADE доработка, кэш и обработчики результатов
timestamp: 2026-09-10
branch: code-factory/lshade-handlers-cache
commit: v2.1.0
task_type: implement
goal: Доработать LSHADE (вывод лучшей хромосомы, fitness_direction, append статистики, кэш особей), добавить Max_Drawdown_DateTime и сравнение CSV.
changed_files: farukon_core/src/optimization.rs, Farukon_2/src/optimizers.rs, farukon_core/src/performance.rs, Farukon_2/src/portfolio.rs, farukon_core/src/utils.rs, python/optresults_handler.py, Cargo.toml, README.md, USER_MANUAL.md
created_files: AGENTS.md, memory/change-log.md, memory/summary.md
results: integration=pass regression=pass(34 tests) business=pass e2e=pass review=approve
decisions: fitness_direction применяется внутри run (evaluate возвращает сырой фитнес); начальная популяция = Итерация 1; кэш в памяти с очисткой через BankClearGuard; версия 2.1.0 (minor).
assumptions: стратегия собрана под Linux (target/release/libstrategy_lib.so); полный прогон LSHADE на реальных данных выполнен (portfolios/lshade_si_25_apr_dd.json, exit 0).
models_used: analyzer=deepseek-v4-pro; coder=deepseek-v4-flash; reviewer=deepseek-v4-pro; documenter=deepseek-v4-flash

factory_version: 12.5.1

unfinished: нет незавершённых элементов

## 2026-09-29 — Thread-scaling review: root cause of the >8-thread degradation on 1-3m timeframes

title: Thread-scaling review: root cause of the >8-thread degradation on 1-3m timeframes | project: code | timestamp: 2026-09-29T00:00:00+0300
run_id: 20260929-b81785fd
branch: code-factory/thread-scaling-review
task_type: review
goal: Explain why backtest runtime stops scaling beyond ~8 threads on the 1-3 minute timeframes (4-5m scale to 64), why only the first "wave" of a batch is fast at high thread counts, find testing errors in the time_tests campaign, and produce a fix task plus an optimal backtest strategy.
changed_files: AGENTS.md, memory/change-log.md, memory/summary.md
created_files: (factory run artifacts, factory root) .code-factory/report.md, .code-factory/fix_task.yaml, .code-factory/logs/{measurements-analysis,test-results-review,confirmation,hardware,baseline,code-review}.md
results: Review complete, acceptance verdict DEGRADED (5 verify criteria MET, 0 failed; regression not-run — review changed no code; baseline 31 pass / 3 pre-existing settings::tests failures) [verified: .code-factory/state/acceptance.md, logs/baseline.md]. Root cause identified [verified: .code-factory/report.md §5, logs/code-review.md reviewer CONFIRMED items]: per-candidate O(bars) memory footprint (unbounded equity_series + 3-4 copies at portfolio.rs:544-545,713-715, performance.rs:310; per-bar HashMap clones portfolio.rs:438,501,518,535; per-bar dlsym + CString rebuild strategy_loader.rs:137,145-151; no mimalloc) saturates bandwidth/allocator beyond ~8 threads on 1-3m data [inferred mechanism from 110-run dataset + code evidence]. First-wave-fast effect = in-process heap fragmentation + page-fault debt, wave 1 vs later median 5.3-6.1x on 1m, starting at candidate #threads+1 [verified: logs/confirmation.md]. Review verdict request_changes, 0 critical / 10 major / 11 minor / 2 nit — carried into fix_task.yaml [verified: logs/code-review.md].
decisions: deep-clone hypothesis REFUTED as the cliff driver (arc mode stalls too, reviewer-verified) [verified: logs/code-review.md evidence point 4]; optimal strategy: ≤8 threads on 1-3m (7 deep / 8 arc on 1m), up to 64 threads on 4-5m, prefer arc mode, scale processes not threads, chunk 1m runs [verified: report.md §6 derived from logs/measurements-analysis.md]; shard-1 (vendored factory .agents/) excluded from review as factory machinery [inferred]; .code-factory kept in the factory root because the task's repo_path points outside the deployment root [inferred]; reviewer ran on deepseek-flash although the user-added models matrix names kimi-k3 (matrix arrived after the reviewer launched) — deviation recorded [verified: report.md §11].
assumptions: no new multi-hour benchmark runs (110-run dataset is the evidence base); "batch" = one optimizer process over 93 combinations.
models_used: analyzer=explore(kimi-k3); measurements+coder agents=kimi-k3; reviewer=deepseek-flash (deviation, see decisions); main=primary(kimi-k3)

factory_version: 12.12.1

unfinished:
  - item: Implement the fixes (fix_task.yaml items 1-12: hot-path allocations, equity_series copies, mimalloc, deep-mode warning, adaptive thread guidance, clippy/unsafe/commission.unwrap, 3 Windows settings tests, 2min arc config paths)
    reason: review task delivers analysis only; code changes are the follow-up implement run
    severity: critical
    follow_up: true
  - item: Re-run the 1m arc 16-thread benchmark after the fix to validate the improvement vs the 44185 s baseline
    reason: validation of the fix; requires the fix first (~2.5 h run)
    severity: warning
    follow_up: true

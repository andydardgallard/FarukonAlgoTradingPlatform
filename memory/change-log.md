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

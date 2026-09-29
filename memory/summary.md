<!-- code-factory-memory: summary -->
project: code
repo_path: ../FarukonAlgoTradingPlatform/code

# Farukon — сводка долгосрочной памяти

## Текущее состояние
Project Farukon (Rust workspace: `farukon_core` lib, `Farukon_2` bin, `strategy_lib` cdylib; Python tools in `python/`). Version `2.1.0` (review tasks do not bump the version).

Latest run (2026-09-29, run_id 20260929-b81785fd, task_type review, DEGRADED accepted): thread-scaling investigation over 110 measured runs — root cause of the >8-thread degradation on 1-3m timeframes found (per-candidate O(bars) memory footprint + per-bar allocations + system allocator; first-wave effect = in-process heap fragmentation/page-fault debt). Deliverables: factory run artifacts (report, fix_task.yaml with 12 fix items + 10 major review findings, measurement/testing-error analyses). Test suite: 34 tests, 31 pass, 3 pre-existing Windows settings::tests failures. Open backlog: implement fix_task.yaml, then re-validate the 1m arc 16-thread benchmark.

Previous run (2026-09-10):

## Ключевые решения (последний прогон, 2026-09-10)
- LSHADE: лучшая особь определяется по `fitness_direction` («max»/«min»); `evaluate` возвращает сырой фитнес, направление применяется внутри `run`.
- Нумерация итераций LSHADE 1-based (начальная популяция = «Итерация 1»).
- Статистика `lshade_optimization_results.csv` пишется append'ом после каждой итерации (заголовок один раз в начале запуска).
- Кэш особей LSHADE — in-memory `HashMap` (ключ включает `pos_sizer_name`), очищается + `shrink_to_fit` через `BankClearGuard`.
- `Max_Drawdown_DateTime` (относительная просадка) добавлено в `optimization_results.csv` и портфельный CSV.
- `python/optresults_handler.py` получил режим сравнения двух CSV (`-fc/--file_compare`, `-y set_cmp`).

## Верификация
- `cargo test --workspace` → 34 passed / 0 failed.
- Полный end-to-end прогон LSHADE на реальных данных выполнен: `./target/release/Farukon_2 --config portfolios/lshade_si_25_apr_dd.json` → exit 0 (стратегия собрана под Linux: `target/release/libstrategy_lib.so`).

## Последняя история
- 2026-09-10: LSHADE доработка + кэш + обработчики — success, 34 теста, e2e-прогон, review approve, v2.1.0.

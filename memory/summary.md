<!-- code-factory-memory: summary -->
project: code
project_id: 696da31598ba
repo_path: ../FarukonAlgoTradingPlatform/code

# Farukon — сводка долгосрочной памяти

## Текущее состояние
Project Farukon (Rust workspace: `farukon_core` lib, `Farukon_2` bin, `strategy_lib` cdylib; Python tools in `python/` incl. stdlib-runnable `python/tests/`). Version `3.1.0` (minor; single source of truth `VERSION`, mirrored in workspace Cargo.toml).

Latest run (2026-10-26, run_id 20261026-aefd4f09, task_type implement): all 29 pre-publication findings fixed + owner tasks A-D (market-data volume u64->f64; correlation chart; LSHADE Best_Individual CSV column; SYMI-on-RTS business tests). Reviewer approve at iteration 4 (3 criticals found and fixed in rework: final-bar fill booking, double-count, multi-fill accumulation). Regression 75 tests green, clippy/fmt clean, acceptance SUCCESS 8/8 verify MET. Commit 1324753f on branch code-factory/fix-prepublication-20261026. Open backlog: 0 — the project is ready for the GitHub push.

Note: market data now lives in `Tickers_fbs/soa/RTS/` (f64 volumes, converted this run; old CNY/Si deleted by the owner and committed as deletions). Data files are gitignored — a fresh clone needs the data copied in. The local start.sh on disk is the factory launcher; the tracked start.sh is the Farukon build/run script.

BREAKING v3.0.0: strategy ABI — `create_strategy` takes an emission callback pair (`emitter_ctx`, `emit_signal_cb`); strategies store `SignalEmitter { ctx, cb }`; only POD crosses the FFI boundary. mimalloc is the process allocator. **Every strategy dll must be rebuilt against the current farukon_core — old dlls crash the new binary** (in v3.1.0 the DataHandlerVTable volume type changed u64->f64 — rebuild again). Reference: `strategy_lib/src/lib.rs`; contract: `farukon_core/src/event.rs`.

## Ключевые решения (последний прогон, 2026-09-29)
- mimalloc сохранён по решению пользователя; падение E1 (mimalloc exe + старые dll) исправлено переделкой границы сигналов (allocator-safe callback), а НЕ пересборкой dll с mimalloc (опровергнуто эмпирически).
- Снапшоты позиций/холдингов больше не клонируют map'ы на каждом баре (пустые map'ы; читателей не существует — grep-verified); deals_count считается из живого состояния в конце.
- `calculate_final_performance` выполняется для любого metrics mode (был только Offline) — отсюда Max_Drawdown_DateTime для Grid/GA/realtime.
- Windows/MSVC: `~/.cargo/config.toml` [env] CXXFLAGS/CL=-utf-8 — иначе libmimalloc-sys не собирается с кириллическим путём пользователя.
- LSHADE: лучшая особь по `fitness_direction`; нумерация итераций 1-based; банк особей очищается BankClearGuard (из прогона 2026-09-10, актуально).

## Верификация
- `cargo test --workspace` → 75 passed / 0 failed; `cargo clippy --workspace` → 0/0; `cargo fmt --check` → exit 0.
- Бенчмарки 1m arc 16thr и 2m arc 32thr: checker `.code-factory/scripts_run/bench_check.py {1m|2m}` exit 0 (ускорение + паритет метрик).
- Проба E1 (1 кандидат 1m, mimalloc + новый ABI): EXIT=0, полный прогон 1169.9 s.

## Последняя история
- 2026-10-26: Fix run (run 20261026-aefd4f09) — 29 findings + owner A-D, approve iter 4, v3.1.0, backlog 1 -> 0.
- 2026-10-26: Pre-publication whole-repo review (run 20261026-b300cde5) — request_changes, 29 findings (17 critical), fix-tasks.yaml generated, backlog 0 → 1.
- 2026-09-29: Thread-scaling fix (run 20260929-f24b5e53) — success, v3.0.0 (major, ABI break), 6.13x/12.12x на бенчмарках, backlog 2 → 0.
- 2026-09-29: Thread-scaling review (run 20260929-b81785fd) — root cause найден, fix_task.yaml создан, DEGRADED accepted (review не менял код).
- 2026-09-10: LSHADE доработка + кэш + обработчики — success, 34 теста, e2e-прогон, review approve, v2.1.0.

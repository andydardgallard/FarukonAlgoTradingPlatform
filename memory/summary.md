<!-- code-factory-memory: summary -->
project: code
repo_path: ../FarukonAlgoTradingPlatform/code

# Farukon — сводка долгосрочной памяти

## Текущее состояние
Project Farukon (Rust workspace: `farukon_core` lib, `Farukon_2` bin, `strategy_lib` cdylib; Python tools in `python/`). Version `3.0.0` (MAJOR — breaking strategy ABI; single source of truth `VERSION`, mirrored in workspace Cargo.toml).

Latest run (2026-09-29, run_id 20260929-f24b5e53, task_type implement, SUCCESS): the thread-scaling fix (review 20260929-b81785fd's fix_task.yaml). 12/12 acceptance criteria MET, tests 38+1 green (3 pre-existing Windows settings failures fixed), clippy 0/0, review approve (2 iterations). Benchmarks: 1m arc 16thr 44185→7202 s (6.13x), 2m arc 32thr 15316→1264 s (12.12x), later-wave stall eliminated (gap 0.15x/0.20x vs pre-fix 2.4x), exact business-metric parity with pre-fix CSVs on all shared columns; `Max_Drawdown_DateTime` now populated for all optimizers/modes (93/93 rows). Open backlog: 0.

BREAKING v3.0.0: strategy ABI — `create_strategy` takes an emission callback pair (`emitter_ctx`, `emit_signal_cb`) instead of `mpsc::Sender`; strategies store `SignalEmitter { ctx, cb }`; only POD crosses the FFI boundary, the host allocates SignalEvent boxes. mimalloc is the process allocator. **Every strategy dll must be rebuilt against the new farukon_core — old dlls crash the new binary** (allocator mismatch, proven by bisection: cross-boundary free of dll-allocated boxed events; rebuilding dlls with mimalloc does NOT help — two static copies = two heaps). Reference: `strategy_lib/src/lib.rs`; contract: `farukon_core/src/event.rs`.

Previous run (2026-09-29, run_id 20260929-b81785fd, review): thread-scaling root-cause analysis (per-candidate O(bars) memory + per-bar allocations + system allocator; first-wave effect = heap fragmentation). Its fix task + benchmark re-validation were closed by the run above.

## Ключевые решения (последний прогон, 2026-09-29)
- mimalloc сохранён по решению пользователя; падение E1 (mimalloc exe + старые dll) исправлено переделкой границы сигналов (allocator-safe callback), а НЕ пересборкой dll с mimalloc (опровергнуто эмпирически).
- Снапшоты позиций/холдингов больше не клонируют map'ы на каждом баре (пустые map'ы; читателей не существует — grep-verified); deals_count считается из живого состояния в конце.
- `calculate_final_performance` выполняется для любого metrics mode (был только Offline) — отсюда Max_Drawdown_DateTime для Grid/GA/realtime.
- Windows/MSVC: `~/.cargo/config.toml` [env] CXXFLAGS/CL=-utf-8 — иначе libmimalloc-sys не собирается с кириллическим путём пользователя.
- LSHADE: лучшая особь по `fitness_direction`; нумерация итераций 1-based; банк особей очищается BankClearGuard (из прогона 2026-09-10, актуально).

## Верификация
- `cargo test --workspace` → 39 passed / 0 failed; `cargo clippy --workspace` → 0/0; `cargo fmt --check` → exit 0.
- Бенчмарки 1m arc 16thr и 2m arc 32thr: checker `.code-factory/scripts_run/bench_check.py {1m|2m}` exit 0 (ускорение + паритет метрик).
- Проба E1 (1 кандидат 1m, mimalloc + новый ABI): EXIT=0, полный прогон 1169.9 s.

## Последняя история
- 2026-09-29: Thread-scaling fix (run 20260929-f24b5e53) — success, v3.0.0 (major, ABI break), 6.13x/12.12x на бенчмарках, backlog 2 → 0.
- 2026-09-29: Thread-scaling review (run 20260929-b81785fd) — root cause найден, fix_task.yaml создан, DEGRADED accepted (review не менял код).
- 2026-09-10: LSHADE доработка + кэш + обработчики — success, 34 теста, e2e-прогон, review approve, v2.1.0.

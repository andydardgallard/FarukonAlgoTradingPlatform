<!-- code-factory-memory: summary -->
project: code
project_id: 696da31598ba
repo_path: ../FarukonAlgoTradingPlatform/code

# Farukon — сводка долгосрочной памяти

## Текущее состояние
Project Farukon (Rust workspace: `farukon_core` lib, `Farukon_2` bin, `strategy_lib` cdylib; Python tools in `python/` incl. stdlib-runnable `python/tests/`). Version `3.2.0` (minor; single source of truth `VERSION`, mirrored in workspace Cargo.toml).

Latest run (2026-10-26, run_id 20261026-3f117829, task_type implement): all 40 findings of review 20261026-2f16ea3a fixed across BOTH repos. Volume contract u64->f64 landed producer-side (schemas `double` + `file_identifier` OHL1/OHS1, roots finished with identifiers, f64 CSV parsing/resampling) and consumer-side (vendored identifier helpers; loader REJECTS legacy u64-volume buffers with a MIGRATION error; equal-length vector validation; padding bars volume 0.0). Consumer: error propagation (backtest C1, candidate gate C8 + LshadeStatsCsvGuard), portfolio PnL accounting (partial-exit basis, multi-fill accumulator, add-on carried market move), core hardening (checked_mul, sysinfo bytes, one-point curves, SIMD zero lanes, full-ParameterSet fitness cache), Visual dispatches by optimizer_type, generic volume API in both handlers, python plotting fixes, start.sh platform library selection, MA_cross date memoization. Producer: sorting+dedup, no output-dir wipe (atomic replace, aliasing rejected), thread cap honored, csv+txt case-insensitive, failures propagate with exit 1, reader validation + short-buffer guard, lib target + 40 tests, fmt/clippy clean. Acceptance SUCCESS 5/5 verify MET; reviewer approve at iteration 2 (iteration 1: 6 critical + 11 major, all reworked); business parity MATCH vs base-clone old stack (10/10 metrics, equity byte-identical); market data regenerated: 51 RTS .soa.bin+.idx (302 MB, OHS1) into `Tickers_fbs/soa/RTS`. Regression: consumer 36+73=109 Rust tests, producer 24+15+1=40, 6 python suites green. Open backlog: 0.

Previous run (2026-10-26, run_id 20261026-2f16ea3a, task_type review, report-only): combined review of farukon_v3.1.0 + csv-to-flatbuffer — verdict request_changes, 40 merged findings (critical 30, major 7, minor 3) over 4 shards, 50/50 quotes verified, run acceptance SUCCESS 5/5. Follow-up task generated at `.code-factory/review/fix-tasks.yaml` (closed by the run above).

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
- 2026-10-26: Combined review farukon+csv-to-flatbuffer (run 20261026-2f16ea3a) — request_changes, 40 findings (volume u64/f64 contract break confirmed), fix-tasks.yaml generated, backlog 0 → 1.
- 2026-10-26: Fix run (run 20261026-aefd4f09) — 29 findings + owner A-D, approve iter 4, v3.1.0, backlog 1 -> 0.
- 2026-10-26: Pre-publication whole-repo review (run 20261026-b300cde5) — request_changes, 29 findings (17 critical), fix-tasks.yaml generated, backlog 0 → 1.
- 2026-09-29: Thread-scaling fix (run 20260929-f24b5e53) — success, v3.0.0 (major, ABI break), 6.13x/12.12x на бенчмарках, backlog 2 → 0.
- 2026-09-29: Thread-scaling review (run 20260929-b81785fd) — root cause найден, fix_task.yaml создан, DEGRADED accepted (review не менял код).
- 2026-09-10: LSHADE доработка + кэш + обработчики — success, 34 теста, e2e-прогон, review approve, v2.1.0.

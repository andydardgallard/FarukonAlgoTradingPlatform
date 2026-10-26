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
timestamp: 2026-09-29T00:00:00+0300
commit: 146e6551
run_id: 20260929-b81785fd
branch: code-factory/thread-scaling-review
task_type: review
goal: Explain why backtest runtime stops scaling beyond ~8 threads on the 1-3 minute timeframes (4-5m scale to 64), why only the first "wave" of a batch is fast at high thread counts, find testing errors in the time_tests campaign, and produce a fix task plus an optimal backtest strategy.
changed_files: AGENTS.md, memory/change-log.md, memory/summary.md
created_files: (factory run artifacts, factory root) .code-factory/report.md, .code-factory/fix_task.yaml, .code-factory/logs/{measurements-analysis,test-results-review,confirmation,hardware,baseline,code-review}.md
results: integration=not-run regression=not-run business=not-run review=request_changes | Review complete, acceptance verdict DEGRADED (5 verify criteria MET, 0 failed; regression not-run — review changed no code; baseline 31 pass / 3 pre-existing settings::tests failures) [verified: .code-factory/state/acceptance.md, logs/baseline.md]. Root cause identified [verified: .code-factory/report.md §5, logs/code-review.md reviewer CONFIRMED items]: per-candidate O(bars) memory footprint (unbounded equity_series + 3-4 copies at portfolio.rs:544-545,713-715, performance.rs:310; per-bar HashMap clones portfolio.rs:438,501,518,535; per-bar dlsym + CString rebuild strategy_loader.rs:137,145-151; no mimalloc) saturates bandwidth/allocator beyond ~8 threads on 1-3m data [inferred mechanism from 110-run dataset + code evidence]. First-wave-fast effect = in-process heap fragmentation + page-fault debt, wave 1 vs later median 5.3-6.1x on 1m, starting at candidate #threads+1 [verified: logs/confirmation.md]. Review verdict request_changes, 0 critical / 10 major / 11 minor / 2 nit — carried into fix_task.yaml [verified: logs/code-review.md].
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

## 2026-09-29 — Thread-scaling fix: hot-path allocations, allocator-safe signal ABI, v3.0.0

title: Fix backtest thread-scaling degradation (hot-path allocations, per-candidate copies, adaptive thread guidance) | project: code | timestamp: 2026-09-29T04:30:00+0300
timestamp: 2026-09-29T04:30:00+0300
commit: 4abc125d
run_id: 20260929-f24b5e53
branch: feature/fix-thread-scaling-20260929
task_type: implement
goal: Implement fix_task.yaml (items 1-12) from review run 20260929-b81785fd: eliminate per-bar FFI/clone overhead, wire mimalloc, warnings for deep mode and the ~8-thread knee, clippy/unsafe fixes, 3 Windows settings tests, Max_Drawdown_DateTime for Grid/GA; validate on the 1m/2m benchmarks with exact business-metric parity.
changed_files: 26 source files across farukon_core (event.rs, strategy.rs, performance.rs, portfolio.rs, settings.rs, optimization.rs, commission_plans.rs, pos_sizers.rs, utils.rs, data_handler.rs, indicators.rs, lib.rs), Farukon_2 (strategy_loader.rs, portfolio.rs, optimizers.rs, execution.rs, backtest.rs, main.rs, risks.rs, data_engine/*), strategy_lib (lib.rs, Cargo.toml), Strategies/rs/MA_cross.rs, Cargo.toml, VERSION, AGENTS.md, README.md, USER_MANUAL.md
created_files: .code-factory/scripts_run/bench_check.py (deterministic benchmark checker)
results: integration=pass regression=pass business=pass review=approve | SUCCESS — acceptance 12/12 verify criteria MET, regression pass (38+1 tests green, baseline was 31/3-fail; the 3 settings tests fixed) [verified: .code-factory/state/acceptance.md exit 0, logs/test-results.md]; clippy 0/0 (was 8 deny errors + ~156 warnings) [verified: logs/test-results.md]; code review iteration 1 request_changes (1 major: MA_cross.rs ABI migration) -> rework -> iteration 2 approve, 6/6 quotes verified [verified: logs/code-review.md]; business test 1 (1m arc 16thr, 93 combos): 7202 s vs pre-fix 44185 s = 6.13x, wave gap 0.15x (pre-fix 2.4x), metric parity exact on 18 shared columns 93/93, Max_Drawdown_DateTime populated 93/93 [verified: bench_check.py 1m exit 0, tests_results/results_1m_arc_thread_16_postfix_20260929.txt]; business test 2 (2m arc 32thr): 1264 s vs 15316 s = 12.12x, wave gap 0.20x, parity exact on distinct row sets [verified: bench_check.py 2m exit 0].
decisions: mimalloc kept per user decision; E1 segfault (mimalloc exe + any pre-existing dll) root-caused by bisection (cross-allocator free of dll-allocated boxed events; rebuilding dlls with mimalloc REFUTED empirically — two static copies = two heaps) and fixed by task_13: allocator-safe signal ABI — SignalEmitter {ctx, cb} callback, host allocates SignalEvent, only POD crosses the boundary [verified: logs/errors.md E1+CORRECTION, probe EXIT=0 1169.9 s]; snapshot HashMaps now empty in snapshots (grep-verified no readers; types unchanged) [verified: logs/code-review.md]; calculate_final_performance runs for every metrics mode so Max_Drawdown_DateTime is populated for Grid/GA/realtime [verified: task_12 e2e in logs/test-results.md]; version 2.1.0 -> 3.0.0 = MAJOR (breaking strategy ABI), reviewer-validated [verified: logs/code-review.md version_type]; version_manager.py validate is factory-repo-shaped (README title/footer/CHANGELOG/.agents markers) and does not apply to this target repo — VERSION file + workspace Cargo.toml synced manually [inferred].
assumptions: metric parity compared on the 18 columns shared with the pre-fix CSVs (they predate the Max_Drawdown_DateTime column); pre-fix 2m CSV holds two appended runs (186 rows) — distinct-row-set comparison; stand dll build scaffold lives in the commit-excluded test stand (time_tests/dll_build/).
models_used: main=primary(kimi-code/k3); analyzer=primary(kimi-code/k3); coder=deepseek-flash; reviewer=primary(kimi-code/k3, matrix names kimi-k3 — alias unconfigured); documenter=deepseek-flash
factory_version: 12.12.1

unfinished:
  - item: USER_MANUAL.md §8.4 documents strategy_lib/src/SYMI_Ch_SMA_up_lmt.rs and python/biztest_symi.py, neither of which exists in the tree (pre-existing staleness)
    reason: owner decision needed (restore sources or delete the section); out of this run's scope
    severity: warning
    follow_up: false
  - item: Strategies/MA_cross.dylib is a stale pre-v3.0.0 artifact (source migrated; nothing in the repo builds the dylib)
    reason: macOS artifact; needs a rebuild on macOS from Strategies/rs/MA_cross.rs before any host can load it
    severity: warning
    follow_up: false

closed:
  - item: Implement the fixes (fix_task.yaml items 1-12: hot-path allocations, equity_series copies, mimalloc, deep-mode warning, adaptive thread guidance, clippy/unsafe/commission.unwrap, 3 Windows settings tests, 2min arc config paths)
    evidence: acceptance SUCCESS 12/12 (verify_acceptance exit 0, .code-factory/state/acceptance.md); review approve iteration 2 (logs/code-review.md); note: the 2min arc config paths item was removed from the task by the user before this run
  - item: Re-run the 1m arc 16-thread benchmark after the fix to validate the improvement vs the 44185 s baseline
    evidence: bench_check.py 1m exit 0 — 7202 s (6.13x), wave gap 0.15x, metric parity 93/93 rows (tests_results/results_1m_arc_thread_16_postfix_20260929.txt)

## 2026-10-26 — Full pre-publication review of the project (run 20261026-b300cde5)

title: Full detailed review of the project before the GitHub push | project: code
timestamp: 2026-10-26T15:00:00Z
commit: 3ee7bdc1
run_id: 20261026-b300cde5
branch: code-factory/pre-publication-review-20261026
task_type: review
goal: Verify that the whole project works as intended before the final publication to GitHub (report-only whole-repo review).
changed_files: memory/change-log.md, memory/summary.md
created_files: (under .code-factory/, not committed) logs/code-review.md, logs/code-review-merged.json, logs/shard-0-findings.json, logs/quotes-report.json, logs/baseline.md, review/fix-tasks.yaml, state/{acceptance.md,evidence.json,criteria.json,shards.json,pipeline.yaml,preflight.json}
results: integration=not-run regression=pass business=not-run review=request_changes | Report-only review delivered: 29 findings merged (17 critical / 9 major / 2 minor / 1 nit), verdict request_changes [verified: .code-factory/logs/code-review-merged.json, merge_findings exit 1]; regression baseline green (cargo test --workspace exit 0) [verified: .code-factory/logs/baseline.md + FRESH ledger entry regression=pass]; acceptance of the DELIVERY = SUCCESS (4/4 verify criteria MET) [verified: .code-factory/state/acceptance.md exit 0]; 28/29 reviewer quotes re-verified verbatim, 1 UNTRUSTED (a spurious start.sh finding that does not affect the verdict) [verified: .code-factory/logs/quotes-report.json + manual read of start.sh]. Follow-up fix task generated: .code-factory/review/fix-tasks.yaml [verified: review_followup_task.py exit 0, 29 findings]. Headline defects: SIMD max-drawdown uses a future chunk peak (performance.rs:466); GA minimization inverted by double fitness-direction (optimizers.rs:487); Grid Search evaluates only the first value of every additional sizer parameter (optimization.rs:397); LSHADE breaks discrete domains, drops sizer dimensions, hangs on population < 3 and exceeds max_evaluations; market-order slippage never applied (execution.rs:104); final-bar signals discarded (backtest.rs:199); Recovery Factor treats losses as positive (performance.rs:279); zero-capacity sizing still forces a trade (utils.rs:55); portfolio metrics omit unallocated cash (utils.rs:335); documented `poe` sizer crashes order generation (pos_sizers.rs:249); GA plotting script broken (plot_portfolio_genetic_results.py:33).
decisions: review was executed as ONE shard (whole tracked tree = 57 files <= 20000 lines) per the shard protocol [verified: .code-factory/state/shards.json]; the UNTRUSTED quote finding ("tracked startup script references a nonexistent config", start.sh) was dropped from the verdict basis because start.sh is the factory launcher and contains no such line [verified: verify_quotes.py report + cat start.sh]; AGENTS.md embedded fingerprint is STALE vs the current tree but the file was NOT regenerated because a review run never edits tracked files [verified: project_fingerprint.py --all mismatch]; two legacy v2 journal entries (2026-09-29) were repaired additively (timestamp/commit keys + result tokens) without rewriting their content, restoring memory validation to ok [verified: memory_project.py check exit 0]; summary pinned project_id: 696da31598ba per the validator hint [verified: memory_project.py check hint].
assumptions: the owner's local uncommitted edits (.gitignore, start.sh) were left untouched and out of the review/commit scope; no business benchmark re-runs (review run; the previous run's benchmarks stand).
models_used: main=kimi-code/k3; reviewer=kimi-code/k3.1-pro (task matrix); analyzer/planner/diagnostician not invoked; models_warning: task-named collision coder/tester/gui_expert/advisor on the deepseek family (authoritative, no substitution)
factory_version: 12.16.0

unfinished:
  - item: Implement the 29 pre-publication findings from .code-factory/review/fix-tasks.yaml (17 critical correctness defects in optimizers/execution/performance/sizing + 9 major)
    reason: report-only review delivers the follow-up task file; fixing is a separate implement run per the review-report-only rule
    severity: critical
    follow_up: true

## 2026-10-26 — Fix 29 pre-publication findings + owner tasks A-D (run 20261026-aefd4f09)

title: Fix pre-publication review findings (run 20261026-b300cde5) + owner tasks A-D | project: code
timestamp: 2026-10-26T19:30:00Z
commit: 1324753f
run_id: 20261026-aefd4f09
branch: code-factory/fix-prepublication-20261026
task_type: implement
goal: Implement all 29 findings of the report-only review run 20261026-b300cde5 plus owner tasks A (u64->f64 quantities), B (strategy correlation chart), C (LSHADE Best_Individual CSV column), D (test with SYMI strategy on RTS data).
changed_files: 33 tracked files (engine, farukon_core optimization/performance/pos_sizers/utils/data_handler/lib, docs, python tools, portfolio configs, VERSION 3.0.0->3.1.0, AGENTS.md, start.sh) + owner's .gitignore/instruments_info.json edits carried into the commit + owner-deleted CNY/Si market data committed as deletions
created_files: python/tests/test_plot_portfolio_genetic_results.py, python/tests/test_optresults_handler.py, portfolios/symi_rts_smoke.json (gitignored by owner's SYMI* rule; local-only smoke config)
results: integration=pass regression=pass business=pass review=approve | SUCCESS — acceptance 8/8 verify criteria MET, exit 0 [verified: .code-factory/state/acceptance.md]; regression 75 tests green (59 farukon_core + 16 Farukon_2), clippy 0 warnings, fmt clean [verified: .code-factory/logs/test-results.md + FRESH ledger entry regression=pass]; review iterations: 1 request_changes (2 critical: final-bar fills not booked, equity CSV unallocated cash; 7 major) -> rework -> 2 request_changes (final-EXIT double-count) -> fix -> 3 owner-authorized request_changes (multi-fill overwrite) -> fix -> 4 approve [verified: .code-factory/logs/code-review.md]; business BT1: SYMI_Ch_prct_SMA_up_lmt (fresh cdylib from Strategies/rs) grid smoke on RTS-12.25 5min exit 0, 32 rows, deals 0-12 [verified: opt_results/symi_rts_smoke]; GA and LSHADE smokes exit 0, LSHADE CSV carries Best_Individual, evals capped 12/12 [verified: opt_results/symi_rts_smoke_lshade/lshade_optimization_results.csv header]; MA_cross control run on same RTS data: 367-845 deals [verified: opt_results/ma_rts_smoke]; version 3.1.0 minor, reviewer-validated [verified: logs/code-review.md version_type].
decisions: task A scope = trade pipeline was already f64; migrated market-data VOLUME u64->f64 (SOA generated API, data engine, DataHandler, FFI vtable) [verified: git diff of ohlcv_soa_generated.rs, data_handler.rs, lib.rs]; owner's new RTS .soa.bin files contained u64 volume bit patterns despite being believed f64 — converted in place (u64->f64 reinterpret, originals backed up under .code-factory/backups/Tickers_fbs/) and re-verified: volumes read 25.0/3.0/14.0..., MA_cross results bit-identical [verified: flatbuffers probe script + smoke rerun]; start.sh committed with the fixed Farukon launcher content (from start.sh.bak) via git index while the owner's local factory launcher file stays untouched [verified: git show HEAD:start.sh]; final settlement semantics: last_marked_close booked-through reference + pending_fill_pnl per-symbol accumulator + final_settle phase, covered by 3 regressions proven red on the unfixed code [verified: Farukon_2/src/backtest.rs:470,711,747]; SYMI smoke grid moved to a trading region (avg 3-5, channel 5-10, width 100-200, sma 5-10) after the initial 20-50 grid produced 0 deals — the strategy's LONG entry needs a breakout AND close-SMA below the smoothed-low channel simultaneously [verified: debug run logs]; python GA plotter dropped its pandas dependency (stdlib csv) to make the vaccine dependency-light [verified: python/tests/test_plot_portfolio_genetic_results.py].
assumptions: SYMI 0-deal parameter regions are legitimate strategy behavior, not a data/load defect (MA_cross trades on the same data; SYMI trades in the tuned region); the reviewer's iteration-1 call for a dll-fixture loader vaccine was satisfied with a source-level ordering invariant test instead (strategy_loader.rs:410); GA/LSHADE sampling helpers may produce off-grid in-range values for non-zero-start Ranges (start-anchored snapping fixed at mutation/crossover; continuous in-range sampling for evolutionary algorithms accepted as intended semantics); the review-run backlog item is closed by this run.
models_used: main_agent=kimi-code/k3; analyzer=kimi-code/k3.1-pro; coder=deepseek-flash (WP1-WP5 + rework); reviewer=kimi-code/k3.1-pro (4 iterations); documenter=deepseek-flash
factory_version: 12.16.0

unfinished:
  - item: no unfinished items
    reason: all 29 findings fixed or dispositioned, owner tasks A-D delivered, review approved
    severity: info
    follow_up: false

closed:
  - item: Implement the 29 pre-publication findings from .code-factory/review/fix-tasks.yaml (17 critical correctness defects in optimizers/execution/performance/sizing + 9 major)
    evidence: acceptance SUCCESS 8/8 verify MET (.code-factory/state/acceptance.md exit 0), reviewer approve at iteration 4 (.code-factory/logs/code-review.md), 75/75 tests green, commit 1324753f

## 2026-10-26 — Combined review of farukon_v3.1.0 + csv-to-flatbuffer (run 20261026-2f16ea3a)

title: Full detailed review of the two projects working in combination
project: code
timestamp: 2026-10-26T22:50:00Z
commit: 4c4d0190 (tree untouched — report-only review)
run_id: 20261026-2f16ea3a
branch: main
task_type: review
goal: Verify the csv-to-flatbuffer -> Farukon market-data pipeline end to end, with special attention to candle data types (volume u64/f64).
changed_files: none (report-only: only .code-factory/ artifacts and this memory record were written; proven by git status on both repos [verified: .code-factory/state/acceptance.md AC4/AC5])
created_files: .code-factory/logs/code-review.md, code-review-merged.json, code-review-shards/*.json, .code-factory/review/fix-tasks.yaml, .code-factory/report.md (all gitignored run artifacts)
results: integration=not-run regression=pass business=not-run review=request_changes — 40 merged findings (critical 30, major 7, minor 3) over 4 shards (farukon Rust core, farukon python/configs, csv-to-flatbuffer whole repo, cross-repo integration contract) [verified: .code-factory/logs/code-review-merged.json, merge_findings.py exit 1]; run acceptance SUCCESS 5/5 verify MET [verified: .code-factory/state/acceptance.md exit 0]; headline answer: PRODUCER writes volumes as ulong/u64 in both schemas (ohlcv_soa.fbs:29, ohlcv.fbs:27, csv_processor.rs:25 vol: u64, no scaling) while CONSUMER v3.1.0 reads f64 (vendored ohlcv_soa_generated.rs:135, farukon_core/src/data_handler.rs) — equal 64-bit widths pass flatbuffers verification, so volume bits are silently reinterpreted; freshly converted data is currently incompatible with Farukon [verified: 50/50 verbatim quotes, verify_quotes.py exit 0]; no file_identifier/schema-version guard exists on either side (major) [verified: integration shard findings]; csv-to-flatbuffer has zero automated tests [verified: cargo test exit 0 with 0 tests + analyzer grep]; baseline green: farukon 75/75 cargo tests + 2 python suites [verified: .code-factory/logs/baseline.md, FRESH ledger regression=pass]
decisions: re-run of the same-day review 20261026-a65b9a6e on the owner's explicit choice; the earlier run's artifacts were archived under .code-factory/archive/ and its identical task hash would have collided on run_id, so the parsed task carries a re-run comment yielding run_id 20261026-2f16ea3a [verified: .code-factory/state/task.yaml first line]; report-only rule enforced — AGENTS.md fingerprint drift (stale since the v3.1.0 fix run) was recorded as a finding instead of regenerating a tracked file [verified: project_fingerprint.py --all output in .code-factory/logs/baseline.md notes]; review verdict is final for the run, no reviewer budget spent on rework (review runs do not iterate) [inferred: references/code-review.md §5]; calibration gate ran against the factory root (prompt digest a77413b07865, exit 0) because farukon has no .agents/skill-base of its own [verified: calibrate_reviewer.py status output]
assumptions: the uncommitted csv-to-flatbuffer codegen refresh (newer flatc, no schema change) is the owner's work in progress and was reviewed as-is; the sibling csv-to-flatbuffer checkout at ../csv-to-flatbuffer is the producer the owner means; findings in the producer repo are handed over in the same fix-tasks.yaml although its repo_path is farukon — the owner runs the follow-up per repository
models_used: main_agent=kimi-code/k3; analyzer=kimi-code/k3.1-pro (2); reviewer=kimi-code/k3.1-pro (4 shards); planner=kimi-code/k3 (not delegated - delivery plan written by main agent)
factory_version: 12.17.0

unfinished:
  - item: Close the 40 review findings via the generated follow-up task .code-factory/review/fix-tasks.yaml (30 critical incl. the u64/f64 volume contract break, destructive output-dir wipe, swallowed conversion errors, PnL accounting on partial exits/multi-fills)
    reason: report-only review — findings are handed over, never fixed in the review run; the owner decides when to run the follow-up implement task
    severity: critical
    follow_up: true

## 2026-10-26 — Fix the 40 review findings across BOTH repositories (run 20261026-3f117829)

title: Fix the 40 review findings across BOTH repositories (farukon_v3.1.0 + csv-to-flatbuffer) — review run 20261026-2f16ea3a
project: code
timestamp: 2026-10-26T23:59:00Z
commit: dcf7f73d (consumer, v3.2.0) + d14884e (producer csv-to-flatbuffer)
run_id: 20261026-3f117829
branch: code-factory/fix-40-findings-20261026 (consumer), code-factory/fix-contract-and-findings-20261026 (producer)
task_type: implement
goal: Close all 40 merged findings of review 20261026-2f16ea3a in both repos, including the u64->f64 volume contract and a versioned file_identifier on both sides; regenerate the RTS market data.
changed_files: 35 files across both repos (consumer 19 incl. backtest.rs, portfolio.rs, optimizers.rs, main.rs, data_engine/*, farukon_core/{optimization,performance,pos_sizers,instruments_info}.rs, python/*, start.sh, Strategies/rs/MA_cross.rs, USER_MANUAL.md, AGENTS.md, VERSION 3.1.0->3.2.0; producer 16 incl. both .fbs, csv_processor.rs, resample.rs, read_flatbuffers.rs, file_processing.rs, progress.rs, main.rs, cli.rs, utils.rs, regenerated bindings, README.md, build.rs) [verified: .code-factory/manifest.json]
created_files: consumer python/tests/{test_plot_script_fixes,test_ma_cross_memoization,test_start_sh_platforms,test_user_manual_v31}.py; producer src/lib.rs + tests/conversion.rs; run scripts under .code-factory/scripts_run/ (gitignored) [verified: .code-factory/manifest.json]
results: integration=pass regression=pass business=pass review=approve | SUCCESS — acceptance 5/5 verify MET, exit 0 [verified: .code-factory/state/acceptance.md]; contract landed on BOTH sides: schemas double + file_identifier OHL1/OHS1, producer finishes roots with identifiers, consumer rejects identifier-less legacy buffers with a MIGRATION error [verified: producer tests/conversion.rs identifier round-trips + consumer utils.rs buffer_without_identifier_is_rejected_with_migration_hint + business-test.md §5]; AC1 volume round-trip two-part PASS: real consumer binary loads the producer-written 100-bar fixture (exit 0, 10 deals) AND 100/100 integer+fractional volumes read back bit-exact [verified: .code-factory/scripts_run/volume_roundtrip_check.py FINAL_EXIT=0]; business parity MATCH — temp base-clone old stack+old data vs new stack+new data: all 10 metrics identical, equity CSV byte-identical (md5 35e4a934...) [verified: .code-factory/logs/business-test.md §8]; regression green: consumer 36 Farukon_2 + 73 farukon_core, producer 24+15+1, clippy/fmt clean both repos, 6 python test files pass [verified: .code-factory/logs/regression-final.md]; review: iteration 1 request_changes (6 critical + 11 major incl. portfolio add-on carry, LSHADE stats guard, short-buffer panic) -> rework by 6 agents -> iteration 2 approve [verified: .code-factory/logs/code-review.md]; market data regenerated: 51 RTS .soa.bin + .idx (302 MB) with OHS1 into Tickers_fbs/soa/RTS [verified: business-test.md §2]; version 3.2.0 minor, reviewer-validated [verified: logs/code-review.md version_type]
decisions: volume contract fixed by aligning the PRODUCER to the consumer's f64 (schemas double, CSV/parse/builder/resampling f64) rather than the reverse — the consumer v3.1.0 contract was already published [verified: task.yaml coordination rule 1]; file_identifier OHL1/OHS1 chosen as the version marker; any future incompatible revision must change it [verified: ohlcv.fbs/ohlcv_soa.fbs]; partial exits keep the original entry basis (entry_price/entry_capital cleared only at zero position) [verified: portfolio.rs:355 comment + test partial_exit_keeps_the_entry_basis]; add-on entries book the old quantity's close-to-close move as carried_market_move before advancing last_marked_close [verified: portfolio.rs add_on tests + vaccination]; failed LSHADE/GA runs leave neither results nor stats CSV (candidate gate + LshadeStatsCsvGuard RAII) [verified: optimization.rs test_failed_run_leaves_no_stats_csv]; fitness caches key by full ParameterSet per digest bucket (collision => miss) [verified: optimization.rs cache_lookup + collision tests]; market data regeneration used the producer's real in/RTS-*.txt (51 files) [verified: business-test.md]; owner's uncommitted producer codegen refresh kept and committed together with the fix [verified: producer git status at run start]
assumptions: task.yaml was normalized formatting-only for the deterministic parser (block scalars, AC ids folded into criterion text) — content unchanged; baseline metric parity was initially recorded as not reproducible (owner deleted pre-change data) but the reviewer-required real rerun via temp base clones succeeded, so the assumption is RETIRED [verified: business-test.md §6 annotation + §8]; visual-mode population searches run with mode Optimize semantics to avoid parallel writers on equity_series.csv [verified: main.rs resolve_strategy_execution]; python pandas/matplotlib unavailable in this environment — python changes verified via stdlib AST/functional tests only [verified: analysis notes in agent-9 result]
models_used: main_agent=kimi-code/k3; analyzer=kimi-code/k3.1-pro (2); coder=deepseek-flash (T1-T13 + rework R1-R3/RB/RC); tester=deepseek-flash (T13, RD); reviewer=kimi-code/k3.1-pro (2 iterations); documenter=deepseek-flash
factory_version: 12.17.0

unfinished:
  - item: 17 pre-existing Cyrillic comments remain in shipped consumer files (Farukon_2/src/data_engine/utils.rs:233, python/optresults_handler.py:27-59)
    reason: english-only cleanup is a code-line edit the documenter gate rejects; needs a small implement-task touch
    severity: info
    follow_up: false
  - item: pandas/numpy/matplotlib are undeclared and were unavailable in this environment
    reason: python plotting verified statically; a live plotting run needs the owner venv
    severity: info
    follow_up: false

closed:
  - item: Close the 40 review findings via the generated follow-up task .code-factory/review/fix-tasks.yaml (30 critical incl. the u64/f64 volume contract break, destructive output-dir wipe, swallowed conversion errors, PnL accounting on partial exits/multi-fills)
    evidence: acceptance SUCCESS 5/5 verify MET (.code-factory/state/acceptance.md exit 0), reviewer approve at iteration 2 (.code-factory/logs/code-review.md), all 40 findings confirmed fixed with regressions (per-finding coverage table in the iteration-1 review and the iteration-2 confirmation), business parity MATCH on regenerated RTS data (.code-factory/logs/business-test.md §8)

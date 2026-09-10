<!-- code-factory-fingerprint: 590f259a122eb745fd94f77b6c62efeb8fffe48c9db47e70a40ba66b4d59ddb1 -->
# Farukon

## Project Overview

Farukon is a trading-strategy optimizer. It runs several optimization algorithms (Grid Search,
Genetic Algorithm, and LSHADE-RSP) over strategy parameters, backtests each candidate against
market data, computes performance metrics (Total Return, APR, Max Drawdown, Recovery Factor,
Composite, etc.), and writes results to CSV files under `opt_results/`. A set of Python scripts
visualizes the optimization and portfolio results.

The repository is a Rust workspace plus Python tooling. There is no CI configuration.

## Technology Stack

- **Rust** (edition 2024, cargo/rustc 1.97), Cargo workspace, resolver 3.
- Crates: `farukon_core` (library), `Farukon_2` (binary), `strategy_lib` (cdylib).
- Key Rust dependencies: `rayon` (parallel evaluation), `rand`/`rand_distr`, `serde`/`serde_json`,
  `csv`, `clap`, `chrono`, `wide` (SIMD), `memmap2`, `flatbuffers`, `bincode`, `libloading`,
  `num_cpus`, `anyhow`, `itertools`, `sysinfo`, `futures`.
- **Python 3.12** tooling for visualization: `pandas`, `numpy`, `matplotlib` (not declared in any
  requirements file; a Windows venv is expected at `python/VM`).

## Architecture Overview

- `Farukon_2` (binary, entry point `Farukon_2/src/main.rs:17`) — CLI (clap), backtest engine,
  portfolio handling, execution, strategy loader, data engine, and the optimizer orchestration
  layer (`Farukon_2/src/optimizers.rs`).
- `farukon_core` (library) — core algorithms and domain types. The most important file is
  `farukon_core/src/optimization.rs` (~2400 lines) which contains Grid Search, the Genetic
  Algorithm, and LSHADE-RSP. Other modules: `performance.rs` (metrics), `settings.rs`,
  `pos_sizers.rs`, `indicators.rs`, `strategy.rs`, `data_handler.rs`, `utils.rs`.
- `strategy_lib` (cdylib) — a sample MA-cross strategy compiled as a shared library and loaded at
  runtime via `libloading`.
- Dispatch: `main.rs` maps `OptimizerType::{GridSearch, Genetic, LshadeRSP}` to
  `optimizers.rs::{run_grid_search, run_genetic_search, run_lshade_rsp_search}`.

## Directory Structure

- `farukon_core/src/` — core library (optimization, performance, settings, indicators, …).
- `Farukon_2/src/` — binary and orchestration (`main.rs`, `cli.rs`, `optimizers.rs`, `backtest.rs`,
  `portfolio.rs`, `execution.rs`, `strategy_loader.rs`, `data_engine/`).
- `strategy_lib/src/` — cdylib sample strategy.
- `python/` — visualization/analysis scripts (`optresults_handler.py`, `visual.py`,
  `plot_portfolio_*.py`).
- `portfolios/` — portfolio JSON configs (e.g. `lshade_si_25_apr_dd.json`).
- `Strategies/` — prebuilt strategy shared libraries (e.g. `MA_cross.dylib`).
- `Tickers_fbs/` — market data in flatbuffers/SOA format.
- `commission_plans.json`, `instruments_info.json` — static config data.

## Key Configuration Files

- `Cargo.toml` (workspace) and per-crate `Cargo.toml`.
- `portfolios/*.json` — portfolio/optimizer configuration. `exit_results_path` inside each config
  determines the `opt_results/<name>` output directory.
- `commission_plans.json`, `instruments_info.json` — commission and instrument metadata.
- `.gitignore` — excludes build artifacts and `opt_results/**`, `python/VM/**`, etc.

## Build & Run Instructions

```bash
cargo build --release
cargo test --workspace           # 25 tests in farukon_core, 0 elsewhere
cargo build -p strategy_lib --release   # build the sample strategy cdylib
cargo run --release -- --config portfolios/lshade_si_25_apr_dd.json
```

The single CLI argument is `--config <path>` (clap `Args` in `Farukon_2/src/cli.rs`). Config `mode`
drives Optimize / Visual / Portfolio behavior.

## Dependencies & Integrations

- Rust workspace: `farukon_core`, `Farukon_2`, `strategy_lib`.
- `farukon_core`: `wide, rand, csv, futures, rayon, anyhow, itertools, sysinfo, num_cpus,
  rand_distr, serde_json, chrono, serde`.
- `Farukon_2`: `clap, csv, wide, rayon, memmap2, bincode, anyhow, libloading, num_cpus,
  serde_json, flatbuffers, farukon_core, chrono, serde`.
- `strategy_lib`: `anyhow, serde_json, farukon_core, chrono` (crate-type `cdylib`).
- Python scripts read the CSVs produced by the Rust binary (`;`-delimited) using pandas.

## Known Constraints & Limitations

- `save_stats_to_csv` for LSHADE (`farukon_core/src/optimization.rs`) currently **truncates**
  (`WriterBuilder::from_path`) and is invoked only every 10 iterations, so per-iteration history is
  not reliably persisted.
- LSHADE-RSP has **no** per-individual cache; the Genetic Algorithm has `chromosome_bank`
  (a `HashMap<u64, f64>` keyed by `DefaultHasher`), which has collision risk and does not hash
  `pos_sizer_name`.
- `PerformanceMetrics` does not store the timestamp of max drawdown, so `Max_Drawdown_DateTime`
  cannot currently be emitted to `optimization_results.csv`.
- `python/optresults_handler.py` has no "compare two CSVs" mode (`select` mode is a stub), and its
  Python dependencies are not declared anywhere.
- No CI, no `tests/` directory; tests are `#[cfg(test)]` modules inside `farukon_core/src`.

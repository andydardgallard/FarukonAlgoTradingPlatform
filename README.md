# 🚀 Farukon Algo Trading Platform

**Ultra-Fast, Low-Latency, Event-Driven Algorithmic Trading Engine**

Farukon is a high-performance, Rust-based algorithmic trading platform designed for **ultra-low-latency backtesting, multi-strategy optimization**, and **real-time execution**. Built **with zero-copy FlatBuffers, SIMD-accelerated calculations, and a multi-threaded event-driven architecture,** Farukon enables researchers and traders to prototype, test, and deploy strategies with minimal overhead.

The platform supports dynamic strategy loading via `.dylib`/`.so` libraries, grid search and genetic algorithm optimization, margin-aware position sizing, and real-time performance tracking.

 ## 🚀 Key Features

* ✅ **Event-Driven Architecture**: Decouples data, strategy, portfolio, and execution for maximum modularity and speed.
* ✅ **Zero-Copy Data Access**: Uses FlatBuffers + `mmap` for memory-mapped OHLCV data — no copying, no allocations.
* ✅ **Structure-of-Arrays (SOA) Format**: Data is stored and processed in SOA format (`[open1, open2, ...], [high1, high2, ...]`), which is **optimized for SIMD operations** and cache-friendly access
* ✅ **SIMD Optimization**: Leverages the `wide` crate for vectorized computations on indicators, returns, and drawdowns.
* ✅ **Multi-Threading**: Full parallelization across strategies, data loading, and optimization (up to 128+ threads).
* ✅ **Dynamic Strategy Loading**: Compile strategies as shared libraries (`cdylib`) and load them at runtime — no recompilation needed.
* ✅ **Multi-Strategy & Multi-Asset Support**: Run independent strategies on different instruments simultaneously.
* ✅ **Advanced Optimization**: Grid Search and Genetic Algorithm optimizers with composite fitness functions.
* ✅ **Margin & Risk Management**: Automatic position sizing, margin call detection, and forced liquidation.
* ✅ **JSON Configuration**: All settings are externally configurable — ideal for automated experimentation.

## 📦 Project Structure
```
FarukonAlgoTradingPlatform/
├── Farukon_2_0/ # Main backtesting executable
│ ├── src/
│ │ ├── main.rs # Entry point
│ │ ├── cli.rs # CLI parser
│ │ ├── backtest.rs # Core backtesting loop
│ │ ├── execution.rs # Simulated execution engine
│ │ ├── optimizers.rs # Grid Search & Genetic Algorithm
│ │ ├── portfolio.rs # Portfolio & risk management
│ │ ├── risks.rs # Margin call logic
│ │ ├── strategy_loader.rs # Dynamic .dylib loader
│ │ └── data_engine/ # Data handling and storage module
│ │ ├── mod.rs # Module declaration
│ │ ├── data_handler.rs # Zero-copy SOA data handler (implements DataHandler trait)
│ │ ├── global_data_storage.rs # Centralized store for pre-resampled SOA data
│ │ ├── ohlcv_soa_generated.rs # FlatBuffer generated code (SOA format)
│ │ └── utils.rs # Helper functions for data loading, resampling, and alignment
│ └── Cargo.toml
├── farukon_core/ # Shared core library
│ ├── src/
│ │ ├── event.rs # Event system (MARKET, SIGNAL, ORDER, FILL)
│ │ ├── data_handler.rs # DataHandler trait definition
│ │ ├── execution.rs # ExecutionHandler trait
│ │ ├── portfolio.rs # Position, Holding, Equity state
│ │ ├── performance.rs # SIMD-backed metrics (APR, DD, Recovery)
│ │ ├── indicators.rs # SMA, etc. (can use SOA data efficiently)
│ │ ├── instruments_info.rs # Instrument metadata
│ │ ├── commission_plans.rs # Commission rules
│ │ ├── settings.rs # Config parsing & validation
│ │ ├── optimization.rs # Grid + GA logic
│ │ ├── pos_sizers.rs # MPR, fixed_ratio, etc.
│ │ ├── utils.rs # Helpers
│ │ └── lib.rs # Public API
│ └── Cargo.toml
├── strategy_lib/ # Example strategy (Moving Average Cross)
│ ├── src/
│ │ └── lib.rs # Compiled as cdylib → libstrategy_lib.dylib
│ └── Cargo.toml
├── Tickers/ # Market data directory (FlatBuffers .soa.bin/.idx files)
├── Portfolios/ # Strategy configuration files (.json)
├── commission_plans.json # Commission structure per exchange
├── instruments_info.json # Contract metadata (margin, step, expiration)
├── LICENSE
├── USER_MANUAL.md
└── README.md
```
> 💡 **Note**: The [csv-to-flatbuffer](https://github.com/andydardgallard/csv-to-flatbuffer) utility (see below) generates `.bin` and `.idx` files for the `Tickers/` directory.

## 🛠 Getting Started

1. **Prerequisites**: Install [Rust 1.78+](https://rust-lang.org/tools/install/)
2. **Clone the Repository:**
   ```bash
   git clone https://github.com/andydardgallard/FarukonAlgoTradingPlatform.git
   cd FarukonAlgoTradingPlatform
   ```
3. **Build the Project**:
   ```bash
   cargo build --release
   ```
4. **Prepare Market Data**
Place your OHLCV data in the `Tickers/` directory as FlatBuffer `soa.bin` + `soa.idx` files. These files store data in the **Structure-of-Arrays (SOA) format**.
> ✅ Generate these files using our companion tool:
>
> 🔗 [csv-to-flatbuffer](https://github.com/andydardgallard/csv-to-flatbuffer)
>
> High-performance tool to convert CSV/TXT financial tick/OHLCV data into FlatBuffer SOA binary format for ultra-fast backtesting.
* ✅ Zero-copy reading via mmap
* ✅ Multi-threaded conversion
* ✅ Resampling to 1/2/3/4/5min, 1d
* ✅ Fast random access via .idx index
* ✅ Stores data in SOA format for SIMD optimization
> 
> Example:
> ```bash
> cargo run --release -- \
>    -i ./Tickers/FBS/Si \
>    -o ./Tickers/FBS/Si/Si-12.23.bin \
>    -t 8 \
>    -r 4min
> ```

5. **Configure Strategy**
   Edit `Portfolios/Debug_Portfolio.json` (see Configuration section below).
6. **Run the Backtester**
   ```bash
   cargo run --release -- --config Portfolios/Debug_Portfolio.json
   ```

## ⚙️ Configuration (JSON Settings)

All behavior is controlled via a single JSON config file passed with `--config`.

**Top-Level Object**
```json
{
  "common": { ... },
  "portfolio": { ... }
}
```
`common` **(Object): Global Settings**
* `mode` (String): Operational mode. Valid values: `"Debug"`, `"Optimize"`, `"Visual"`.
* `initial_capital` (float): Starting capital for the entire portfolio, in base currency (e.g., USD). No need to sum strategy weights to 1.0 — unused capital remains in cash.
`portfolio` **(Object): Strategy Definitions**

A map where keys are unique strategy IDs (e.g., `"Strategy_1"`), and values are strategy configurations.

**Strategy Configuration** (`portfolio.<strategy_id>`)
```json
{
  "threads": 8,
  "strategy_name": "MovingAverageCrossStrategy",
  "strategy_path": "target/release/libstrategy_lib.dylib",
  "strategy_weight": 1.0,
  "slippage": [0.005],
  "data": { ... },
  "symbol_base_name": "Si",
  "symbols": ["Si-12.23", "Si-3.24"],
  "strategy_params": { ... },
  "pos_sizer_params": { ... },
  "margin_params": { ... },
  "portfolio_settings_for_strategy": { ... },
  "optimizer_type": "Grid_Search"
}
```

* `threads` (int, optional): Number of CPU threads to use for this strategy’s calculations. Defaults to `num_cpus::get()`.
✅ Fully multi-threaded: Each strategy runs independently in its own thread pool.
* `strategy_name` (string): Name of the strategy class (e.g., `MovingAverageCrossStrategy`). Must match the exported symbol in the `.dylib`.
* `strategy_path` (string): Path to the compiled dynamic library (`.dylib` on macOS/Linux, `.dll` on Windows).
* `strategy_weight` (float): Proportion of `initial_capital` allocated to this strategy. Unused capital remains in cash — no need to sum to 1.0.
* `slippage` (array of float OR range object):
Slippage applied to market orders as a percentage of price.
  * Single value: `[0.005]`
  * Range: `{"start": 0.001, "end": 0.01, "step": 0.001}` → generates `[0.001, 0.002, ..., 0.01]`
* `data` (object): Data source configuration.
  * `data_path` (string): Path to directory containing `.bin`/`.idx` files (e.g., `"Tickers/FBS/Si"`).
  * `timeframe` (string): Target resampled timeframe. Valid values: `"1min"`, `"2min"`, `"3min"`, `"4min"`, `"5min"`, `"1d"`.
* `symbol_base_name` (string): Base symbol name (e.g., `"Si"`) used to look up contract metadata in `instruments_info.json`.
* `symbols` (array of strings): List of contract symbols to trade (e.g., `["Si-12.23", "Si-3.24"]`). Must exist in `instruments_info.json`.
* `strategy_params` (object): Strategy hyperparameters.
Each key is a parameter name; value is an array of discrete values or a range object.
  * Discrete: `"short_window": [50, 100, 150]`
  * Range: `"long_window": {"start": 500, "end": 1000, "step": 100}` → `generates [500, 600, 700, 800, 900, 1000]`
* `pos_sizer_params` (object): Position sizing configuration.
  * `pos_sizer_name` (string): Sizing method. Valid: `"mpr"`, `"poe"`, `"fixed_ratio"`, `"1"`.
  * `pos_sizer_params` (object, optional): Additional parameters (currently unused for `mpr`).
  * `pos_sizer_value` (array of float OR range object): Value(s) for the position sizer parameter.
    * Single: `[1.5]`
    * Range: `{"start": 1.0, "end": 3.0, "step": 0.5}` → generates `[1.0, 1.5, 2.0, 2.5, 3.0]`
* `margin_params` (object): Risk control.
  * `min_margin` (float): Minimum margin requirement as fraction of strategy capital (e.g., 0.5 = 50%).
  * `margin_call_type` (string): Action on margin breach. Currently only `"close_deal"` supported.
* `portfolio_settings_for_strategy` (object): Performance metrics mode.
  * `metrics_calculation_mode` (string):
    * `"offline"`: Calculate metrics once at end of backtest (faster).
    * `"realtime"`: Update metrics incrementally during backtest (slower, for visualization).
* `optimizer_type` (string or object): Optimization method.
  * `"Grid_Search"`: Exhaustive search over all parameter combinations.
  * `{ "Genetic": { "ga_params": { ... } } }`: Genetic Algorithm optimizer.
    * `ga_params` (object):
      * `population_size` (int): Number of individuals per generation.
      * `p_crossover` (float): Crossover probability (0.0–1.0).
      * `p_mutation` (float): Mutation probability (0.0–1.0).
      * `max_generations` (int): Max generations to run.
      * `fitness_params` (object):
        * `fitness_direction` (string): `"max"` or `"min"` (optimize for max or min fitness).
        * `fitness_value` (string or object): Metric(s) to optimize.
          * Single: `"APR"`, `"Total_Return"`, `"MaxDD"`, `"Recovery_Factor"`, `"Deals_Count"`.
          * Composite:
            ```json
            {
              "Composite": {
                "metrics": ["APR/DD_factor", "Recovery_Factor", "Deals_Count"]
              }
            }
            ```

## 🧠 For AI Systems
Farukon is designed to be **AI-native** — a platform for automated strategy discovery and hyperparameter optimization.

* **Standardized API**: The `farukon_core::strategy::Strategy` trait defines a clean interface for AI agents to implement trading logic.
* **Structured Configuration Space**: JSON config allows AI to generate, mutate, and evaluate millions of parameter combinations automatically.
* **Vectorized Performance Metrics**: `PerformanceManager` uses SIMD to compute returns, drawdowns, and APR — ideal for gradient-free optimization.
* **Parallelized Optimization**: Grid Search and Genetic Algorithm run across 100s of threads — AI can spawn thousands of parallel evaluations.
* **Zero-Copy Data Access**: AI models query OHLCV data directly from memory-mapped `.bin` files — no serialization overhead.
* **Dynamic Strategy Loading**: AI can compile and load new strategy libraries on-the-fly without restarting the engine.
* **Fitness Evaluation Hook**: The `calculate_fitness_score` function in `optimizers.rs` exposes raw metrics (`TotalReturn`, `APR/DD`, etc.) for reinforcement learning or Bayesian optimization pipelines.

> ### 💡 Suggested AI Workflow:
> 
> Use Farukon as a **fitness function evaluator**.
> 
> An AI agent (e.g., Optuna, BayesianOptimization, or custom RL) generates parameter sets → > Farukon runs backtest → Returns metrics → Agent updates policy → Repeat.

### LSHADE-RSP Optimizer

**LSHADE-RSP** (Linear Success-History based Adaptive Differential Evolution with Rank-based Selective Pressure) is a state-of-the-art Differential Evolution variant that combines:
- **LPSR** (Linear Population Size Reduction) — the population shrinks during optimization, focusing evaluations on the most promising regions
- **SHADE** (Success-History based Adaptive Differential Evolution) — adaptive F (mutation factor) and CR (crossover rate) parameters automatically tuned from historical memory
- **RSP** (Rank-based Selective Pressure) — individuals are selected for mutation based on their rank, directing search toward high-fitness areas

This is the third optimizer available in the platform, alongside Grid Search and Genetic Algorithm.

**JSON Configuration Example:**
```json
"optimizer_type": {
  "LSHADE_RSP": {
    "lshade_params": {
      "population_size": 50,
      "max_evaluations": 100,
      "p_best": 0.1,
      "archive_rate": 1.0,
      "memory_size": 5,
      "fitness_params": {
        "fitness_direction": "max",
        "fitness_value": "APR/DD_factor"
      }
    }
  }
}
```

**Full Settings Reference:**

| Parameter | Type | Default | Description | Affects | Recommended |
|-----------|------|---------|-------------|--------|-------------|
| `population_size` | int | 50 | Initial population size | Wider initial search coverage; more = better diversity, slower per iteration | 20–200 |
| `max_evaluations` | int | 200 | Maximum fitness evaluations (stop criterion) | Primary stop condition; more = longer search, potentially better optimum | 100–10,000 |
| `p_best` | float | 0.1 | Fraction of best individuals used for mutation direction | Greediness: higher = faster convergence, higher risk of premature convergence | 0.05–0.3 |
| `archive_rate` | float | 1.0 | External archive size as fraction of current population | Diversity: archive stores displaced solutions used for mutation diversity | 0.5–1.5 |
| `memory_size` | int | 5 | Size of historical memory for adaptive F and CR | Adaptation speed: smaller = faster adaptation but noisier | 3–10 |
| `fitness_params` | object | — | Fitness function configuration (same as Genetic) | Target metric and optimization direction (max/min) | Same as GA |

**How to Run:**
```bash
# Build the strategy library first
cargo build -p strategy_lib --release

# Run LSHADE-RSP optimization
cargo run --release -- -c portfolios/lshade_si_25_apr_dd.json
```

> 💡 **Comparison Note:** LSHADE-RSP uses adaptive F/CR parameters (automatically tuned from historical memory) and Linear Population Size Reduction — the population shrinks during optimization, focusing evaluations on the most promising regions. This typically finds equal or better solutions than Genetic Algorithm in fewer evaluations.

## 📈 Why FlatBuffers + SOA + SIMD?
Farukon is engineered for **ultra-low-latency**:
| Feature | Benefit |
|--------|-----------|
| ✅ **FlatBuffers** `.bin` + `.idx`| Zero-copy memory mapping; no parsing overhead. Random access to any timestamp via `.idx`. |
| ✅ `mmap` | Load 10GB of OHLCV data in < 0.1s — data stays in OS page cache. |
| ✅ **Structure-of-Arrays (SOA) Format** | Stores data as separate arrays (e.g., `opens`, `highs`). **Optimal for SIMD** and cache-friendly access |
| ✅ **SIMD (**`wide` **crate)** | Vectorized SMA, returns, and drawdown calculations — 4x–8x speedup. |
| ✅ **Multi-threaded Data Loader** | Each strategy loads its own data in parallel. |
| ✅ **Multi-threaded Optimization** | Grid search and GA run across all CPU cores — 100k+ combinations in minutes. |
| ✅ **Dynamic Libraries** | Strategies compiled separately → hot-swappable without recompiling engine. |

## 📁 File Structure Reference

`Tickers/`
```
Tickers/
└── FBS/
    └── Si/
        ├── Si-12.23.soa.bin     ← FlatBuffer OHLCV data
        ├── Si-12.23.soa.idx     ← Index: timestamps, daily ranges, resampled bars
        ├── Si-3.24.soa.bin
        └── Si-3.24.soa.idx
```
`Portfolios/`
```
Portfolios/
└── Debug_Portfolio.json   ← Main config
└── Optimize_Portfolio.json ← For GA optimization
```

`instruments_info.json`

Defines contract meta margin, step, expiration, commission type.
See provided example in repo.

`commission_plans.json`

Defines commission rates per exchange and instrument type.
See provided example in repo.

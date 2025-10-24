# 🚀 Farukon Algo Trading Platform

**Ultra-Fast, Low-Latency, Event-Driven Algorithmic Trading Engine**

Farukon is a high-performance, Rust-based algorithmic trading platform designed for **ultra-low-latency backtesting, multi-strategy optimization**, and **real-time execution**. Built **with zero-copy FlatBuffers, SIMD-accelerated calculations, and a multi-threaded event-driven architecture,** Farukon enables researchers and traders to prototype, test, and deploy strategies with minimal overhead.

The platform supports dynamic strategy loading via `.dylib`/`.so` libraries, grid search and genetic algorithm optimization, margin-aware position sizing, and real-time performance tracking.

 ## 🚀 Key Features

* ✅ **Event-Driven Architecture**: Decouples data, strategy, portfolio, and execution for maximum modularity and speed.
* ✅ **Zero-Copy Data Access**: Uses FlatBuffers + `mmap` for memory-mapped OHLCV data — no copying, no allocations.
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
├── Farukon_2_0/           # Main backtesting executable
│   ├── src/
│   │   ├── main.rs        # Entry point
│   │   ├── cli.rs         # CLI parser
│   │   ├── backtest.rs    # Core backtesting loop
│   │   ├── data_handler.rs # Zero-copy FlatBuffers loader
│   │   ├── execution.rs   # Simulated execution engine
│   │   ├── optimizers.rs  # Grid Search & Genetic Algorithm
│   │   ├── portfolio.rs   # Portfolio & risk management
│   │   ├── risks.rs       # Margin call logic
│   │   └── strategy_loader.rs # Dynamic .dylib loader
│   └── Cargo.toml
├── farukon_core/          # Shared core library
│   ├── src/
│   │   ├── event.rs       # Event system (MARKET, SIGNAL, ORDER, FILL)
│   │   ├── data_handler.rs # DataHandler trait
│   │   ├── execution.rs   # ExecutionHandler trait
│   │   ├── portfolio.rs   # Position, Holding, Equity state
│   │   ├── performance.rs # SIMD-backed metrics (APR, DD, Recovery)
│   │   ├── indicators.rs  # SMA, etc.
│   │   ├── instruments_info.rs # Instrument metadata
│   │   ├── commission_plans.rs # Commission rules
│   │   ├── settings.rs    # Config parsing & validation
│   │   ├── optimization.rs # Grid + GA logic
│   │   ├── pos_sizers.rs  # MPR, fixed_ratio, etc.
│   │   ├── utils.rs       # Helpers
│   │   └── lib.rs         # Public API
│   └── Cargo.toml
├── strategy_lib/          # Example strategy (Moving Average Cross)
│   ├── src/
│   │   └── lib.rs         # Compiled as cdylib → libstrategy_lib.dylib
│   └── Cargo.toml
├── Tickers/               # Market data directory (FlatBuffers .bin/.idx files)
├── Portfolios/            # Strategy configuration files (.json)
├── commission_plans.json  # Commission structure per exchange
├── instruments_info.json  # Contract metadata (margin, step, expiration)
├── LICENSE
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
Place your OHLCV data in the `Tickers/` directory as FlatBuffer `.bin` + `.idx` files.
> ✅ Generate these files using our companion tool:
>
> 🔗 [csv-to-flatbuffer](https://github.com/andydardgallard/csv-to-flatbuffer)
>
> Converts CSV/TXT files (e.g., `Si-12.23.txt`) into ultra-fast, zero-copy `.bin` + `.idx` format with resampling and indexing.
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

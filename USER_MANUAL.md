# Farukon Algo Trading Platform - User Manual

## Table of Contents

1.  [Overview](#1-overview)
2.  [Architecture](#2-architecture)
3.  [Core Components](#3-core-components)
4.  [Data Flow](#4-data-flow)
5.  [Configuration](#5-configuration)
6.  [Building and Running](#6-building-and-running)
7.  [Optimization](#7-optimization)
8.  [Extending with Strategies](#8-extending-with-strategies)
9.  [File Formats](#9-file-formats)
10. [Performance & Optimization](#10-performance--optimization)
11. [Troubleshooting](#11-troubleshooting)
12. [Glossary](#12-glossary)

---

## 1. Overview

The **Farukon Algo Trading Platform** is a high-performance, event-driven framework designed for developing, backtesting, and optimizing algorithmic trading strategies. It prioritizes speed, modularity, and flexibility.

**Key Features:**

*   **Event-Driven Architecture:** Decouples data handling, strategy logic, portfolio management, and order execution, allowing for clear separation of concerns and high performance.
*   **Ultra-Fast Data Access:** Utilizes FlatBuffers with memory mapping (`mmap`) for zero-copy data access, significantly reducing I/O overhead. Includes indexing for fast navigation and on-demand resampling.
*   **SIMD-Optimized Calculations:** Employs SIMD instructions for performance-critical operations like indicator calculations and performance metric computations.
*   **Multi-Strategy & Multi-Asset Support:** Can run multiple independent strategies simultaneously on different assets within a single backtest run.
*   **Dynamic Strategy Loading:** Strategies are compiled as separate dynamic libraries (`.so` on Linux, `.dylib` on macOS) and loaded at runtime, enabling hot-swapping of logic without recompiling the core engine. Strategy libraries are built against a versioned signal ABI (see [§8](#8-extending-with-strategies)): **v3.0.0 replaced the channel-based ABI with a host emission callback, so every strategy library must be rebuilt against the current `farukon_core`** — loading a library built for the old ABI crashes the engine.
*   **Advanced Optimization:** Includes Grid Search (exhaustive), Genetic Algorithm (evolutionary), and LSHADE-RSP optimizers for hyperparameter tuning, with runtime warnings and throttled progress output on large runs (see [§7](#7-optimization)).
*   **Risk Management:** Implements margin checking, position sizing (e.g., MPR - Maximum Possible Risk), and margin call monitoring.
*   **Modular Core:** Core logic is separated into the `farukon_core` library, making it reusable and easier to maintain.

---

## 2. Architecture

> **Design philosophy: fast and event-driven.** One FIFO pass over a unified, pre-resampled
> timeline; a new bar on any symbol triggers the strategy (`calculate_signals`), then
> SIGNAL → ORDER → FILL → portfolio update. Strategies are bar-event handlers, not batch
> transforms. Speed is a first-class constraint: zero-copy SOA data, SIMD metrics,
> per-candidate isolation, parallel optimizers — but the per-bar hot path must stay
> allocation-light, and performance work must never sacrifice event semantics.

The platform is structured as a Rust workspace containing several crates:

*   **`Farukon_2_0`:** The main application executable. Orchestrates the backtesting process, handles command-line arguments, and manages the lifecycle of other components. It receives events from a central `GlobalDataStore` via a shared event channel.
*   **`farukon_core`:** A shared library containing the core logic: event system, `DataHandler` trait definition, portfolio management, performance calculation, optimization utilities, and instrument information handling. This is the library that both `Farukon_2_0` and `strategy_lib` depend on.
*   **`strategy_lib`:** An example dynamic library containing a sample strategy implementation (e.g., Moving Average Cross). This crate is compiled into a `.so`/`.dylib` file.

### Core Concepts:

*   **Events:** Communication between the engine components (`Backtest`, `Portfolio`, `ExecutionHandler`) happens via a publish-subscribe model using an `mpsc` (multi-producer, single-consumer) channel. Events include `MARKET` (new bar), `SIGNAL` (strategy intent), `ORDER` (portfolio action), and `FILL` (execution result). Strategies are the one exception: they hold **no** channel endpoint, because no heap object and no `Sender` may cross the shared-library boundary. A strategy emits a signal as plain copyable arguments through the host's emission callback (`farukon_core::event::SignalEmitter`), and the **host** allocates the `SignalEvent` and sends it into the channel (see [§8.1](#81-understanding-the-strategy-trait)).
*   **Global Data Store (`GlobalDataStore`): A centralized component that loads all market data once** (from `.soa.bin` and `.idx` files using `mmap`), performs resampling (e.g., 1min -> 5min) on-the-fly to a unified timeline, and stores the final, aligned SOA data (`SOAData`).
*   **Data Handler:** An abstraction (`trait DataHandler`) for accessing market data. Implementations (like `SOADataHandler`) **act as consumers of the data stream generated by `GlobalDataStore`.** They hold a reference to `GlobalDataStore` and provide methods like `get_latest_bar_value` to strategies.
* **Event Stream Generation:** The `GlobalDataStore` **initiates a background task** that iterates through its **unified, pre-resampled timeline** and **sends `MARKET` events** to the shared event channel.
*   **Strategy:** Implements the `Strategy` trait, defining the `calculate_signals` logic based on market data and portfolio state.
*   **Portfolio:** Manages positions, holdings, and equity. Updates state based on `FILL` events.
*   **Execution Handler:** Simulates order execution, applying slippage and commission.

---

## 3. Core Components

This section details the main modules within `farukon_core` and `Farukon_2_0`.

### `farukon_core`

*   **`event`:** Defines the `Event` trait and concrete event types (`MarketEvent`, `SignalEvent`, `OrderEvent`, `FillEvent`). Enables type-erased communication. Also defines the FFI signal-emission contract used by strategies: `EmitSignalFn` (the host callback signature) and `SignalEmitter` (opaque host context + callback) plus the `datetime_to_nanos`/`nan_to_optional` wire conversions.
*   **`data_handler`:** Defines the `DataHandler` trait, which abstracts data source access. Implementations must provide methods to get the latest bars, values, and advance the data timeline.
*   **`strategy`:** Defines the `Strategy` trait. All user-defined strategies must implement this trait to be compatible with the platform.
*   **`portfolio`:** Defines the `PortfolioHandler` trait and related structures (`PositionState`, `HoldingsState`, `PositionSnapshot`, `HoldingSnapshot`). Manages the state and updates based on fill events. The per-bar snapshots carry an empty symbol map — only their scalar fields are read downstream.
*   **`execution`:** Defines the `ExecutionHandler` trait for simulating trade execution.
*   **`indicators`:** Contains basic technical indicators (e.g., `sma`) that strategies can use.
*   **`performance`:** Calculates performance metrics (`Total Return`, `APR`, `Max Drawdown`, `Recovery Factor`, etc.) using SIMD for speed.
*   **`optimization`:** Contains the `GridSearchOptimizer`, `GeneticAlgorythm`, and LSHADE-RSP implementations, plus `ProgressThrottle` (time-based throttling of the per-candidate progress prints).
*   **`instruments_info`:** Manages instrument metadata (margin, step, step_price, expiration, etc.) loaded from `instruments_info.json`.
*   **`commission_plans`:** Manages commission structures loaded from `commission_plans.json` and calculates fees.
*   **`index`:** Defines structures for FlatBuffer indexing (used by data handlers).
*   **`settings`:** Defines structures for parsing and holding configuration from the JSON settings file.
*   **`pos_sizers`:** Implements position sizing logic (e.g., MPR).
*   **`utils`:** Contains utility functions for parsing settings, calculating quantities, etc.

### `Farukon_2_0`

*   **`main`:** Entry point. Parses command-line arguments (`--config`) and starts the optimization process.
*   **`backtest`:** Contains the `Backtest` struct, which runs the main event loop, coordinating data updates, strategy signals, portfolio updates, and execution simulation.
*   **`data_engine/data_stream.rs`:** Contains the `GlobalDataStore` and `SOAData` structures, responsible for centralized data loading, resampling, and alignment.
*   **`data_handler.rs`:** Contains the `SOADataHandler` implementation of the DataHandler trait, which acts as a consumer of the `GlobalDataStore`'s data, providing access to `SOAData` slices/values by index.
*   **`execution`:** Contains `SimulatedExecutionHandler` which implements the `ExecutionHandler` trait.
*   **`portfolio`:** Contains `Portfolio` which implements the `PortfolioHandler` trait.
*   **`optimizers`:** Contains `OptimizationRunner` which manages the optimization process (Grid Search / Genetic Algorithm).
*   **`strategy_loader`:** Contains logic for dynamically loading strategy libraries (`.so`/`.dylib`) at runtime.

---

## 4. Data Flow

1.  **Initialization:**
    *   `Farukon_2_0` loads settings from the JSON file.
    *   A `GlobalDataStore` is created. **It loads raw data** for all symbols, performs resampling to the target timeframe, **aligns data to a common timeline**, and **stores the final** `SOAData` for each symbol.
    *   The `GlobalDataStore` **starts a background task** that iterates through its **unified, pre-resampled timeline** and sends `MARKET` events to a **shared event channel**.
    *   A `Portfolio` is created with initial capital.
    *   An `ExecutionHandler` is created.
    *   A `Strategy` is loaded dynamically.
    *   An `SOADataHandler` is created, receiving a **reference to the shared** `GlobalDataStore`.
    *   The **shared event channel** is used for communication between components.

2.  **Backtesting Loop (`Backtest::run_backtest`):**
    *   `Backtest` **waits for events** from the **shared event receiver**.
    *   When a `MARKET` event arrives (generated by the `GlobalDataStore`'s background task):
        *   `Portfolio` updates its time-indexed state based on the **current state of the `GlobalDataStore`** (e.g., by asking `SOADataHandler` for data at the current index).
        *   `Strategy::calculate_signals` is called, **receiving a reference to the `SOADataHandler`** (`&dyn DataHandler`). The strategy accesses the **current market data** from the `GlobalDataStore` via the handler (e.g., `get_latest_bar_value`, `get_latest_bar_datetime`) and may generate `SIGNAL` events.
        *   `SIGNAL` events lead to `ORDER` events sent by `Portfolio`.
        *   `ExecutionHandler` receives `ORDER` events and simulates execution, sending `FILL` events.
        *   `Portfolio` receives `FILL` events and updates positions/holdings.
    *   Other events (`SIGNAL`, `ORDER`, `FILL`) are processed as usual via the shared channel.

3.  **Finalization:**
    *   After the loop, `Portfolio::calculate_final_performance()` is called to compute final metrics using the full equity curve.
    *   Results are output.

---

## 5. Configuration

The platform is configured using a single JSON file passed via the `--config` command-line argument.

### Top-Level Structure

```json
{
  "common": { ... },
  "portfolio": { ... }
}
```

*   **`common` (Object):** Global settings.
    *   **`mode`** (String): `"Debug"`, `"Optimize"`, `"Visual"`, or `"Portfolio"`. Controls verbosity and behavior (in `"Portfolio"` mode only the `Grid_Search` optimizer is allowed).
    *   **`initial_capital`** (float): Starting capital for the entire portfolio.
    *   **`global_data_storage_mode`** (String): `"arc"` or `"deep"`. In `"arc"` mode every candidate shares one `Arc`-held dataset; in `"deep"` mode every candidate **deep-clones the whole dataset** before its backtest. `"deep"` is valid but very expensive: the process prints a prominent startup warning recommending `"arc"` (see [§7](#7-optimization)).

*   **`portfolio` (Object):** A map where keys are unique strategy IDs (e.g., `"Strategy_1"`), and values are strategy-specific configurations.

### Strategy Configuration (`portfolio.<strategy_id>`)

```json
{
  "threads": 8,
  "strategy_name": "MovingAverageCrossStrategy",
  "strategy_path": "target/release/libstrategy_lib.dylib", // Path to .so/.dylib
  "strategy_weight": 1.0, // Proportion of capital allocated
  "slippage": [0.005], // Can be a range: {"start": 0.001, "end": 0.01, "step": 0.001}
  "data": {
    "data_path": "Tickers/FBS/Si", // Path to .bin/.idx files
    "timeframe": "4min" // Target timeframe (1min, 2min, ... 5min, 1d)
  },
  "symbol_base_name": "Si", // Base name for lookup in instruments_info.json
  "symbols": ["Si-12.23", "Si-3.24"], // Specific contracts to trade
  "strategy_params": { // Parameters for the strategy
    "short_window": [50], // Can be a range: {"start": 50, "end": 100, "step": 10}
    "long_window": [100]
  },
  "pos_sizer_params": {
    "pos_sizer_name": "mpr",
    "pos_sizer_params": {},
    "pos_sizer_value": [1.5] // Can be a range: {"start": 1.0, "end": 2.0, "step": 0.5}
  },
  "margin_params": {
    "min_margin": 0.5, // Minimum equity as fraction of initial_capital
    "margin_call_type": "close_deal"
  },
  "portfolio_settings_for_strategy": {
    "metrics_calculation_mode": "offline" // "offline" or "realtime"
  },
  "optimizer_type": "Grid_Search" // or { "Genetic": { "ga_params": { ... } } }
}
```

### `ga_params` (for Genetic Algorithm)

```json
{
  "population_size": 100,
  "p_crossover": 0.8,
  "p_mutation": 0.1,
  "max_generations": 10,
  "fitness_params": {
    "fitness_direction": "max", // "max" or "min"
    "fitness_value": "APR/DD_factor" // or "TotalReturn", "RecoveryFactor", "Composite", etc.
  }
}
```

### `instruments_info.json`

Defines metadata for all available instruments. Example structure:

```json
{
  "Si": {
    "Si-12.23": {
      "exchange": "FORTS",
      "type": "futures",
      "contract_precision": 0,
      "margin": 13965.5,
      "commission_type": "currency",
      "trade_from_date": "2023-09-21 09:00:00",
      "expiration_date": "2024-12-20 09:00:00",
      "marginal_costs": 0,
      "step": 1,
      "step_price": 1
    },
    // ... other Si contracts
  },
  "RTS": { // ... other instrument types
  }
}
```

### `commission_plans.json`

Defines commission structures per exchange and type. Example structure:

```json
{
  "FORTS": {
    "currency": 0.5, // Commission per contract for currency-type instruments
    "index": 1.0,    // Commission per contract for index-type instruments
    "percent": 0.01  // Commission as percentage of trade value
  }
}
```

---

## 6. Building and Running

1.  **Prerequisites:**
    *   Install [Rust](https://www.rust-lang.org/tools/install) (edition 2024).
    *   Ensure `cargo` is in your PATH.

2.  **Clone the Repository:**

    ```bash
    git clone https://github.com/andydardgallard/FarukonAlgoTradingPlatform.git
    cd FarukonAlgoTradingPlatform
    ```

3.  **Build the Project:**

    ```bash
    # Build all crates in the workspace
    cargo build --release
    ```

    This will create:
    *   The main executable: `./target/release/Farukon_2_0`
    *   The strategy library: `./target/release/libstrategy_lib.dylib` (or `.so`)

    > **Rebuild every strategy library (breaking ABI, v3.0.0).** A strategy library must be built
    > against the same signal ABI as the engine it is loaded by. v3.0.0 replaced the channel-based
    > strategy ABI (`create_strategy` taking an `mpsc::Sender`) with a host emission callback, and
    > the host now links `mimalloc`. A library built for the old ABI **crashes** the new binary
    > (allocator mismatch on the event object). After updating the engine, rebuild the sample
    > library (`cargo build --release -p strategy_lib`) and every external strategy `cdylib`
    > (see [§8.3](#83-creating-your-own-strategy)).

4.  **Prepare Data:**
    *   Place your market data in the `Tickers/` directory.
    *   Data must be in FlatBuffers format (`.soa.bin` files) with corresponding index files (`.soa.idx`).
    *   Use the companion tool [csv-to-flatbuffer](https://github.com/andydardgallard/csv-to-flatbuffer) to convert your CSV/TXT OHLCV data into the required `.soa.bin`/`.soa.idx` format.

5.  **Prepare Configuration:**
    *   Create or modify your configuration JSON file (e.g., `Portfolios/Debug_Portfolio.json`).
    *   Ensure `strategy_path` points to the compiled strategy library (e.g., `target/release/libstrategy_lib.dylib`).
    *   Ensure `data_path` in the config points to the directory containing your `.soa.bin`/`.soa.idx` files.
    *   Ensure `symbols` in the config match entries in `instruments_info.json`.

6.  **Run the Backtester:**

    ```bash
    # Run with a specific configuration file
    cargo run --release -- --config Portfolios/Debug_Portfolio.json
    # Or directly execute the binary
    # ./target/release/Farukon_2_0 --config Portfolios/Debug_Portfolio.json
    ```

---

## 7. Optimization

The platform supports three optimization methods:

### Grid Search

*   **Purpose:** Exhaustively tests all combinations of specified parameter values.
*   **Configuration:** Set `"optimizer_type"` to `"Grid_Search"` in your JSON config.
*   **Usage:** Define parameter ranges in `strategy_params`, `pos_sizer_value`, and `slippage` using arrays or range objects (e.g., `{"start": 1, "end": 10, "step": 1}`).
*   **Execution:** The `OptimizationRunner` will run a full backtest for each combination in parallel.

### Genetic Algorithm (GA)

*   **Purpose:** Evolves a population of parameter sets over generations to find optimal values.
*   **Configuration:** Set `"optimizer_type"` to `{ "Genetic": { "ga_params": { ... } } }`.
*   **Usage:** Define `ga_params` (population size, mutation rate, crossover rate, generations) and the fitness metric in the JSON config.
*   **Execution:** The `OptimizationRunner` will run the GA, evaluating parameter sets via backtests.

### LSHADE-RSP

*   **Purpose:** Adaptive Differential Evolution (SHADE) with Linear Population Size Reduction and rank-based selective pressure.
*   **Configuration:** Set `"optimizer_type"` to `{ "LSHADE_RSP": { "lshade_params": { ... } } }`.
*   **Usage:** Define `lshade_params` (population size, max evaluations, `p_best`, `archive_rate`, `memory_size`) and `fitness_params`. The `fitness_direction` (`"max"` / `"min"`) is honoured by selection, by the best/worst statistics and by the convergence check.
*   **Execution:** The optimizer evaluates the population in parallel and appends one statistics row per iteration (iterations numbered from 1) to `lshade_optimization_results.csv` in the strategy's `exit_results_path`; the file is recreated with its header at the start of each run.

All optimizers write the evaluated parameter sets and their metrics to `optimization_results.csv`. The metric columns include `Max_Drawdown_DateTime` (timestamp of the relative maximum-drawdown trough, formatted as `YYYY-MM-DD HH:MM:SS`) alongside `Max_Drawdown`. Grid Search, Genetic Algorithm, and LSHADE-RSP all emit this column, and it is populated for **both** metrics modes (`offline` and `realtime`): the final calculation always runs over the full equity curve, which is the only source of the drawdown timestamp. It is empty only when no drawdown exists.

### Runtime Warnings and Progress Output

*   **Deep data storage warning:** when `common.global_data_storage_mode` is `"deep"`, the process prints a one-time `WARNING: global_data_storage_mode = "deep"` block at startup (once per process, never per candidate) explaining that every candidate deep-clones the dataset and recommending `"arc"`. The run continues.
*   **Thread-scaling knee warning:** Grid Search, Genetic Algorithm, and LSHADE-RSP each print a one-time warning when the configured `threads` exceed 8 **and** the combined timeline reaches 100 000 bars per candidate (a "high bar-count dataset"). The warning states the measured scaling knee (~8 threads on 1-3 minute timeframes, where runtime stops improving and may degrade); it is guidance only — **no cap is applied**, the run continues with the configured thread count.
*   **Throttled progress output:** the per-candidate prints (`# N from M <params>` and `# N from M is done in X seconds`) are throttled to at most one candidate roughly every 2 seconds per evaluation batch. The last candidate of a batch always prints, so the final state of every generation/iteration stays visible. The printed text itself is unchanged.

---

Конечно. Ниже приведён обновлённый раздел **User Manual**, включающий **детальный разбор примера стратегии пересечения средних** (`MovingAverageCrossStrategy`) и **руководство по созданию новой стратегии**.

---

## 8. Extending with Strategies

To create a new trading strategy, you need to implement the `Strategy` trait defined in `farukon_core` and compile it as a dynamic library (`.so` on Linux, `.dylib` on macOS) that the main platform can load at runtime.

> **Breaking change (v3.0.0) — the strategy ABI changed.** `create_strategy` no longer receives an
> `mpsc::Sender<Box<dyn Event>>`. The host now hands over an **emission pair** —
> `emitter_ctx: *const c_void` and `emit_signal_cb: EmitSignalFn` — and the strategy stores
> `farukon_core::event::SignalEmitter { ctx, cb }`. Signals cross the boundary as plain copyable
> values (nanosecond timestamp, C strings, `f64` with `NaN` for absent options) and the **host**
> allocates the `SignalEvent`. Every strategy library must therefore be **rebuilt** against the
> current `farukon_core`: an old library passes an `mpsc::Sender` where the host now expects an
> opaque context, and since the host links `mimalloc` (the old library used the system allocator)
> mixing the two allocators for one event object crashes the process. The full rationale and the
> exact contract: `farukon_core/src/event.rs`, section "SIGNAL EMISSION ACROSS THE FFI BOUNDARY";
> `strategy_lib/src/lib.rs` is the reference implementation.

### 8.1 Understanding the `Strategy` Trait

The core of any strategy is the `Strategy` trait defined in `farukon_core/src/strategy.rs`:

```rust
// farukon_core/src/strategy.rs

use crate::event;
use crate::portfolio;
use crate::data_handler;

pub trait Strategy {
    /// Calculates signals based on market data and current portfolio state.
    /// # Arguments
    /// * `data_handler` - Interface to market data.
    /// * `current_positions` - Current positions for all symbols.
    /// * `all_holdings` - Holdings snapshots (capital, cash, blocked) used for position sizing.
    /// * `symbol_list` - List of symbols to trade.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn calculate_signals(
        &mut self,
        data_handler: &dyn data_handler::DataHandler,
        current_positions: &std::collections::HashMap<String, portfolio::PositionState>,
        all_holdings: &[portfolio::HoldingSnapshot],
        symbol_list: &[String],
    ) -> anyhow::Result<()>;

    /// Opens a position by sending a limit order.
    /// # Arguments
    /// * `emitter` - Signal emitter of the host this strategy runs in.
    /// * `current_bar_datetime` - Current bar datetime.
    /// * `symbol` - Symbol to trade.
    /// * `signal_name` - Signal name (e.g., "LONG", "SHORT").
    /// * `quantity` - Quantity to trade (optional, can be determined by position sizer).
    /// * `limit_price` - The limit price for the order.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn open_by_limit(
        &self,
        emitter: &event::SignalEmitter,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &str,
        signal_name: &str,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    ) -> anyhow::Result<()> {
        emitter.emit(current_bar_datetime, symbol, signal_name, "LMT", quantity, limit_price)
    }

    /// Opens a position by sending a market order. The order kind is "MKT".
    fn open_by_market(
        &self,
        emitter: &event::SignalEmitter,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &str,
        signal_name: &str,
        quantity: Option<f64>,
    ) -> anyhow::Result<()> {
        emitter.emit(current_bar_datetime, symbol, signal_name, "MKT", quantity, None)
    }

    /// Closes a position by sending a market order.
    fn close_by_market(
        &self,
        emitter: &event::SignalEmitter,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &str,
        signal_name: &str,
        quantity: Option<f64>,
    ) -> anyhow::Result<()> {
        emitter.emit(current_bar_datetime, symbol, signal_name, "MKT", quantity, None)
    }

    /// Closes a position by sending a limit order. The order kind is "LMT".
    fn close_by_limit(
        &self,
        emitter: &event::SignalEmitter,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &str,
        signal_name: &str,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    ) -> anyhow::Result<()> {
        emitter.emit(current_bar_datetime, symbol, signal_name, "LMT", quantity, limit_price)
    }
}
```

**Key Points:**

*   **`calculate_signals`**: This is your **main strategy function**. It's called by the backtester every time a new market bar arrives for *any* of the symbols in your `symbol_list`. You access market data, check your portfolio state, and decide whether to generate buy/sell/exit signals.
*   **Helper Functions (`open_by_*`, `close_by_*`)**: These are **default trait methods** — a strategy normally does not override them. Each one marshals its arguments into the host callback carried by `emitter` (`SignalEmitter::emit`), and the host allocates the `SignalEvent` and sends it into the engine's event channel. The `Portfolio` module receives these signals, processes them (e.g., checks margin, calculates quantity using position sizer), and creates `OrderEvent`s which are sent to the `ExecutionHandler`.
*   **`emitter`**: The host-owned `SignalEmitter { ctx, cb }`. `ctx` is opaque host state (the host's event sender) and `cb` the host callback; the library only stores and passes them back, and it never allocates, clones or drops host state. A callback return code other than `0` is mapped to an `anyhow::Error` ("Signal emission callback failed with code: ...").
*   **`data_handler`**: Provides methods like `get_latest_bar_value(symbol, "close")`, `get_latest_bars_values(symbol, "close", n)`, etc., to access market data.
*   **`current_positions`**: A map of symbol names to `PositionState` structs, allowing you to check if you are currently long, short, or flat on a symbol, and the size of the position.
*   **`all_holdings`**: The holdings snapshots (first and latest) of the portfolio; they carry `capital`, `cash` and `blocked`, and are what the position sizer receives.

### 8.2 Detailed Analysis: `MovingAverageCrossStrategy`

Let's examine the provided `strategy_lib/src/lib.rs` which implements the `MovingAverageCrossStrategy`.

#### 8.2.1 Structure and Initialization

```rust
// strategy_lib/src/lib.rs

use farukon_core::{self, strategy::Strategy}; // Import the core library and Strategy trait

// mimalloc global allocator: matches the Farukon_2 host binary. No heap object crosses the FFI
// boundary any more (signals go through the host callback in `farukon_core::event`), so this
// allocator only serves allocations made and freed inside this library.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// The main strategy struct holds its configuration and state.
pub struct MovingAverageCrossStrategy {
    mode: String, // e.g., "Debug", "Optimize"
    strategy_settings: farukon_core::settings::StrategySettings, // Configuration loaded from JSON
    strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>, // Metadata for traded symbols

    // Pre-parsed, immutable per-symbol instrument dates: (expiration_date, trade_from_date).
    // Parsed once in `new()` so the per-bar signal loop never re-parses metadata.
    parsed_symbol_dates: std::collections::HashMap<
        String,
        (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>),
    >,

    // The signal emitter of the host that loaded this strategy. Both its fields point at host
    // state that outlives the strategy; this library never owns or clones it.
    emitter: farukon_core::event::SignalEmitter,

    short_window: usize, // Length of the short-term SMA (e.g., 50)
    long_window: usize,  // Length of the long-term SMA (e.g., 100)
}

impl MovingAverageCrossStrategy {
    pub fn new(
        mode: String,
        strategy_settings: farukon_core::settings::StrategySettings,
        strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        emitter: farukon_core::event::SignalEmitter,
    ) -> anyhow::Result<Self> {
        // Extract required parameters (short_window, long_window) from strategy_settings.strategy_params
        // (via farukon_core::utils::get_param_as_usize).
        let short_window = utils::get_param_as_usize(&strategy_settings.strategy_params, "short_window")?;
        let long_window = utils::get_param_as_usize(&strategy_settings.strategy_params, "long_window")?;

        // Validate parameters: short window must be less than long window
        if short_window >= long_window {
            anyhow::bail!("'short_window' ({}) must be less than 'long_window' ({}).", short_window, long_window);
        }

        // Pre-parse the immutable per-symbol instrument dates once (hot path must not re-parse
        // metadata every bar). A malformed date fails here, exactly like the old per-bar `?`.
        let mut parsed_symbol_dates =
            std::collections::HashMap::with_capacity(strategy_instruments_info.len());
        for (symbol, instrument_info) in &strategy_instruments_info {
            let expiration_date_dt =
                utils::string_to_date_time(&instrument_info.expiration_date, "%Y-%m-%d %H:%M:%S")
                    .map_err(|err| err.context(format!(
                        "Failed to parse 'expiration_date' for symbol '{}'", symbol)))?;
            let trade_from_date_dt =
                utils::string_to_date_time(&instrument_info.trade_from_date, "%Y-%m-%d %H:%M:%S")
                    .map_err(|err| err.context(format!(
                        "Failed to parse 'trade_from_date' for symbol '{}'", symbol)))?;
            parsed_symbol_dates.insert(symbol.clone(), (expiration_date_dt, trade_from_date_dt));
        }

        // Create and return the strategy instance
        anyhow::Ok(MovingAverageCrossStrategy {
            mode,
            strategy_settings,
            strategy_instruments_info,
            parsed_symbol_dates,
            short_window,
            long_window,
            emitter,
        })
    }
}
```

*   **`new`**: This constructor is called by the dynamic loading mechanism (in `strategy_loader.rs`) when the library is loaded. It receives the `mode`, parsed `strategy_settings` (from the JSON config), `strategy_instruments_info` (parsed from `instruments_info.json`), and the `SignalEmitter` — the host's `(ctx, callback)` emission pair.
*   **Parameter Parsing**: It extracts `short_window` and `long_window` from the `strategy_params` map within `strategy_settings` (via `farukon_core::utils::get_param_as_usize`). This map comes directly from the `strategy_params` section in your JSON config file.
*   **Validation**: It performs a simple validation to ensure `short_window < long_window`.
*   **Date Pre-Parsing**: It parses every symbol's `expiration_date` and `trade_from_date` once into `parsed_symbol_dates`, so the per-bar loop never re-parses this immutable metadata (a malformed date fails at construction time).
*   **State Storage**: The parsed parameters, the memoized dates, the emitter and other necessary data are stored in the struct instance.

#### 8.2.2 Core Logic: `calculate_signals`

```rust
// ... inside impl Strategy for MovingAverageCrossStrategy

fn calculate_signals(
        &mut self, // Mutable reference to self to allow state changes if needed
        data_handler: &dyn farukon_core::data_handler::DataHandler, // Access to market data
        current_positions: &std::collections::HashMap<String, farukon_core::portfolio::PositionState>, // Current portfolio state
        all_holdings: &[farukon_core::portfolio::HoldingSnapshot], // Holdings snapshots used by the position sizer
        symbol_list: &[String], // List of symbols to trade
) -> anyhow::Result<()> {
    // Iterate through each symbol in the configured list
    for symbol in symbol_list {
        // Get instrument info for the symbol
        let strategy_instruments_info_for_symbol = self.strategy_instruments_info.get(symbol).unwrap();

        // Get the current datetime and close price for the symbol
        let current_bar_datetime = data_handler.get_latest_bar_datetime(symbol).unwrap();
        let close = Some(data_handler.get_latest_bar_value(symbol, "close").unwrap());

        // Get the pre-parsed (expiration, trade_from) datetimes memoized in `new()`.
        // A missing entry means an unknown symbol; fail like the old parse error path did.
        let (expiration_date_dt, trade_from_date_dt) =
            *self.parsed_symbol_dates.get(symbol).ok_or_else(|| {
                anyhow::anyhow!("No pre-parsed instrument dates for symbol '{}'", symbol)
            })?;

        // Get current position state for the symbol
        let current_position_state = current_positions.get(symbol).unwrap();
        let current_position_quantity = current_position_state.position;

        // Fetch the windows and calculate the short and long SMAs over the close series
        let short_sma_bars = data_handler
            .get_latest_bars_values(symbol, "close", self.short_window)
            .unwrap();
        let long_sma_bars = data_handler
            .get_latest_bars_values(symbol, "close", self.long_window)
            .unwrap();

        if let (Some(short_sma), Some(long_sma)) = (
            farukon_core::indicators::sma(short_sma_bars, self.short_window),
            farukon_core::indicators::sma(long_sma_bars, self.long_window),
        ) {
            // Debug logging
            if self.mode == "Debug" {
                println!("Start event, Indicators, {}, {}, short_sma: {}, long_sma: {}, current_position: {}",
                    symbol, current_bar_datetime, short_sma, long_sma, current_position_quantity);
                println!("Start event, Indicators + equity_point, {:?}", all_holdings);
            }

            // --- Check for EXIT conditions first ---
            if current_position_quantity != 0.0 {
                let signal_name = "EXIT";
                // Check if a long position exists
                if current_position_quantity > 0.0 {
                    // EXIT LONG on SMA crossover or on contract expiration
                    if short_sma < long_sma || current_bar_datetime >= expiration_date_dt {
                        self.close_by_market(
                            &self.emitter,
                            current_bar_datetime,
                            symbol,
                            signal_name,
                            Some(current_position_quantity), // Close the full long position
                        )?;
                    }
                }
                // Check if a short position exists
                else {
                    // EXIT SHORT on SMA crossover or on contract expiration
                    if short_sma > long_sma || current_bar_datetime >= expiration_date_dt {
                        self.close_by_market(
                            &self.emitter,
                            current_bar_datetime,
                            symbol,
                            signal_name,
                            Some(current_position_quantity), // Close the full short position (quantity is negative)
                        )?;
                    }
                }
            }
            // --- Check for ENTRY conditions if no position exists ---
            else {
                // LONG: Check for crossover and validity period
                if short_sma > long_sma
                    && current_bar_datetime < expiration_date_dt // Must be before expiration
                    && current_bar_datetime >= trade_from_date_dt // Must be after trade start date
                {
                    let signal_name = "LONG";
                    // Calculate position size using the configured position sizer
                    let quantity = farukon_core::pos_sizers::get_pos_sizer_from_settings(
                        &self.mode,
                        all_holdings,
                        close,
                        Some(long_sma), // Pass long_sma as a parameter to the sizer
                        &self.strategy_settings,
                        strategy_instruments_info_for_symbol,
                    );

                    // Send a LIMIT order signal to open a long position
                    self.open_by_limit(
                        &self.emitter,
                        current_bar_datetime,
                        symbol,
                        signal_name,
                        quantity,
                        close, // Use current close as the limit price
                    )?;

                    if self.mode == "Debug" {
                        println!("quantity: {:?}", quantity);
                    }
                }
                // SHORT: Check for crossover and validity period
                else if short_sma < long_sma
                    && current_bar_datetime < expiration_date_dt // Must be before expiration
                    && current_bar_datetime >= trade_from_date_dt // Must be after trade start date
                {
                    let signal_name = "SHORT";
                    // Calculate position size using the configured position sizer
                    let quantity = farukon_core::pos_sizers::get_pos_sizer_from_settings(
                        &self.mode,
                        all_holdings,
                        close,
                        Some(long_sma), // Pass long_sma as a parameter to the sizer
                        &self.strategy_settings,
                        strategy_instruments_info_for_symbol,
                    );

                    // Send a LIMIT order signal to open a short position
                    self.open_by_limit(
                        &self.emitter,
                        current_bar_datetime,
                        symbol,
                        signal_name,
                        quantity,
                        close, // Use current close as the limit price
                    )?;

                    if self.mode == "Debug" {
                        println!("quantity: {:?}", quantity);
                    }
                }
            }

            // Debug logging
            if self.mode == "Debug" {
                println!("Finish event, Indicators, {}, {}, short_sma: {}, long_sma: {}, current_position: {}",
                    symbol, current_bar_datetime, short_sma, long_sma, current_position_quantity);
                println!("Finish event, Indicators + equity_point, {:?}", all_holdings);
            }
        }
        // If SMAs could not be calculated (e.g., insufficient data), do nothing for this bar/symbol.
    }

    // Return Ok to indicate successful signal calculation for this iteration
    anyhow::Ok(())
}
```

*   **Iteration**: It loops through each symbol in the `symbol_list` (e.g., `["Si-12.23", "Si-3.24"]`).
*   **Data Access**: It retrieves the current datetime, close price, the pre-parsed expiration/trade-from datetimes (from `self.parsed_symbol_dates`, filled once in `new()`), and the current position quantity for the symbol using the `data_handler` and `current_positions` map.
*   **Indicator Calculation**: It fetches the close series with `get_latest_bars_values(symbol, "close", window)` and passes the slice to `farukon_core::indicators::sma(bars, window)` for both `self.short_window` and `self.long_window`.
*   **Logic Flow**:
    1.  **Exit Check**: If a position exists (`current_position_quantity != 0.0`), it checks for exit conditions:
        *   **Long Exit**: If short SMA < long SMA (bearish crossover) OR expiration date reached.
        *   **Short Exit**: If short SMA > long SMA (bullish crossover) OR expiration date reached.
        *   If an exit condition is met, it calls `self.close_by_market(...)` to send an "EXIT" signal.
    2.  **Entry Check**: If no position exists (`current_position_quantity == 0.0`), it checks for entry conditions:
        *   **Long Entry**: If short SMA > long SMA (bullish crossover) AND within the valid trading period (before expiration, after trade start).
        *   **Short Entry**: If short SMA < long SMA (bearish crossover) AND within the valid trading period.
        *   If an entry condition is met, it calculates the position size using `farukon_core::pos_sizers::get_pos_sizer_from_settings` based on the strategy's configuration (e.g., "mpr", value 1.5) and on `all_holdings` (the capital/cash/blocked snapshot). Then, it calls `self.open_by_limit(...)` to send a "LONG" or "SHORT" signal with the calculated quantity.
*   **Signal Sending**: The helper functions `open_by_limit`, `close_by_market`, etc., are the trait's default methods: they marshal their arguments through `self.emitter` (the host callback), and the **host** allocates the `SignalEvent` and puts it into the event channel. The `Portfolio` module receives these signals and handles the order creation and execution simulation.
*   **Hot path**: the strategy resolves nothing per bar that it could resolve once — the FFI entry point and the symbol list are cached by the loader per strategy instance, and the instrument dates are memoized in `parsed_symbol_dates` at construction time.

Конечно, вот обновлённый раздел 8.3 "Creating Your Own Strategy", переписанный с акцентом на то, что пользователю в большинстве случаев нужно изменять **только** функцию `calculate_signals`.

---

### 8.3 Creating Your Own Strategy

To create a new trading strategy for the Farukon platform, you implement the `Strategy` trait in a separate Rust library that gets dynamically loaded by the main application.

**The key insight is that for most custom strategies, you will primarily focus on writing the logic inside the `calculate_signals` function.** The other parts (structure, initialization, helper functions for sending signals, and the C FFI interface) often follow a standard pattern and can be reused or adapted from the provided `MovingAverageCrossStrategy` example.

#### 8.3.1 Step-by-Step Guide

1.  **Create a New Rust Crate:**
    *   Create a new directory for your strategy (e.g., `my_new_strategy`).
    *   Initialize it as a library crate: `cargo new my_new_strategy --lib`.
    *   Navigate into the new directory: `cd my_new_strategy`.

2.  **Configure `Cargo.toml`:**
    *   Edit the generated `Cargo.toml` file in your strategy's directory.
    *   Add `farukon_core` as a dependency, pointing to the location of the core library in your workspace.
    *   Add `mimalloc` and register it as the `#[global_allocator]` of the library (as `strategy_lib` does). Host and library have separate allocators; matching the host's allocator keeps the library's own allocations cheap, and no heap object is exchanged with the host.
    *   Crucially, set the crate type to `cdylib` so it compiles into a dynamic library (`.so` on Linux, `.dylib` on macOS) that can be loaded by the main application.

    **Example `Cargo.toml`:**

    ```toml
    [package]
    name = "my_new_strategy"
    version = "0.1.0"
    edition = "2024"

    [lib]
    # This tells Cargo to build a dynamic library (.so/.dylib)
    crate-type = ["cdylib"]

    [dependencies]
    # Link to the Farukon core library
    farukon_core = { path = "../farukon_core" } # Adjust path as needed
    # Add other libraries you might need (e.g., for complex math, indicators)
    anyhow = "1.0"
    chrono = "0.4"
    # Same allocator as the Farukon_2 host binary
    mimalloc = "0.1"
    ```

3.  **Implement Your Strategy in `src/lib.rs`:**
    *   Replace the contents of the generated `src/lib.rs` file.
    *   **Define Your Strategy Struct:** This struct holds the state and configuration for your strategy instance, including the host's `SignalEmitter`.
    *   **Implement the `new` Constructor:** This function is called when the library is loaded. It receives initial configuration (mode, settings, instrument info, the `SignalEmitter` emission pair) and should parse any strategy-specific parameters from `strategy_settings.strategy_params`. Parse immutable metadata (e.g., expiration dates) **once here**, never per bar.
    *   **Implement the `Strategy` Trait:** This is the core.
        *   **`calculate_signals` (Your Focus):** This function is called on every market bar update. Here, you access market data (`data_handler`), check your current portfolio state (`current_positions`, `all_holdings`), apply your trading logic, and emit signals (`open_by_*`, `close_by_*`).
        *   **Helper Functions (`open_by_*`, `close_by_*`):** These are default trait methods — do **not** re-implement them by sending on a channel. Call them with `&self.emitter`; they marshal the arguments through the host callback, and the host allocates the `SignalEvent`.

    **Example Skeleton:**

    ```rust
    // my_new_strategy/src/lib.rs

    // Same allocator as the Farukon_2 host binary; only this library's own allocations use it.
    #[global_allocator]
    static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

    use farukon_core::{self, strategy::Strategy};

    // --- 1. Define Your Strategy Struct ---
    pub struct MyNewStrategy {
        mode: String,
        strategy_settings: farukon_core::settings::StrategySettings,
        strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        // Host-owned emission pair: stored for the whole strategy lifetime, never cloned or dropped.
        emitter: farukon_core::event::SignalEmitter,
        // Add any specific state variables your strategy needs here
        my_param1: f64,
        my_param2: usize,
        // Example: for storing indicator values across bars
        // my_indicator_cache: std::collections::HashMap<String, Vec<f64>>,
    }

    // --- 2. Implement Constructor ---
    impl MyNewStrategy {
        pub fn new(
            mode: String,
            strategy_settings: farukon_core::settings::StrategySettings,
            strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
            emitter: farukon_core::event::SignalEmitter,
        ) -> anyhow::Result<Self> {
            // Example: Extract parameters from JSON config
            fn get_param_as_f64(params: &std::collections::HashMap<String, Vec<serde_json::Value>>, name: &str) -> anyhow::Result<f64> {
                 let value = params
                    .get(name)
                    .and_then(|v| v.first())
                    .ok_or_else(|| anyhow::anyhow!("Missing parameter '{}'", name))?;

                if let Some(val) = value.as_f64() {
                    Ok(val)
                } else {
                    Err(anyhow::anyhow!("Parameter '{}' must be a number, got: {:?}", name, value))
                }
            }

             fn get_param_as_usize(params: &std::collections::HashMap<String, Vec<serde_json::Value>>, name: &str) -> anyhow::Result<usize> {
                 let value = params
                    .get(name)
                    .and_then(|v| v.first())
                    .ok_or_else(|| anyhow::anyhow!("Missing parameter '{}'", name))?;

                if let Some(val) = value.as_u64() {
                    Ok(val as usize)
                } else if let Some(val) = value.as_f64() {
                    Ok(val as usize)
                } else {
                    Err(anyhow::anyhow!("Parameter '{}' must be a number, got: {:?}", name, value))
                }
            }

            let my_param1 = get_param_as_f64(&strategy_settings.strategy_params, "my_param1")?;
            let my_param2 = get_param_as_usize(&strategy_settings.strategy_params, "my_param2")?;

            // Validate parameters if necessary
            if my_param2 == 0 {
                anyhow::bail!("'my_param2' must be greater than 0.");
            }

            Ok(MyNewStrategy {
                mode,
                strategy_settings,
                strategy_instruments_info,
                emitter,
                my_param1,
                my_param2,
                // my_indicator_cache: std::collections::HashMap::new(), // Initialize state if needed
            })
        }
    }

    // --- 3. Implement the Strategy Trait ---
    impl farukon_core::strategy::Strategy for MyNewStrategy {
        // *** THIS IS THE MAIN LOGIC YOU WILL WRITE ***
        fn calculate_signals(
                &mut self, // Mutable to potentially update internal state (e.g., indicator cache)
                data_handler: &dyn farukon_core::data_handler::DataHandler,
                current_positions: &std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
                all_holdings: &[farukon_core::portfolio::HoldingSnapshot],
                symbol_list: &[String],
        ) -> anyhow::Result<()> {
            // Iterate through all symbols this strategy trades
            for symbol in symbol_list {
                // Example: Get current market data
                let current_datetime = data_handler.get_latest_bar_datetime(symbol).unwrap();
                let current_close = data_handler.get_latest_bar_value(symbol, "close").unwrap();
                let current_high = data_handler.get_latest_bar_value(symbol, "high").unwrap();
                let current_low = data_handler.get_latest_bar_value(symbol, "low").unwrap();

                // Example: Get current position for this symbol
                let current_position_state = current_positions.get(symbol).unwrap();
                let current_position_quantity = current_position_state.position;

                // Example: Get instrument info (e.g., expiration)
                let instrument_info = self.strategy_instruments_info.get(symbol).unwrap();
                let expiration_date_dt = farukon_core::utils::string_to_date_time(
                    &instrument_info.expiration_date, "%Y-%m-%d %H:%M:%S"
                )?;

                // --- YOUR TRADING LOGIC GOES HERE ---
                // Example: Simple RSI-based logic (assuming you have an RSI indicator)
                // let rsi_value = calculate_rsi(data_handler, symbol, "close", 14, 0);

                // Example: Simple breakout logic
                let recent_highs = data_handler.get_latest_bars_values(symbol, "high", self.my_param2); // Get last N highs
                if let Some(max_recent_high) = recent_highs.iter().cloned().fold(None, |acc, x| Some(acc.map_or(x, |y| y.max(x)))) {
                    if current_close > max_recent_high && current_position_quantity == 0.0 {
                        // Condition met to enter a LONG position
                        let signal_name = "LONG";
                        // Calculate quantity using position sizer
                        let quantity = farukon_core::pos_sizers::get_pos_sizer_from_settings(
                            &self.mode,
                            all_holdings,
                            Some(current_close),
                            None, // Pass any relevant value for position sizing, e.g., long SMA
                            &self.strategy_settings,
                            instrument_info,
                        );

                        // Send a signal to open a long position
                        self.open_by_market(
                            &self.emitter,
                            current_datetime,
                            symbol,
                            signal_name,
                            quantity,
                        )?;

                        if self.mode == "Debug" {
                            println!("Generated LONG signal for {} at {}", symbol, current_close);
                        }
                    }
                }

                // Example: Exit logic (e.g., if position is long and close is below a threshold)
                if current_position_quantity > 0.0 {
                    let exit_threshold = current_position_state.entry_price.unwrap_or(0.0) - self.my_param1; // Example: exit if price drops by my_param1 from entry
                    if current_close < exit_threshold {
                         // Condition met to exit a LONG position
                        let signal_name = "EXIT";
                        self.close_by_market(
                            &self.emitter,
                            current_datetime,
                            symbol,
                            signal_name,
                            Some(current_position_quantity), // Close the current position size
                        )?;

                        if self.mode == "Debug" {
                            println!("Generated EXIT (LONG) signal for {} at {}", symbol, current_close);
                        }
                    }
                }

                // Add more complex logic here based on your strategy...

            }
            Ok(()) // Indicate successful execution
        }

        // --- 4. Do NOT implement the helper functions ---
        // `open_by_limit`, `open_by_market`, `close_by_market` and `close_by_limit` are default
        // methods of the `Strategy` trait (see §8.1). Call them with `&self.emitter`; overriding
        // them is only needed for a different order kind. Never send on a channel from a strategy
        // library: no heap object and no channel endpoint may cross the FFI boundary.
    }

    // --- 5. C FFI Interface (Required for Dynamic Loading) ---
    // These functions provide the C-compatible entry points for the main application.
    // You can usually copy these directly from the example, replacing `MyNewStrategy` with your struct name.

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn create_strategy(
        mode_cstr: *const std::os::raw::c_char,
        strategy_settings_ptr: *const farukon_core::settings::StrategySettings,
        strategy_instruments_info_ptr: *const std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        // Emission pair: opaque host context + host callback. Both are only stored and passed back;
        // the host keeps them valid for the whole strategy lifetime.
        emitter_ctx: *const std::ffi::c_void,
        emit_signal_cb: Option<farukon_core::event::EmitSignalFn>,
    ) -> *mut MyNewStrategy {
        if mode_cstr.is_null()
            || strategy_settings_ptr.is_null()
            || strategy_instruments_info_ptr.is_null()
            || emitter_ctx.is_null()
        {
            return std::ptr::null_mut();
        }
        let Some(emit_signal_cb) = emit_signal_cb else {
            return std::ptr::null_mut();
        };
        let mode = unsafe { std::ffi::CStr::from_ptr(mode_cstr) }.to_string_lossy().into_owned();
        let strategy_settings_ref = unsafe { &*strategy_settings_ptr }.clone();
        let strategy_instruments_info_ref = unsafe { &*strategy_instruments_info_ptr }.clone();
        let emitter = farukon_core::event::SignalEmitter { ctx: emitter_ctx, cb: emit_signal_cb };

        match MyNewStrategy::new(
            mode,
            strategy_settings_ref,
            strategy_instruments_info_ref,
            emitter,
        ) {
            Ok(strategy) => Box::into_raw(Box::new(strategy)),
            Err(_) => std::ptr::null_mut(),
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn destroy_strategy(strategy: *mut MyNewStrategy) {
        if !strategy.is_null() {
            unsafe {
                let _ = Box::from_raw(strategy);
            }
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn calculate_signals(
        strategy_ptr: *mut std::ffi::c_void,
        data_handler_vtable: *const farukon_core::DataHandlerVTable,
        data_handler_ptr: *const (),
        // The strategy only reads the portfolio state, so both pointers are `*const`.
        current_positions_ptr: *const std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
        all_holdings_ptr: *const Vec<farukon_core::portfolio::HoldingSnapshot>,
        symbol_list_ptr: *const *const std::os::raw::c_char,
        symbol_list_size: usize,
    ) -> i32 {
        if strategy_ptr.is_null() || current_positions_ptr.is_null() || all_holdings_ptr.is_null() || symbol_list_ptr.is_null() {
            return -1;
        }
        // The host owns the instance uniquely and never calls into it concurrently.
        let strategy = unsafe { &mut *(strategy_ptr as *mut MyNewStrategy) };

        let data_handler: &dyn farukon_core::data_handler::DataHandler = unsafe {
            std::mem::transmute::<(*const (), *const ()), &dyn farukon_core::data_handler::DataHandler>((
                data_handler_ptr,
                data_handler_vtable as *const(),
            ))
        };

        let current_positions = unsafe { &*current_positions_ptr };
        let all_holdings = unsafe { &*all_holdings_ptr };

        let symbols: Vec<String> = (0..symbol_list_size)
            .filter_map(|i| unsafe {
                let str_ptr = *symbol_list_ptr.add(i);
                if str_ptr.is_null() { return None; }
                std::ffi::CStr::from_ptr(str_ptr)
                    .to_str()
                    .ok()
                    .map(|s| s.to_string())
            })
            .collect();

        match strategy.calculate_signals(
            data_handler,
            current_positions,
            all_holdings,
            &symbols,
        ) {
            Ok(_) => 0,   // Success
            Err(_) => -1, // Error
        }
    }
    ```

    Each exported function carries a `# Safety` section in the reference implementation
    (`strategy_lib/src/lib.rs`); both sides must keep those contracts, in particular that the
    emission pair stays valid for the whole lifetime of the strategy returned by `create_strategy`.

4.  **Build Your Strategy Library:**
    *   Run `cargo build --release` inside your `my_new_strategy` directory.
    *   This will create the dynamic library file (e.g., `target/release/libmy_new_strategy.so` on Linux or `target/release/libmy_new_strategy.dylib` on macOS).
    *   Rebuild the library whenever the engine's strategy ABI changes (v3.0.0 was such a change): a library built for an older ABI must not be loaded, it will crash the engine.

5.  **Configure the Main Platform:**
    *   Update your main JSON configuration file (e.g., `Portfolios/MyConfig.json`).
    *   Point `strategy_path` to the newly created library file.
    *   Set `strategy_name` to the name of your strategy struct (`MyNewStrategy` in this example).
    *   Add any parameters your strategy requires to the `strategy_params` section.
    *   Ensure `data_path`, `symbols`, `symbol_base_name`, and other settings are correct.

    **Example Configuration Snippet:**

    ```json
    {
      "portfolio": {
        "Strategy_1": {
          "strategy_name": "MyNewStrategy", // Match your struct name
          "strategy_path": "target/release/libmy_new_strategy.so", // Path to your library
          "strategy_params": {
            "my_param1": [5.0], // Pass parameters to your strategy
            "my_param2": [20]
          },
          // ... other settings (data, symbols, pos_sizer, etc.) ...
        }
      }
    }
    ```

6.  **Build and Run the Main Platform:**
    *   Go back to the main project directory (`FarukonAlgoTradingPlatform`).
    *   Build the main application: `cargo build --release`.
    *   Run the backtester with your new configuration: `cargo run --release -- --config Portfolios/MyConfig.json`.

By focusing primarily on the `calculate_signals` function, you can implement the core logic of your trading strategy while leveraging the robust infrastructure provided by the Farukon platform and the standard patterns for initialization and signal sending.

### 8.4 `SYMI_Ch_SMA_up_lmt` Strategy

`SYMI_Ch_SMA_up_lmt` is a channel-based strategy. It builds a price channel from moving averages
of highs and lows, computes the channel width, and enters LONG/SHORT positions when the current
bar breaks the channel boundary, the channel width is below the `width_channel` threshold, and
the SMA confirms the direction.

The strategy source does not live in this repository — it is part of the measurement test stand
(`../Strategies/time_tests/SYMI_Ch_SMA_up_lmt.rs`, together with its variant
`SYMI_Ch_prct_SMA_up_lmt.rs`). The in-repo reference strategy is the MA-cross sample in
`strategy_lib/src/lib.rs`, which follows the same structure and the current ABI.

#### 8.4.1 Strategy Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `avg_price_period` | int | Period of the SMA applied to high/low prices forming the channel |
| `channel_period` | int | Lookback period for `highest`/`lowest` channel boundaries |
| `prct_width_channel` | float (percent) | Internal offset ("lustra") from the channel boundaries used for exits |
| `width_channel` | **float (points)** | Maximum allowed channel width for entering a position |
| `sma_period` | int | Period of the SMA on close used for direction confirmation |

#### 8.4.2 Working with Instruments of Different Price Magnitudes

`width_channel` is a **fractional threshold expressed in price points** and is compared to the
channel width **without integer truncation**. This is important for instruments with small
fractional prices:

- **Expensive integer-priced instruments** (e.g., `Si-3.23`, prices ~79 000): the channel width
  is tens–hundreds of points; set `width_channel` to a value of the same order of magnitude
  (e.g., `100`–`200`). Values smaller than the typical channel width will filter out all entries —
  this is intended filtering behavior.
- **Cheap fractional-priced instruments** (e.g., `CNY-3.23`, prices ~11.7): the channel width is
  hundredths–thousandths of a point (0.01–0.05). Set `width_channel` to a fractional value in the
  same range (e.g., `0.05`–`1.5`). Do **not** rely on integer truncation of the width: the
  comparison is performed in `f64`, so a fractional threshold is preserved exactly.
- When optimizing, use a `Range` spec for `width_channel` (e.g., `{"start": 0.05, "end": 1.505,
  "step": 0.05}`); the optimizer expands it to discrete values before the strategy is created.

The same logic applies to `prct_width_channel` — it is a percentage offset, so it is
scale-independent by design.

#### 8.4.3 Building the Strategy Library

`SYMI_Ch_SMA_up_lmt` is not part of the workspace build. It is compiled through a small `cdylib`
crate scaffold (the test stand keeps one per strategy at
`../Strategies/time_tests/dll_build/<strategy_name>/`):

```toml
[package]
name = "SYMI_Ch_SMA_up_lmt"
version = "3.0.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
anyhow = "1.0.99"
serde_json = "1.0.143"
farukon_core = { path = "<path-to-repo>/farukon_core" }
chrono = { version = "0.4.41", features = ["serde"] }
mimalloc = "0.1.52"
```

```rust
// src/lib.rs — build wrapper around the strategy source.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

include!("../../../SYMI_Ch_SMA_up_lmt.rs");
```

```sh
cargo build --release   # produces target/release/SYMI_Ch_SMA_up_lmt.dll (or .so/.dylib)
```

Two requirements are mandatory since v3.0.0:

- **mimalloc as the dll's global allocator** — the host binary allocates with mimalloc, and the
  allocator kind must match on both sides of the FFI boundary.
- **The current strategy ABI** — `create_strategy` takes the emission callback pair
  (`emitter_ctx`, `emit_signal_cb`) and the strategy stores `SignalEmitter { ctx, cb }`
  (see §8.1 and `farukon_core/src/event.rs`). A dll built against an older `farukon_core` will
  crash the new binary.

Then point `strategy_path` in the portfolio config at the produced library file.

#### 8.4.4 Validation

There is no automated business test for this strategy inside the repository. It is validated
through the measurement test stand: the optimizer configs under
`../Strategies/time_tests/portfolios/` (e.g. `SYMI_Ch_SMA_up_lmt_mpr_optimize_arc_2min_threads_*.json`
and the `SYMI_Ch_prct_SMA_up_lmt_..._1min_...` series) run full grid searches on real Si data, and
the produced `optimization_results.csv` can be compared against earlier runs for metric parity.
The v3.0.0 release was validated this way on the 1-minute (16 threads) and 2-minute (32 threads)
benchmarks with exact business-metric parity.

---

## 9. File Formats

*   **Configuration (JSON):** Standard JSON format for settings.
*   **Instrument Info (JSON):** Standard JSON format defining instrument metadata.
*   **Commission Plans (JSON):** Standard JSON format defining commission structures.
*   **Market Data (FlatBuffers `.soa.bin` + `.soa.idx`):**
    *   `.soa.bin`: Binary FlatBuffer file containing `OHLCVList` data. Optimized for zero-copy access.
    *   `.soa.idx`: Bincode-serialized index file containing `TimeIndexEntry`, `DailyIndexEntry`, and `TimeframeIndex` for fast navigation and resampling.
    *   **Generation:** Use the `csv-to-flatbuffer` tool.
*   **Results (CSV, `;`-delimited):** written into the strategy's `exit_results_path`:
    *   `optimization_results.csv` — the evaluated parameter sets of Grid Search / Genetic Algorithm with their metrics (`optimization_results_<strategy_name>.csv` per strategy in `Portfolio` mode).
    *   `lshade_optimization_results.csv`, `ga_optimization_results.csv` — per-iteration / per-generation statistics of LSHADE-RSP and the Genetic Algorithm.
    *   `equity_series.csv`, `equity_curve_<strategy_name>.csv` and the aggregated `optimization_results_portfolio.csv` / `equity_curve_portfolio.csv` (portfolio mode).
    *   Metric rows include `Max_Drawdown_DateTime` (`YYYY-MM-DD HH:MM:SS`) for every optimizer and both metrics modes — see [§7](#7-optimization).

---

## 10. Performance & Optimization

*   **Zero-Copy Data:** Using FlatBuffers with `mmap` is crucial for performance.
*   **SIMD:** Performance metrics and some indicators leverage SIMD for speed.
*   **Parallelism:** Grid Search, Genetic Algorithm, and LSHADE-RSP run evaluations in parallel using Rayon. Configure `threads` in your strategy settings (see the thread-scaling knee warning in [§7](#7-optimization)).
*   **Allocator:** the host binary `Farukon_2` and the strategy library both register `mimalloc` as their global allocator (rayon-parallel backtesting churns many small allocations per candidate, where the system allocator's locks become the bottleneck). The two sides keep **separate** allocators, which is why no heap object ever crosses the FFI boundary — signals are marshalled as plain arguments and the host allocates the event.
*   **Per-candidate hot path:** the FFI entry points (`calculate_signals`) and the symbol-list C buffers are resolved/built **once per strategy instance**, and immutable instrument metadata (expiration/trade-from dates) is parsed once at strategy creation; neither is repeated per bar.
*   **Dynamic Loading:** Allows strategy hot-swapping without recompiling the core engine. Only libraries built against the current strategy ABI may be loaded (see [§8](#8-extending-with-strategies)).

---

## 11. Troubleshooting

*   **"No instrument info for ...":** Verify the symbol exists in `instruments_info.json`.
*   **"Failed to create data handler":** Check if the `.soa.bin`/`.soa.idx` files exist and are readable at the specified `data_path`.
*   **"Failed to load dynamic strategy":** Ensure the `strategy_path` is correct and the library file exists. Check the `strategy_name` matches the exported symbol.
*   **Engine crashes as soon as a strategy library is loaded (or on the first signal):** the library was built against an older strategy ABI. Rebuild it against the current `farukon_core` (see [§8](#8-extending-with-strategies)); a library built for the pre-v3.0.0 channel ABI cannot work with the current host.
*   **"No commission plan found for exchange '...' / commission_type '...'":** `commission_plans.json` has no rate for that exchange/commission-type pair, so the execution handler fails the order with this descriptive error instead of silently trading without commission.
*   **"WARNING: global_data_storage_mode = \"deep\""** or a thread-scaling warning: both are advisories printed once per run; the run continues. Switch to `"arc"` storage mode or reduce `threads` (see [§7](#7-optimization)).
*   **Negative Capital / Margin Calls:** Review your strategy logic, slippage, commission settings, and margin requirements in `instruments_info.json`.
*   **Slow Performance:** Ensure you are using FlatBuffers data, not CSV. Check the number of threads configured and the storage mode (`arc` vs `deep`). Profile your strategy code if necessary.

---

## 12. Glossary

*   **Backtest:** A simulation of a trading strategy on historical market data.
*   **Event-Driven:** A programming paradigm where the flow of the program is determined by events (e.g., new market data, signals).
*   **FlatBuffers:** A cross-platform serialization library that allows access to serialized data without parsing/unpacking.
*   **Grid Search:** An optimization technique that systematically works through multiple combinations of parameter tunes.
*   **Genetic Algorithm (GA):** A search heuristic inspired by the process of natural selection.
*   **Index (`.soa.idx`):** A companion file to FlatBuffers data providing fast lookup and navigation.
*   **Market Bar:** A data point representing OHLCV (Open, High, Low, Close, Volume) for a specific time period.
*   **Memory Mapping (`mmap`):** A mechanism that maps a file directly into memory for efficient access.
*   **Multi-Threading:** Executing multiple threads of execution concurrently.
*   **Position Sizing:** Determining the amount of capital to risk on a single trade.
*   **SIMD:** Single Instruction, Multiple Data - a type of parallel processing that performs the same operation on multiple data points simultaneously.
*   **Strategy:** The algorithmic logic that determines when to buy, sell, or hold assets.
*   **Zero-Copy:** A technique where data is accessed directly from its source (e.g., memory-mapped file) without copying it into intermediate buffers.

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
*   **Dynamic Strategy Loading:** Strategies are compiled as separate dynamic libraries (`.so` on Linux, `.dylib` on macOS) and loaded at runtime, enabling hot-swapping of logic without recompiling the core engine.
*   **Advanced Optimization:** Includes both Grid Search (exhaustive) and Genetic Algorithm (evolutionary) optimizers for hyperparameter tuning.
*   **Risk Management:** Implements margin checking, position sizing (e.g., MPR - Maximum Possible Risk), and margin call monitoring.
*   **Modular Core:** Core logic is separated into the `farukon_core` library, making it reusable and easier to maintain.

---

## 2. Architecture

The platform is structured as a Rust workspace containing several crates:

*   **`Farukon_2_0`:** The main application executable. Orchestrates the backtesting process, handles command-line arguments, and manages the lifecycle of other components.
*   **`farukon_core`:** A shared library containing the core logic: event system, data handler trait, portfolio management, performance calculation, optimization utilities, and instrument information handling. This is the library that both `Farukon_2_0` and `strategy_lib` depend on.
*   **`strategy_lib`:** An example dynamic library containing a sample strategy implementation (e.g., Moving Average Cross). This crate is compiled into a `.so`/`.dylib` file.

### Core Concepts:

*   **Events:** Communication between components happens via a publish-subscribe model using an `mpsc` (multi-producer, single-consumer) channel. Events include `MARKET` (new bar), `SIGNAL` (strategy intent), `ORDER` (portfolio action), and `FILL` (execution result).
*   **Data Handler:** An abstraction (`trait DataHandler`) for accessing market data. Implementations like `HistoricFlatBuffersDataHandlerZC` provide the actual data loading and access logic.
*   **Strategy:** Implements the `Strategy` trait, defining the `calculate_signals` logic based on market data and portfolio state.
*   **Portfolio:** Manages positions, holdings, and equity. Updates state based on `FILL` events.
*   **Execution Handler:** Simulates order execution, applying slippage and commission.

---

## 3. Core Components

This section details the main modules within `farukon_core` and `Farukon_2_0`.

### `farukon_core`

*   **`event`:** Defines the `Event` trait and concrete event types (`MarketEvent`, `SignalEvent`, `OrderEvent`, `FillEvent`). Enables type-erased communication.
*   **`data_handler`:** Defines the `DataHandler` trait, which abstracts data source access. Implementations must provide methods to get the latest bars, values, and advance the data timeline.
*   **`strategy`:** Defines the `Strategy` trait. All user-defined strategies must implement this trait to be compatible with the platform.
*   **`portfolio`:** Defines the `PortfolioHandler` trait and related structures (`PositionState`, `HoldingsState`, `EquityPoint`). Manages the state and updates based on fill events.
*   **`execution`:** Defines the `ExecutionHandler` trait for simulating trade execution.
*   **`indicators`:** Contains basic technical indicators (e.g., `sma`) that strategies can use.
*   **`performance`:** Calculates performance metrics (`Total Return`, `APR`, `Max Drawdown`, `Recovery Factor`, etc.) using SIMD for speed.
*   **`optimization`:** Contains the `GridSearchOptimizer` and `GeneticAlgorythm` implementations.
*   **`instruments_info`:** Manages instrument metadata (margin, step, step_price, expiration, etc.) loaded from `instruments_info.json`.
*   **`commission_plans`:** Manages commission structures loaded from `commission_plans.json` and calculates fees.
*   **`index`:** Defines structures for FlatBuffer indexing (used by data handlers).
*   **`settings`:** Defines structures for parsing and holding configuration from the JSON settings file.
*   **`pos_sizers`:** Implements position sizing logic (e.g., MPR).
*   **`utils`:** Contains utility functions for parsing settings, calculating quantities, etc.

### `Farukon_2_0`

*   **`main`:** Entry point. Parses command-line arguments (`--config`) and starts the optimization process.
*   **`backtest`:** Contains the `Backtest` struct, which runs the main event loop, coordinating data updates, strategy signals, portfolio updates, and execution simulation.
*   **`data_handler`:** Contains implementations of the `DataHandler` trait, including `HistoricCSVDataHandler` (legacy) and `HistoricFlatBuffersDataHandlerZC` (production).
*   **`execution`:** Contains `SimulatedExecutionHandler` which implements the `ExecutionHandler` trait.
*   **`portfolio`:** Contains `Portfolio` which implements the `PortfolioHandler` trait.
*   **`optimizers`:** Contains `OptimizationRunner` which manages the optimization process (Grid Search / Genetic Algorithm).
*   **`strategy_loader`:** Contains logic for dynamically loading strategy libraries (`.so`/`.dylib`) at runtime.

---

## 4. Data Flow

1.  **Initialization:**
    *   `Farukon_2_0` loads settings from the JSON file.
    *   A `DataHandler` (e.g., `HistoricFlatBuffersDataHandlerZC`) is created, loading market data (`.bin`, `.idx`) into memory via `mmap`.
    *   A `Portfolio` is created with initial capital.
    *   A `SimulatedExecutionHandler` is created.
    *   The strategy library (`.so`/`.dylib`) is dynamically loaded using `strategy_loader`.
    *   An `mpsc` event channel is established.

2.  **Backtesting Loop (`Backtest::run_backtest`):**
    *   `DataHandler::update_bars()` is called. It advances the data (e.g., reads next 5-minute bar from FlatBuffers) and sends a `MARKET` event to the channel.
    *   The main loop drains events from the channel:
        *   **`MARKET` Event:**
            *   The `Strategy`'s `calculate_signals` method is called with the latest market data and portfolio state.
            *   If the strategy generates a signal (e.g., "LONG"), it sends a `SIGNAL` event.
            *   The `Portfolio` updates its time-indexed state (positions, holdings, equity).
        *   **`SIGNAL` Event:**
            *   The `Portfolio` receives the signal and potentially generates an `ORDER` event based on position sizing and margin checks.
            *   The `ORDER` event is sent to the channel.
        *   **`ORDER` Event:**
            *   The `SimulatedExecutionHandler` receives the order, simulates execution (applying slippage/commission based on `MARKET`/`LMT` type and current bar data), and sends a `FILL` event.
        *   **`FILL` Event:**
            *   The `Portfolio` receives the `FILL` event and updates its position and holding states.
    *   This loop continues until `DataHandler::get_continue_backtest()` returns `false`.

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
    *   **`mode`** (String): `"Debug"`, `"Optimize"`, `"Visual"`. Controls verbosity and behavior.
    *   **`initial_capital`** (float): Starting capital for the entire portfolio.

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

4.  **Prepare Data:**
    *   Place your market data in the `Tickers/` directory.
    *   Data must be in FlatBuffers format (`.bin` files) with corresponding index files (`.idx`).
    *   Use the companion tool [csv-to-flatbuffer](https://github.com/andydardgallard/csv-to-flatbuffer) to convert your CSV/TXT OHLCV data into the required `.bin`/`.idx` format.

5.  **Prepare Configuration:**
    *   Create or modify your configuration JSON file (e.g., `Portfolios/Debug_Portfolio.json`).
    *   Ensure `strategy_path` points to the compiled strategy library (e.g., `target/release/libstrategy_lib.dylib`).
    *   Ensure `data_path` in the config points to the directory containing your `.bin`/`.idx` files.
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

The platform supports two optimization methods:

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

---

## 8. Extending with Strategies

To create a new trading strategy:

1.  **Implement the `Strategy` Trait:** In your own Rust crate (or modify `strategy_lib`), define a struct (e.g., `MyAwesomeStrategy`) and implement the `farukon_core::strategy::Strategy` trait. The key method is `calculate_signals`, which contains your trading logic.
2.  **Build as a Dynamic Library:** Configure your `Cargo.toml` to build as a `cdylib`:
    ```toml
    [lib]
    name = "my_strategy_lib" # Name of the resulting .so/.dylib
    crate-type = ["cdylib"]
    ```
3.  **Export C Functions:** The platform expects specific C-compatible functions to be exported from your library: `create_strategy`, `destroy_strategy`, and `calculate_signals`. The `strategy_lib` example shows the required signatures.
4.  **Configure:** Update your JSON configuration file to point `strategy_path` to your new `.so`/`.dylib` file and set `strategy_name` to match the expected name in your `create_strategy` function.
5.  **Build and Run:** Build your strategy library (`cargo build --release`) and run the main platform with the updated config.

---

## 9. File Formats

*   **Configuration (JSON):** Standard JSON format for settings.
*   **Instrument Info (JSON):** Standard JSON format defining instrument metadata.
*   **Commission Plans (JSON):** Standard JSON format defining commission structures.
*   **Market Data (FlatBuffers `.bin` + `.idx`):**
    *   `.bin`: Binary FlatBuffer file containing `OHLCVList` data. Optimized for zero-copy access.
    *   `.idx`: Bincode-serialized index file containing `TimeIndexEntry`, `DailyIndexEntry`, and `TimeframeIndex` for fast navigation and resampling.
    *   **Generation:** Use the `csv-to-flatbuffer` tool.

---

## 10. Performance & Optimization

*   **Zero-Copy Data:** Using FlatBuffers with `mmap` is crucial for performance.
*   **SIMD:** Performance metrics and some indicators leverage SIMD for speed.
*   **Parallelism:** Grid Search and Genetic Algorithm run evaluations in parallel using Rayon. Configure `threads` in your strategy settings.
*   **Dynamic Loading:** Allows strategy hot-swapping without recompiling the core engine.

---

## 11. Troubleshooting

*   **"No instrument info for ...":** Verify the symbol exists in `instruments_info.json`.
*   **"Failed to create data handler":** Check if the `.bin`/`.idx` files exist and are readable at the specified `data_path`.
*   **"Failed to load dynamic strategy":** Ensure the `strategy_path` is correct and the library file exists. Check the `strategy_name` matches the exported symbol.
*   **Negative Capital / Margin Calls:** Review your strategy logic, slippage, commission settings, and margin requirements in `instruments_info.json`.
*   **Slow Performance:** Ensure you are using FlatBuffers data, not CSV. Check the number of threads configured. Profile your strategy code if necessary.

---

## 12. Glossary

*   **Backtest:** A simulation of a trading strategy on historical market data.
*   **Event-Driven:** A programming paradigm where the flow of the program is determined by events (e.g., new market data, signals).
*   **FlatBuffers:** A cross-platform serialization library that allows access to serialized data without parsing/unpacking.
*   **Grid Search:** An optimization technique that systematically works through multiple combinations of parameter tunes.
*   **Genetic Algorithm (GA):** A search heuristic inspired by the process of natural selection.
*   **Index (`.idx`):** A companion file to FlatBuffers data providing fast lookup and navigation.
*   **Market Bar:** A data point representing OHLCV (Open, High, Low, Close, Volume) for a specific time period.
*   **Memory Mapping (`mmap`):** A mechanism that maps a file directly into memory for efficient access.
*   **Multi-Threading:** Executing multiple threads of execution concurrently.
*   **Position Sizing:** Determining the amount of capital to risk on a single trade.
*   **SIMD:** Single Instruction, Multiple Data - a type of parallel processing that performs the same operation on multiple data points simultaneously.
*   **Strategy:** The algorithmic logic that determines when to buy, sell, or hold assets.
*   **Zero-Copy:** A technique where data is accessed directly from its source (e.g., memory-mapped file) without copying it into intermediate buffers.

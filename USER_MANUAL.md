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

Конечно. Ниже приведён обновлённый раздел **User Manual**, включающий **детальный разбор примера стратегии пересечения средних** (`MovingAverageCrossStrategy`) и **руководство по созданию новой стратегии**.

---

## 8. Extending with Strategies

To create a new trading strategy, you need to implement the `Strategy` trait defined in `farukon_core` and compile it as a dynamic library (`.so` on Linux, `.dylib` on macOS) that the main platform can load at runtime.

### 8.1 Understanding the `Strategy` Trait

The core of any strategy is the `Strategy` trait defined in `farukon_core/src/strategy.rs`:

```rust
// farukon_core/src/strategy.rs

use crate::event;
use crate::portfolio;
use crate::data_handler;

pub trait Strategy {
    /// The main logic function called on every market bar update.
    /// This is where you implement your trading signals based on data and portfolio state.
    /// # Arguments
    /// * `data_handler` - Interface to access market data (OHLCV, timestamps).
    /// * `current_positions` - Current position states for all symbols managed by this strategy.
    /// * `latest_equity_point` - The latest equity point (capital, blocked, cash).
    /// * `symbol_list` - List of symbols this strategy trades.
    /// # Returns
    /// * `anyhow::Result<()>` - Ok(()) on success, Err(...) if an error occurs.
    fn calculate_signals(&mut self,
        data_handler: &dyn data_handler::DataHandler,
        current_positions: &std::collections::HashMap<String, portfolio::PositionState>,
        latest_equity_point: &portfolio::EquitySnapshot,
        symbol_list: &[String],
    ) -> anyhow::Result<()>;

    /// Helper function to send a LIMIT order signal event.
    /// # Arguments
    /// * `event_sender` - Channel to send the signal event.
    /// * `current_bar_datetime` - Timestamp for the signal.
    /// * `symbol` - The symbol to trade.
    /// * `signal_name` - The name of the signal (e.g., "LONG", "SHORT", "EXIT").
    /// * `quantity` - Quantity to trade (optional, can be determined by position sizer).
    /// * `limit_price` - The limit price for the order.
    fn open_by_limit(&self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
        limit_price: f64,
    ) -> anyhow::Result<()>;

    /// Helper function to send a MARKET order signal event.
    /// # Arguments
    /// * `event_sender` - Channel to send the signal event.
    /// * `current_bar_datetime` - Timestamp for the signal.
    /// * `symbol` - The symbol to trade.
    /// * `signal_name` - The name of the signal (e.g., "LONG", "SHORT", "EXIT").
    /// * `quantity` - Quantity to trade (optional, can be determined by position sizer).
    fn open_by_market(&self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
    ) -> anyhow::Result<()>;

    /// Helper function to send a MARKET order signal event to close a position.
    /// # Arguments
    /// * `event_sender` - Channel to send the signal event.
    /// * `current_bar_datetime` - Timestamp for the signal.
    /// * `symbol` - The symbol to trade.
    /// * `signal_name` - The name of the signal (e.g., "EXIT").
    /// * `quantity` - Quantity to close (usually the current position size).
    fn close_by_market(&self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
    ) -> anyhow::Result<()>;

    /// Helper function to send a LIMIT order signal event to close a position.
    /// # Arguments
    /// * `event_sender` - Channel to send the signal event.
    /// * `current_bar_datetime` - Timestamp for the signal.
    /// * `symbol` - The symbol to trade.
    /// * `signal_name` - The name of the signal (e.g., "EXIT").
    /// * `quantity` - Quantity to close (usually the current position size).
    /// * `limit_price` - The limit price for the order.
    fn close_by_limit(&self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    ) -> anyhow::Result<()>;
}
```

**Key Points:**

*   **`calculate_signals`**: This is your **main strategy function**. It's called by the backtester every time a new market bar arrives for *any* of the symbols in your `symbol_list`. You access market data, check your portfolio state, and decide whether to generate buy/sell/exit signals.
*   **Helper Functions (`open_by_*`, `close_by_*`)**: These functions simplify sending `SignalEvent`s through the event channel. The `Portfolio` module receives these signals, processes them (e.g., checks margin, calculates quantity using position sizer), and creates `OrderEvent`s which are sent to the `ExecutionHandler`.
*   **`data_handler`**: Provides methods like `get_latest_bar_value(symbol, "close")`, `get_latest_bars(symbol, n)`, etc., to access market data.
*   **`current_positions`**: A map of symbol names to `PositionState` structs, allowing you to check if you are currently long, short, or flat on a symbol, and the size of the position.
*   **`latest_equity_point`**: Provides access to your current capital, blocked margin, and cash balance.

### 8.2 Detailed Analysis: `MovingAverageCrossStrategy`

Let's examine the provided `strategy_lib/src/lib.rs` which implements the `MovingAverageCrossStrategy`.

#### 8.2.1 Structure and Initialization

```rust
// strategy_lib/src/lib.rs

use farukon_core::{self, strategy::Strategy}; // Import the core library and Strategy trait

// The main strategy struct holds its configuration and state.
pub struct MovingAverageCrossStrategy {
    mode: String, // e.g., "Debug", "Optimize"
    strategy_settings: farukon_core::settings::StrategySettings, // Configuration loaded from JSON
    strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>, // Metadata for traded symbols
    event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>, // Channel to send signals
    short_window: usize, // Length of the short-term SMA (e.g., 50)
    long_window: usize,  // Length of the long-term SMA (e.g., 100)
}

impl MovingAverageCrossStrategy {
    pub fn new(
        mode: String,
        strategy_settings: farukon_core::settings::StrategySettings,
        strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    ) -> anyhow::Result<Self> {
        // Helper function to extract a usize parameter from the JSON settings.
        fn get_param_as_usize(params: &std::collections::HashMap<String, Vec<serde_json::Value>>, name: &str) -> anyhow::Result<usize> {
            let value = params
                .get(name)
                .and_then(|v| v.first()) // Get the first value from the array in the JSON
                .ok_or_else(|| anyhow::anyhow!("Missing parameter '{}'", name))?;

            if let Some(val) = value.as_u64() {
                anyhow::Ok(val as usize)
            } else if let Some(val) = value.as_f64() {
                anyhow::Ok(val as usize)
            } else {
                Err(anyhow::anyhow!("Parameter '{}' must be a number, got: {:?}", name, value))
            }
        }

        // Extract required parameters (short_window, long_window) from strategy_settings.strategy_params
        let short_window = get_param_as_usize(&strategy_settings.strategy_params, "short_window")?;
        let long_window = get_param_as_usize(&strategy_settings.strategy_params, "long_window")?;

        // Validate parameters: short window must be less than long window
        if short_window >= long_window{
            anyhow::bail!("'short_window' ({}) must be less than 'long_window' ({}).", short_window, long_window);
        }

        // Create and return the strategy instance
        anyhow::Ok(
            MovingAverageCrossStrategy {
                mode,
                strategy_settings,
                strategy_instruments_info,
                short_window: short_window as usize,
                long_window: long_window as usize,
                event_sender,                          
            }
        )
    }
}
```

*   **`new`**: This constructor is called by the dynamic loading mechanism (in `strategy_loader.rs`) when the library is loaded. It receives the `mode`, parsed `strategy_settings` (from the JSON config), `strategy_instruments_info` (parsed from `instruments_info.json`), and the `event_sender` channel.
*   **Parameter Parsing**: It uses the `get_param_as_usize` helper to extract `short_window` and `long_window` from the `strategy_params` map within `strategy_settings`. This map comes directly from the `strategy_params` section in your JSON config file.
*   **Validation**: It performs a simple validation to ensure `short_window < long_window`.
*   **State Storage**: The parsed parameters and other necessary data are stored in the struct instance.

#### 8.2.2 Core Logic: `calculate_signals`

```rust
// ... inside impl Strategy for MovingAverageCrossStrategy

fn calculate_signals(
        &mut self, // Mutable reference to self to allow state changes if needed
        data_handler: &dyn farukon_core::data_handler::DataHandler, // Access to market data
        current_positions: &std::collections::HashMap<String, farukon_core::portfolio::PositionState>, // Current portfolio state
        latest_equity_point: &farukon_core::portfolio::EquitySnapshot, // Current equity state
        symbol_list: &[String], // List of symbols to trade
) -> anyhow::Result<()> {
    // Iterate through each symbol in the configured list
    for symbol in symbol_list{
        // Get current capital and instrument info for the symbol
        let capital = Some(latest_equity_point.equity_point.capital);
        let strategy_instruments_info_for_symbol = self.strategy_instruments_info.get(symbol).unwrap();

        // Get the current datetime and close price for the symbol
        let current_bar_datetime = data_handler.get_latest_bar_datetime(symbol).unwrap();
        let close = Some(data_handler.get_latest_bar_value(symbol, "close").unwrap());
 
        // Parse expiration and trade start dates from instrument info
        let expiration_date = &strategy_instruments_info_for_symbol.expiration_date;
        let expiration_date_dt = farukon_core::utils::string_to_date_time(expiration_date, "%Y-%m-%d %H:%M:%S")?;

        let trade_from_date = &strategy_instruments_info_for_symbol.trade_from_date;
        let trade_from_date_dt = farukon_core::utils::string_to_date_time(trade_from_date, "%Y-%m-%d %H:%M:%S")?;

        // Get current position state for the symbol
        let current_position_state = current_positions.get(symbol).unwrap();
        let current_position_quantity = current_position_state.position;

        // Calculate the short and long SMAs
        if let (Some(short_sma), Some(long_sma)) = (
            farukon_core::indicators::sma(data_handler, symbol, "close", self.short_window, 0), // Calculate SMA with no shift
            farukon_core::indicators::sma(data_handler, symbol, "close", self.long_window, 0),  // Calculate SMA with no shift
        ) {
            // Debug logging
            if self.mode == "Debug".to_string() {
                println!("Start event, Indicators, {}, {}, short_sma: {}, long_sma: {}, current_position: {}", symbol, current_bar_datetime, short_sma, long_sma, current_position_quantity);
                println!("Start event, Indicators + equity_point, {:?}", latest_equity_point);
            }
                                
            // --- Check for EXIT conditions first ---
            if current_position_quantity != 0.0 {
                let signal_name = "EXIT";
                // Check if long position exists
                if current_position_quantity > 0.0 {
                    // EXIT LONG: if short SMA crosses below long SMA OR expiration date is reached
                    if short_sma < long_sma {
                        self.close_by_market(
                            &self.event_sender,
                            current_bar_datetime,
                            symbol,
                            signal_name,
                            Some(current_position_quantity), // Close the full long position
                        )?;
                    }
                    else if current_bar_datetime >= expiration_date_dt {
                        self.close_by_market(
                            &self.event_sender,
                            current_bar_datetime,
                            symbol,
                            signal_name,
                            Some(current_position_quantity), // Close the full long position
                        )?;
                    }
                }
                // Check if short position exists
                else { // current_position_quantity < 0.0
                    // EXIT SHORT: if short SMA crosses above long SMA OR expiration date is reached
                    if short_sma > long_sma {
                        self.close_by_market(
                            &self.event_sender,
                            current_bar_datetime,
                            symbol,
                            signal_name,
                            Some(current_position_quantity), // Close the full short position (quantity is negative)
                        )?;
                    }
                    // EXIT by expiration
                    else if current_bar_datetime >= expiration_date_dt {
                        self.close_by_market(
                            &self.event_sender,
                            current_bar_datetime,
                            symbol,
                            signal_name,
                            Some(current_position_quantity), // Close the full short position
                        )?;
                    } 
                }
            }
            // --- Check for ENTRY conditions if no position exists ---
            else { // current_position_quantity == 0.0
                // LONG: Check for crossover and validity period
                if short_sma > long_sma &&
                current_bar_datetime < expiration_date_dt && // Must be before expiration
                current_bar_datetime >= trade_from_date_dt  // Must be after trade start date
                {
                    let signal_name = "LONG";
                    // Calculate position size using the configured position sizer
                    let quantity = farukon_core::pos_sizers::get_pos_sizer_from_settings(
                        &self.mode,
                        capital,
                        close,
                        Some(long_sma), // Example: pass long_sma as a parameter to the sizer
                        &self.strategy_settings,
                        strategy_instruments_info_for_symbol,
                    );

                    // Send a LIMIT order signal to open a long position
                    self.open_by_limit(
                        &self.event_sender,
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
                else if
                short_sma < long_sma &&
                current_bar_datetime < expiration_date_dt && // Must be before expiration
                current_bar_datetime >= trade_from_date_dt  // Must be after trade start date
                {
                    let signal_name = "SHORT";
                    // Calculate position size using the configured position sizer
                    let quantity = farukon_core::pos_sizers::get_pos_sizer_from_settings(
                        &self.mode,
                        capital,
                        close,
                        Some(long_sma), // Example: pass long_sma as a parameter to the sizer
                        &self.strategy_settings,
                        strategy_instruments_info_for_symbol,
                    );
                    
                    // Send a LIMIT order signal to open a short position
                    self.open_by_limit(
                        &self.event_sender,
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
            if self.mode == "Debug".to_string() {
                println!("Finish event, Indicators, {}, {}, short_sma: {}, long_sma: {}, current_position: {}", symbol, current_bar_datetime, short_sma, long_sma, current_position_quantity);
                println!("Finish event, Indicators + equity_point, {:?}", latest_equity_point);
            }
        }
        // If SMAs could not be calculated (e.g., insufficient data), do nothing for this bar/symbol.
    }

    // Return Ok to indicate successful signal calculation for this iteration
    anyhow::Ok(())
}
```

*   **Iteration**: It loops through each symbol in the `symbol_list` (e.g., `["Si-12.23", "Si-3.24"]`).
*   **Data Access**: It retrieves the current datetime, close price, instrument metadata (for expiration/trade dates), and the current position quantity for the symbol using the `data_handler` and `current_positions` map.
*   **Indicator Calculation**: It calls `farukon_core::indicators::sma` to calculate the short-term and long-term SMAs using the data handler. It passes the symbol, value type ("close"), the window length (`self.short_window`, `self.long_window`), and a shift of 0 (current bar).
*   **Logic Flow**:
    1.  **Exit Check**: If a position exists (`current_position_quantity != 0.0`), it checks for exit conditions:
        *   **Long Exit**: If short SMA < long SMA (bearish crossover) OR expiration date reached.
        *   **Short Exit**: If short SMA > long SMA (bullish crossover) OR expiration date reached.
        *   If an exit condition is met, it calls `self.close_by_market(...)` to send an "EXIT" signal.
    2.  **Entry Check**: If no position exists (`current_position_quantity == 0.0`), it checks for entry conditions:
        *   **Long Entry**: If short SMA > long SMA (bullish crossover) AND within the valid trading period (before expiration, after trade start).
        *   **Short Entry**: If short SMA < long SMA (bearish crossover) AND within the valid trading period.
        *   If an entry condition is met, it calculates the position size using `farukon_core::pos_sizers::get_pos_sizer_from_settings` based on the strategy's configuration (e.g., "mpr", value 1.5). Then, it calls `self.open_by_limit(...)` to send a "LONG" or "SHORT" signal with the calculated quantity.
*   **Signal Sending**: The helper functions `open_by_limit`, `close_by_market`, etc., create `SignalEvent` structs and send them via the `event_sender` channel. The `Portfolio` module receives these signals and handles the order creation and execution simulation.

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
    *   Crucially, set the crate type to `cdylib` so it compiles into a dynamic library (`.so` on Linux, `.dylib` on macOS) that can be loaded by the main application.

    **Example `Cargo.toml`:**

    ```toml
    [package]
    name = "my_new_strategy"
    version = "0.1.0"
    edition = "2021"

    [lib]
    # This tells Cargo to build a dynamic library (.so/.dylib)
    crate-type = ["cdylib"]

    [dependencies]
    # Link to the Farukon core library
    farukon_core = { path = "../farukon_core" } # Adjust path as needed
    # Add other libraries you might need (e.g., for complex math, indicators)
    anyhow = "1.0"
    chrono = "0.4"
    ```

3.  **Implement Your Strategy in `src/lib.rs`:**
    *   Replace the contents of the generated `src/lib.rs` file.
    *   **Define Your Strategy Struct:** This struct holds the state and configuration for your strategy instance.
    *   **Implement the `new` Constructor:** This function is called when the library is loaded. It receives initial configuration (mode, settings, instrument info, event channel) and should parse any strategy-specific parameters from `strategy_settings.strategy_params`.
    *   **Implement the `Strategy` Trait:** This is the core.
        *   **`calculate_signals` (Your Focus):** This function is called on every market bar update. Here, you access market data (`data_handler`), check your current portfolio state (`current_positions`, `latest_equity_point`), apply your trading logic, and send signals (`open_by_*`, `close_by_*`).
        *   **Helper Functions (`open_by_*`, `close_by_*`):** These functions are boilerplate for sending signals. You can often copy these directly from the `MovingAverageCrossStrategy` example. They take care of creating the correct `SignalEvent` and sending it via the `event_sender`.

    **Example Skeleton:**

    ```rust
    // my_new_strategy/src/lib.rs

    use farukon_core::{self, strategy::Strategy};

    // --- 1. Define Your Strategy Struct ---
    pub struct MyNewStrategy {
        mode: String,
        strategy_settings: farukon_core::settings::StrategySettings,
        strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
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
            event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
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
                event_sender,
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
                latest_equity_point: &farukon_core::portfolio::EquitySnapshot,
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
                            Some(latest_equity_point.equity_point.capital),
                            Some(current_close),
                            None, // Pass any relevant value for position sizing, e.g., long SMA
                            &self.strategy_settings,
                            instrument_info,
                        );

                        // Send a signal to open a long position
                        self.open_by_market(
                            &self.event_sender,
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
                            &self.event_sender,
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

        // --- 4. Implement Helper Functions (Often Boilerplate) ---
        // These functions send signal events. You can usually copy these from the example.
        fn open_by_limit(&self,
            event_sender: &std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
            current_bar_datetime: chrono::DateTime<chrono::Utc>,
            symbol: &String,
            signal_name: &str,
            quantity: Option<f64>,
            limit_price: f64,
        ) -> anyhow::Result<()> {
            event_sender.send(Box::new(farukon_core::event::SignalEvent::new(
                current_bar_datetime,
                symbol.clone(),
                signal_name.to_string(),
                "LMT".to_string(),
                quantity,
                Some(limit_price),
            )))?;
            Ok(())
        }

        fn open_by_market(&self,
            event_sender: &std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
            current_bar_datetime: chrono::DateTime<chrono::Utc>,
            symbol: &String,
            signal_name: &str,
            quantity: Option<f64>,
        ) -> anyhow::Result<()> {
             event_sender.send(Box::new(farukon_core::event::SignalEvent::new(
                current_bar_datetime,
                symbol.clone(),
                signal_name.to_string(),
                "MKT".to_string(),
                quantity,
                None, // No limit price for market orders
            )))?;
            Ok(())
        }

        fn close_by_market(&self,
            event_sender: &std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
            current_bar_datetime: chrono::DateTime<chrono::Utc>,
            symbol: &String,
            signal_name: &str,
            quantity: Option<f64>,
        ) -> anyhow::Result<()> {
             event_sender.send(Box::new(farukon_core::event::SignalEvent::new(
                current_bar_datetime,
                symbol.clone(),
                signal_name.to_string(),
                "MKT".to_string(),
                quantity,
                None, // No limit price for market orders
            )))?;
            Ok(())
        }

         fn close_by_limit(&self,
            event_sender: &std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
            current_bar_datetime: chrono::DateTime<chrono::Utc>,
            symbol: &String,
            signal_name: &str,
            quantity: Option<f64>,
            limit_price: Option<f64>,
        ) -> anyhow::Result<()> {
             event_sender.send(Box::new(farukon_core::event::SignalEvent::new(
                current_bar_datetime,
                symbol.clone(),
                signal_name.to_string(),
                "LMT".to_string(),
                quantity,
                limit_price,
            )))?;
            Ok(())
        }
    }

    // --- 5. C FFI Interface (Required for Dynamic Loading) ---
    // These functions provide the C-compatible entry points for the main application.
    // You can usually copy these directly from the example, replacing `MyNewStrategy` with your struct name.

    #[unsafe(no_mangle)]
    pub extern "C" fn create_strategy(
        mode_cstr: *const std::os::raw::c_char,
        strategy_settings_ptr: *const farukon_core::settings::StrategySettings,
        strategy_instruments_info_ptr: *const std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        event_sender_ptr: *const std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    ) -> *mut MyNewStrategy {
        if mode_cstr.is_null() || strategy_settings_ptr.is_null() || strategy_instruments_info_ptr.is_null() || event_sender_ptr.is_null() {
            return std::ptr::null_mut();
        }
        let mode = unsafe { std::ffi::CStr::from_ptr(mode_cstr) }.to_string_lossy().into_owned();
        let strategy_settings_ref = unsafe { &*strategy_settings_ptr }.clone();
        let strategy_instruments_info_ref = unsafe { &*strategy_instruments_info_ptr }.clone();
        let event_sender_ref = unsafe { &*event_sender_ptr }.clone();

        match MyNewStrategy::new(
            mode,
            strategy_settings_ref,
            strategy_instruments_info_ref,
            event_sender_ref,
        ) {
            Ok(strategy) => Box::into_raw(Box::new(strategy)),
            Err(_) => std::ptr::null_mut(),
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn destroy_strategy(strategy: *mut MyNewStrategy) {
        if !strategy.is_null() {
            unsafe {
                let _ = Box::from_raw(strategy);
            }
        }
    }

    #[unsafe(no_mangle)]
    pub extern "C" fn calculate_signals(
        strategy_ptr: *mut std::ffi::c_void,
        data_handler_vtable: *const farukon_core::DataHandlerVTable,
        data_handler_ptr: *const (),
        current_positions_ptr: *mut std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
        latest_equity_point_ptr: *mut farukon_core::portfolio::EquitySnapshot,
        symbol_list_ptr: *const *const std::os::raw::c_char,
        symbol_list_size: usize,
    ) -> i32 {
         if strategy_ptr.is_null() || current_positions_ptr.is_null() || latest_equity_point_ptr.is_null() || symbol_list_ptr.is_null() {
            return -1;
        }
        let strategy = unsafe { &mut *(strategy_ptr as *mut MyNewStrategy) };

        let data_handler: &dyn farukon_core::data_handler::DataHandler = unsafe {
            std::mem::transmute::<(*const (), *const ()), &dyn farukon_core::data_handler::DataHandler>((
                data_handler_ptr,
                data_handler_vtable as *const(),
            ))
        };

        let current_positions = unsafe { &mut *current_positions_ptr };
        let latest_equity_point = unsafe { &mut *latest_equity_point_ptr };

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
            latest_equity_point,
            &symbols,
        ) {
            Ok(_) => 0,   // Success
            Err(_) => -1, // Error
        }
    }
    ```

4.  **Build Your Strategy Library:**
    *   Run `cargo build --release` inside your `my_new_strategy` directory.
    *   This will create the dynamic library file (e.g., `target/release/libmy_new_strategy.so` on Linux or `target/release/libmy_new_strategy.dylib` on macOS).

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

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

//! Farukon_2_0/src/backtest.rs
//! Core backtesting engine that orchestrates the event-driven trading loop.
//! Coordinates data loading, strategy execution, portfolio updates, and order simulation.
//! Uses a publish-subscribe pattern via channels to decouple components.

use anyhow::Context;

use crate::strategy_loader;

/// Main backtesting controller.
/// Manages the lifecycle of a single strategy backtest.
/// Integrates all components: data handler, dynamic strategy, portfolio, and execution engine.
pub struct Backtest {
    mode: String,   // Operational mode: "Debug", "Optimize", "Visual"
    strategy_settings: farukon_core::settings::StrategySettings,    // Strategy-specific config
    strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,   // Metadata for all traded instruments
    data_handler: Box<dyn farukon_core::data_handler::DataHandler>, // Source of market data (FlatBuffers or CSV)
    event_receiver: std::sync::mpsc::Receiver<Box<dyn farukon_core::event::Event>>, // Channel to receive events from strategy
    dynamic_strategy: Box<strategy_loader::DynamicStratagy>,    // Dynamically loaded strategy library
    portfolio: Box<dyn farukon_core::portfolio::PortfolioHandler>,  // Manages positions, equity, and risk
    execution_handler: Box<dyn farukon_core::execution::ExecutionHandler>,  // Simulates order execution with slippage/commission
}

impl Backtest {
    /// Constructs a new Backtest instance with all required components.
    /// This is the entry point for a single strategy backtest.
    /// # Arguments
    /// * `mode` - Controls verbosity and behavior (Debug/Optimize/Visual)
    /// * `strategy_settings` - Configuration for this specific strategy
    /// * `strategy_instruments_info` - Metadata for all instruments traded by this strategy
    /// * `data_handler` - Abstract interface to market data
    /// * `event_receiver` - Receiver end of the event channel (events are sent by strategy)
    /// * `dynamic_strategy` - Loaded dynamic library implementing the trading logic
    /// * `portfolio` - Handles position tracking, equity, and margin
    /// * `execution_handler` - Simulates market execution (fills, slippage, commission)
    pub fn new(
        mode: &String,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        data_handler: Box<dyn farukon_core::data_handler::DataHandler>,
        event_receiver: std::sync::mpsc::Receiver<Box<dyn farukon_core::event::Event>>,
        dynamic_strategy: Box<strategy_loader::DynamicStratagy>,
        portfolio: Box<dyn farukon_core::portfolio::PortfolioHandler>,
        execution_handler: Box<dyn farukon_core::execution::ExecutionHandler>,
    ) -> Self {
        Backtest {
            mode: mode.to_string(),
            strategy_settings: strategy_settings.clone(),
            strategy_instruments_info: strategy_instruments_info.clone(),
            data_handler,
            event_receiver,
            dynamic_strategy,
            portfolio,
            execution_handler,
        }
    }

    fn process_pending_events(&mut self) -> anyhow::Result<()> {
        loop {
            match self.event_receiver.try_recv() {
                Ok(event_box) => {
                    match event_box.event_type() {
                        "MARKET" => {
                            // Debug: Print current state before strategy runs
                            if self.mode == "Debug".to_string() {
                                print!("Start event, {:?}, ", event_box);
                                for symbol in &self.strategy_settings.symbols {
                                    print!("{}, {:?}, ", symbol, self.data_handler.get_latest_bar(symbol))
                                }
                                println!();
                                println!("Start_all position, {:?}", self.portfolio.get_all_positions());
                                println!("Start_all holdings, {:?}", self.portfolio.get_all_holdings());
                                // println!("All equity, {:?}", self.portfolio.get_all_equity_points())
                            }

                            // // Debug: Print state after strategy and portfolio update
                            // if self.mode == "Debug".to_string() {
                            //     print!("Finish event, {:?}, ", event_box);
                            //     for symbol in &self.strategy_settings.symbols {
                            //         print!("{}, {:?}, ", symbol, self.data_handler.get_latest_bar(symbol))
                            //     }
                            //     println!()
                            // }
                        }
                        "SIGNAL" => {
                            if self.mode == "Debug".to_string() {
                                println!("Start event, {:?}, ", event_box);
                            }

                            // Signal → create order
                            self.portfolio.update_signal(
                                event_box.get_signal_event_params().unwrap(),
                                &self.data_handler,
                            );
                            
                            if self.mode == "Debug".to_string() {
                                println!("Finish event, {:?}, ", event_box);
                            }
                        }
                        "ORDER" => {
                            if self.mode == "Debug".to_string() {
                                println!("Start event, {:?}, ", event_box);
                            }

                            // Order → simulate execution (slippage, commission)
                            self.execution_handler.execute_order(
                                event_box.get_order_event_params().unwrap(),
                                &self.strategy_instruments_info,
                                &self.strategy_settings,
                                &*self.data_handler
                            )?;

                            if self.mode == "Debug".to_string() {
                                println!("Finish event, {:?}, ", event_box);
                            }
                        }
                        "FILL" => {
                            if self.mode == "Debug".to_string() {
                                println!("Start event, {:?}, ", event_box);
                            }
                            
                            self.portfolio.update_fill(
                                event_box.get_fill_event_params().unwrap(),
                                &self.data_handler,
                            );

                            if self.mode == "Debug".to_string() {
                                println!("Finish event, {:?}, ", event_box);
                            }
                        }
                        _ => {
                            println!("Received unknown event type: {}", event_box.event_type());
                        }
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    eprintln!("Event channel disconnected.");
                    self.data_handler.set_continue_backtest(false);
                    break;
                }
            }
        }

        anyhow::Ok(())
    }

    /// The core event loop: processes market data updates and strategy events.
    /// Runs until data is exhausted or a stop condition is triggered.
    /// Events are processed in FIFO order:
    ///   1. MARKET: New bar arrives → trigger strategy → generate signals → send orders → receive fills
    ///   2. SIGNAL: Strategy signals an intent to trade → create order
    ///   3. ORDER: Order sent to execution → simulate fill
    ///   4. FILL: Fill received → update portfolio
    /// In Debug mode, prints detailed state for every event.
    /// On negative capital, stops backtest immediately.
    fn run_backtest(&mut self) -> anyhow::Result<()> {
        // let symbol_list = &self.strategy_settings.symbols;
        // let mut fill_flag: Option<Box<dyn farukon_core::event::Event>> = None;

        loop {
            // Advance data: load next bar for all symbols
            if self.data_handler.get_continue_backtest() {
                self.data_handler.update_bars();

            } else { break; }

            self.process_pending_events()?;

            // Run strategy logic on new market data
            if let Some(latest_holdings) = self.portfolio.get_latest_holdings() {
                if let Err(e) = self.dynamic_strategy.calculate_signals(
                    &*self.data_handler,
                    self.portfolio.get_current_positions(),
                    latest_holdings,
                    &self.strategy_settings.symbols,
                ) {
                    eprintln!("Error in Strategy::calculate_signals: {}", e);
                    self.data_handler.set_continue_backtest(false);
                    break;
                }
            }

            // Update portfolio time index (equity, positions, holdings)
            self.portfolio.update_timeindex(&self.data_handler);

            // Risk check: stop if capital becomes negative
            if let Some(holdings) = self.portfolio.get_latest_holdings() {
                if holdings.capital < 0.0 {
                    self.data_handler.set_continue_backtest(false);
                    println!("STOP BACKTEST DUE TO NEGATIVE CAPITAL!");
                }
            }

            if self.mode == "Debug".to_string() {
                for symbol in &self.strategy_settings.symbols {
                    print!("Finish_loop {}, {:?}, ", symbol, self.data_handler.get_latest_bar(symbol))
                }
                println!();
                println!("Finish_all position, {:?}", self.portfolio.get_all_positions());
                println!("Finish_all holdings, {:?}", self.portfolio.get_all_holdings());
                // println!("All equity, {:?}", self.portfolio.get_all_equity_points())
            }

            // Debug separator
            if self.mode == "Debug".to_string() {
                println!("++++++++++++++++++++++++++++++++++++++");
            }

            // Real-time mode: simulate live trading delay
            if self.mode == "RealTime" {
                let heartbeat = 0.0;
                std::thread::sleep(std::time::Duration::from_secs_f64(heartbeat));
            }
        }
        
        anyhow::Ok(())
    }

    /// Calculates final performance metrics after backtest completes.
    /// Calls Portfolio::calculate_final_performance() to compute all metrics offline.
    /// Returns the final PerformanceMetrics object.
    fn output_performance(&mut self) -> anyhow::Result<&farukon_core::performance::PerformanceMetrics> {
        self.portfolio.calculate_final_performance();

        match self.portfolio.output_summary_stats() {
            Ok(stats) => {
                anyhow::Ok(stats)
            }
            Err(e) => {
                eprintln!("Error generating performance summary stats: {}", e);
                Err(e)
            }
        }
    }

    /// Public API: Runs the entire backtest and returns performance metrics.
    /// # Returns
    /// * `Ok(&PerformanceMetrics)` on success
    /// * `Err(anyhow::Error)` if backtest or performance calculation fails
    pub fn simulate_trading(&mut self) -> anyhow::Result<&farukon_core::performance::PerformanceMetrics> {
        if self.mode == "Debug" {
            println!("Starting backtest simulation...");
        }

        self.run_backtest()
            .context("Backtest simulation failed")?;

        if self.mode == "Debug" {
            println!("all_positions: {:#?}", self.portfolio.get_all_positions());
            println!("all_holdings: {:#?}", self.portfolio.get_all_holdings());
            // println!("all_equity: {:#?}", self.portfolio.get_all_equity_points());
        }
        
        let result = self.output_performance()
            .context("Failed to output performance")?;

        anyhow::Ok(result)
    }

}

// Farukon_2_0/src/portfolio.rs

//! Portfolio manager: tracks positions, holdings, equity, and risk.
//! Implements PortfolioHandler trait for integration with Backtest.
//! Handles fill events, signal events, and margin calls.

use farukon_core::{self, portfolio::PortfolioHandler};

use crate::risks;

pub struct Portfolio {
    /// Operational mode (Debug, Optimize, Visual).
    mode: String,
    initial_capital_for_strategy: f64,
    /// Event sender for communicating with other components.
    event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    /// Strategy settings for this portfolio.
    strategy_settings: farukon_core::settings::StrategySettings,
    /// Instrument metadata for all traded instruments.
    strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    /// Current position state for each symbol.
    current_positions: std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
    /// Current holding state for each symbol.
    current_holdings: std::collections::HashMap<String, farukon_core::portfolio::HoldingsState>,
    /// Historical snapshots of positions.
    all_positions: Vec<farukon_core::portfolio::PositionSnapshot>,
    /// Historical snapshots of holdings.
    all_holdings: Vec<farukon_core::portfolio::HoldingSnapshot>,
    /// Equity curve for plotting.
    equity_series: Vec<(chrono::DateTime<chrono::Utc>, f64)>,
    /// Performance manager for calculating metrics.
    performance_manager: farukon_core::performance::PerformanceManager,
}

impl Portfolio {
    /// Creates a new Portfolio instance.
    /// # Arguments
    /// * `mode` - Operational mode.
    /// * `event_sender` - Sender for events.
    /// * `strategy_settings` - Strategy configuration.
    /// * `strategy_instruments_info` - Instrument metadata.
    /// * `initial_capital_for_strategy` - Starting capital for this strategy.
    pub fn new(
        mode: &String,
        initial_capital_for_strategy: &f64,
        event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    ) -> anyhow::Result<Self> {
        anyhow::Ok(
            Self {
                mode: mode.to_string(),
                initial_capital_for_strategy: *initial_capital_for_strategy,
                event_sender,
                strategy_settings: strategy_settings.clone(),
                strategy_instruments_info: strategy_instruments_info.clone(),
                current_positions: Self::construct_current_positions(strategy_settings),
                current_holdings: Self::construct_current_holdings(strategy_settings),
                all_positions: Vec::new(),
                all_holdings: Vec::new(),
                equity_series: Vec::new(),
                performance_manager: farukon_core::performance::PerformanceManager::new(*initial_capital_for_strategy, &strategy_settings),
            }
        )
    }

    /// Constructs the initial position state for each symbol in the strategy.
    fn construct_current_positions(
        strategy_settings: &farukon_core::settings::StrategySettings,
    ) -> std::collections::HashMap<String, farukon_core::portfolio::PositionState> {
        // Initializes empty position state for each symbol in strategy.

        let mut positions: std::collections::HashMap<String, farukon_core::portfolio::PositionState> = std::collections::HashMap::new();
        for symbol in &strategy_settings.symbols {
            let current_positions_for_symbol: farukon_core::portfolio::PositionState = farukon_core::portfolio::PositionState::new();
            positions.insert(symbol.clone(), current_positions_for_symbol);
        }
        positions
    }

    /// Constructs the initial holding state for each symbol in the strategy.
    fn construct_current_holdings(
        strategy_settings: &farukon_core::settings::StrategySettings,
    ) -> std::collections::HashMap<String, farukon_core::portfolio::HoldingsState> {
        // Initializes empty holding state for each symbol.

        let mut holdings: std::collections::HashMap<String, farukon_core::portfolio::HoldingsState> = std::collections::HashMap::new();
        for symbol in &strategy_settings.symbols {
            let current_holdings_for_symbol: farukon_core::portfolio::HoldingsState = farukon_core::portfolio::HoldingsState::new();
            holdings.insert(symbol.clone(), current_holdings_for_symbol);
        }
        holdings
    }

    /// Generates an order event from a signal event.
    /// Applies position sizing and margin control.
    /// # Arguments
    /// * `signal_event` - The signal event received from the strategy.
    /// * `data_handler` - The data handler for accessing market data.
    /// # Returns
    /// * An optional OrderEvent if the signal should be executed.
    fn generate_order(
        &self,
        signal_event: &farukon_core::event::SignalEvent,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>,
    ) -> Option<farukon_core::event::OrderEvent> {
            // Converts a SIGNAL event into an ORDER event.
            // Uses position sizer to determine quantity.
            // Applies margin control to prevent over-leverage.

            let signal_name = &signal_event.signal_name;
            let symbol = &signal_event.symbol;
            let cur_quantity = self.get_current_positions().get(&symbol.clone()).unwrap().position;
            let cash = self.get_latest_holdings().unwrap().cash;
            let current_datetime = data_handler.get_latest_bar_datetime(symbol).unwrap();
            let order_type = &signal_event.order_type;
            let limit_price = signal_event.limit_price;

            let mut quantity = signal_event.quantity.unwrap().abs();
            let mut direction = None;

            if signal_name != "EXIT" && cur_quantity == 0.0 && cash > 0.0 {
                quantity = farukon_core::utils::calculate_max_available_quantity(
                    cash,
                    quantity,
                    self.strategy_instruments_info.get(symbol).unwrap());
                
                if signal_name == "LONG"  {
                direction = Some("BUY".to_string());
                } else if signal_name == "SHORT" {
                direction = Some("SELL".to_string());
            }
            } else {
                if cur_quantity > 0.0 {
                direction = Some("SELL".to_string());
                } else if cur_quantity < 0.0 {
                    direction = Some("BUY".to_string());
            }
            }

            if risks::margin_call_control_for_signal(
                quantity,
                self.get_latest_holdings().unwrap(),
                signal_event, self.strategy_instruments_info.get(symbol).unwrap(),
            ).ok()? {
                let order = Some(farukon_core::event::OrderEvent::new(
                    current_datetime,
                    symbol.to_string(),
                    order_type.to_string(),
                    quantity,
                    direction,
                    signal_name.to_string(),
                    limit_price,
                ));

                return order;
            }

            None
        }

}

impl farukon_core::portfolio::PortfolioHandler for Portfolio {
    /// Updates position state based on a fill event.
    /// # Arguments
    /// * `fill_event` - The fill event received from the execution engine.
    /// * `data_handler` - The data handler for accessing market data.
    fn update_positions_from_fill(
        &mut self,
        fill_event: &farukon_core::event::FillEvent,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>
    ) {
        // Updates position state on FILL.
        // Adjusts position size, records entry/exit prices.

        let timeindex = data_handler.get_latest_bar_datetime(&fill_event.symbol);
        if self.mode == "Debug".to_string() {
            println!("for Fill event, start Current_positions, {:?}, {:?}", timeindex, self.current_positions);
        }

        let fill_dir = match fill_event.direction.as_deref() {
            Some("BUY") => 1.0,
            Some("SELL") => -1.0,
            _ => {
                eprintln!("Unknown fill direction: {}", fill_event.direction.clone().unwrap());
                0.0
            },
        };
        let symbol = &fill_event.symbol;
        let quantity = fill_event.quantity;
        let signal_name = fill_event.signal_name.as_str();
        let current_cash  = self.get_latest_holdings().unwrap().cash;

        if let Some(position_state) = self.current_positions.get_mut(symbol) {
            position_state.position += fill_dir * quantity;

            match signal_name {
                "EXIT" => {
                    position_state.entry_price = None;
                    position_state.entry_capital = 0.0;
                },
                _ => {
                    position_state.deal_number += 1;
                    position_state.entry_price = fill_event.execution_price;
                    position_state.entry_capital = current_cash;
                },
            }
        }

        if self.mode == "Debug".to_string() {
            println!("for Fill event, finish Current_positions, {:?}, {:?}", timeindex, self.current_positions);
        }
    }

    /// Updates holding state based on a fill event.
    /// # Arguments
    /// * `fill_event` - The fill event received from the execution engine.
    /// * `data_handler` - The data handler for accessing market data.
    fn update_holdings_from_fill(
        &mut self,
        fill_event: &farukon_core::event::FillEvent,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>,
    ) {
        // Updates PnL and blocked margin on FILL.
        // Uses step_price and step to convert price to currency value.

        let timeindex = data_handler.get_latest_bar_datetime(&fill_event.symbol);
        if self.mode == "Debug".to_string() {
            println!("for Fill event, start Current_holdings, {:?}, {:?}", timeindex, self.current_holdings);
        }
        
        let quantity = fill_event.quantity;
        let symbol = &fill_event.symbol;
        let commission = fill_event.commission.unwrap();
        let signal_name = fill_event.signal_name.as_str();
        let direction = fill_event.direction.clone().unwrap();
        let execution_price = fill_event.execution_price.unwrap_or(0.0);
        let close = data_handler.get_latest_bar_value(symbol, "close").unwrap_or(0.0);
        let last_close = data_handler.get_latest_bars_values(symbol, "close", 2)[0];

        let strategy_instrument_info_for_symbol = self.strategy_instruments_info.get(symbol).unwrap();
        let step_price = strategy_instrument_info_for_symbol.step_price;
        let step = strategy_instrument_info_for_symbol.step;
        let cost_of_step_price = ((step_price / step) * 100_000.0).round() / 100_000.0;
      
        self.current_holdings.get_mut(symbol).unwrap().signal_name = Some(fill_event.signal_name.clone());
        
        match signal_name {
            "EXIT" => {
                match  direction.as_str() {
                    "BUY" => {
                        self.current_holdings.get_mut(symbol).unwrap().pnl = ((((last_close - execution_price) * cost_of_step_price) * quantity * 100.0).round() / 100.0) - commission;
                    },
                    "SELL" => {
                        self.current_holdings.get_mut(symbol).unwrap().pnl = ((((execution_price - last_close) * cost_of_step_price) * quantity * 100.0).round() / 100.0) - commission;
                    },
                    _ => {
                        eprintln!("Unknown fill direction: {:?}", fill_event.direction);
                        return;
                    }
                }

                match self.strategy_instruments_info.get(symbol).unwrap().instrument_type.as_str() {
                    "futures" => {
                        self.current_holdings.get_mut(symbol).unwrap().blocked -= strategy_instrument_info_for_symbol.margin * quantity;
                    }
                    _ => {
                        eprintln!("Unknown type of instrument!");
                    }
                }
            },
            _ => {
                match direction.as_str() {
                    "BUY" => {
                        self.current_holdings.get_mut(symbol).unwrap().pnl = ((((close - execution_price) * cost_of_step_price) * quantity * 100.0).round() / 100.0) - commission;
                    },
                    "SELL" => {
                        self.current_holdings.get_mut(symbol).unwrap().pnl = ((((execution_price - close) * cost_of_step_price) * quantity * 100.0).round() / 100.0) - commission;
                    },
                    _ => {
                        eprintln!("Unknown fill direction: {:?}", fill_event.direction);
                        return;
                    }
                }

                match self.strategy_instruments_info.get(symbol).unwrap().instrument_type.as_str() {
                    "futures" => {
                        self.current_holdings.get_mut(symbol).unwrap().blocked += strategy_instrument_info_for_symbol.margin * quantity;
                    }
                    _ => {
                        eprintln!("Unknown type of instrument!");
                    }
                }
            }
        }

        if self.mode == "Debug".to_string() {
            println!("for Fill event, finish Current_holdings, {:?}, {:?}", timeindex, self.current_holdings);
        }
    }

    /// Updates the portfolio state based on a fill event.
    /// # Arguments
    /// * `fill_event` - The fill event received from the execution engine.
    /// * `data_handler` - The data handler for accessing market data.
    fn update_fill(
        &mut self,
        fill_event: &farukon_core::event::FillEvent,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>,
    ) {
        self.update_positions_from_fill(fill_event, data_handler);
        self.update_holdings_from_fill(fill_event, data_handler);
    }

    /// Updates the portfolio state based on a fill event.
    /// # Arguments
    /// * `fill_event` - The fill event received from the execution engine.
    /// * `data_handler` - The data handler for accessing market data.
    fn update_timeindex(
        &mut self,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>,
    ) {
        // Called on every MARKET event to update equity curve, snapshots, and metrics.
        // Also triggers margin call monitoring.

        let current_bar_datetime = data_handler.get_latest_bar_datetime(
            &self.strategy_settings.symbols[0]
            ).unwrap();

        if self.mode == "Debug".to_string() {
            println!(
                "Start event, Update timeindex, {}, {:?}, {:?}",
                current_bar_datetime,
                self.current_positions,
                self.current_holdings,
            );
        }

        // Update all_positions snapshot
        {
            if self.all_positions.len() < 2 {
                self.all_positions.push(farukon_core::portfolio::PositionSnapshot::new(
                    current_bar_datetime,
                    self.current_positions.clone(),
                ));
            } else {
                if let Some(last) = self.all_positions.last_mut() {
                    *last = farukon_core::portfolio::PositionSnapshot::new(
                        current_bar_datetime,
                        self.current_positions.clone(),
                    );
                }
            }
        }

        // Update unrealized PnL for open positions
        {
            for symbol in &self.strategy_settings.symbols {
                let close = match data_handler.get_latest_bar_value(symbol, "close") {
                    Some(value) if value.is_nan() => 0.0,
                    Some(value) => value,
                    None => 0.0,
                };

                let last_close = match data_handler.get_latest_bars_values(symbol, "close", 2).first() {
                    Some(value) if value.is_nan() => 0.0,
                    Some(value) => *value,
                    None => 0.0,
                };

                let strategy_instrument_info_for_symbol = self.strategy_instruments_info.get(symbol).unwrap();
                let step_price = strategy_instrument_info_for_symbol.step_price;
                let step = strategy_instrument_info_for_symbol.step;
                let cost_of_step_price = ((step_price / step) * 100_000.0).round() / 100_000.0;
                
                if let Some(holdings_state) = self.current_holdings.get_mut(symbol) {
                    if let Some(position_state) = self.current_positions.get(symbol) {
                        if holdings_state.signal_name != None {
                            holdings_state.signal_name = None;
                        } else {
                            holdings_state.pnl = (((close - last_close) * cost_of_step_price) * position_state.position * 100.0).round() / 100.0;
                        }
                    }
                }
            }
        }

        // Update all_holdings snapshot
        {
            if self.all_holdings.len() == 0 {
                let blocked = 0.0;
                self.all_holdings.push(farukon_core::portfolio::HoldingSnapshot::new(
                    current_bar_datetime,
                    self.initial_capital_for_strategy,
                    self.initial_capital_for_strategy,
                    blocked, 
                    self.current_holdings.clone(),
                ));
            } else if self.all_holdings.len() == 1 {
                if let Some(first) = self.all_holdings.first_mut() {
                    let blocked: f64 = self.current_holdings.iter()
                        .map(|a| a.1.blocked)
                        .sum();
    
                    let pnl: f64 = self.current_holdings.iter()
                        .map(|a| a.1.pnl)
                        .sum();
    
                    let capital = first.capital + pnl;
                    let cash = capital - blocked;

                    self.all_holdings.push(farukon_core::portfolio::HoldingSnapshot::new(
                        current_bar_datetime,
                        capital,
                        cash,
                        blocked,
                        self.current_holdings.clone(),
                    ));
                }
            } else {
                if let Some(last) = self.all_holdings.last_mut() {
                    let blocked: f64 = self.current_holdings.iter()
                        .map(|a| a.1.blocked)
                        .sum();
    
                    let pnl: f64 = self.current_holdings.iter()
                        .map(|a| a.1.pnl)
                        .sum();
    
                    let capital = last.capital + pnl;
                    let cash = capital - blocked;
    
                    *last = farukon_core::portfolio::HoldingSnapshot::new(
                        current_bar_datetime,
                        capital,
                        cash,
                        blocked,
                        self.current_holdings.clone(),
                    );
                }
            }
        }

        // Udate equity curve data
        {
            if let Some(latest_holdings) = self.get_latest_holdings() {
                self.equity_series.push((current_bar_datetime, latest_holdings.capital));
            }
        }
        
        // Update metrics incrementally if in RealTime mode
        {
            let start_date = self.get_all_holdings().first().unwrap().datetime;
            let end_date = data_handler.get_latest_bar_datetime(
                self.strategy_settings.symbols.first().unwrap()
            ).unwrap();

            // Update deals counter
            let mut deals_count = 0 as usize;
            for symbol in &self.strategy_settings.symbols {
                if let Some(position_state) = self.current_positions.get_mut(symbol) {
                    deals_count += position_state.deal_number;
                }
            }
    
            if let farukon_core::settings::MetricsMode::RealTime { .. } = self.strategy_settings.portfolio_settings_for_strategy.metrics_calculation_mode {
                if let Some(latest_holdings) = self.get_latest_holdings() {
                    self.performance_manager.update_incremental(latest_holdings.capital, start_date, end_date, deals_count);
                    
                    if self.mode == "Debug".to_string() {
                        println!(
                            "Metrics, {:?}",
                            self.performance_manager.get_current_performance_metrics()
                        );
                    }
                }
            }
        }
        
        // Margin call monitoring: if capital falls below min_margin, close all positions
        {
            let margin_call_monitoring = risks::margin_call_control_for_market(
                self.get_latest_holdings().unwrap(),
                self.get_current_positions(),
                &self.strategy_settings,
                &self.strategy_instruments_info
            ).unwrap();
            if !margin_call_monitoring {
                for symbol in &self.strategy_settings.symbols {
                    println!("{:?}", margin_call_monitoring);
                    let quantity = Some(self.get_current_positions().get(symbol).unwrap().position);
                    let _ = self.event_sender.send(Box::new(farukon_core::event::SignalEvent::new(
                current_bar_datetime,
                        symbol.clone(),
                        "EXIT".to_string(),
                        "MKT".to_string(),
                        quantity,
                        None,
                    )));
                }
            }
        }

        if self.mode == "Debug".to_string() {
            println!(
                "Finish event, Update timeindex, {}, {:?}, {:?}",
                current_bar_datetime,
                self.current_positions,
                self.current_holdings,
            );
        }
    }

    /// Updates the portfolio state based on a signal event.
    /// # Arguments
    /// * `signal_event` - The signal event received from the strategy.
    /// * `data_handler` - The data handler for accessing market data.
    fn update_signal(
        &mut self,
        signal_event: &farukon_core::event::SignalEvent,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>,
    ) {
        // Converts SIGNAL to ORDER and sends to execution engine.
        if self.mode == "Debug".to_string() {
            println!("for Signal event, Current_positions, {}, {:?}", data_handler.get_latest_bar_datetime(&signal_event.symbol).unwrap(), self.current_positions);
            println!("for Signal event, Current_holdings, {}, {:?}", data_handler.get_latest_bar_datetime(&signal_event.symbol).unwrap(), self.current_holdings);
            println!("for Signal event, latest_holdings, {}, {:?}", data_handler.get_latest_bar_datetime(&signal_event.symbol).unwrap(), self.get_latest_holdings());
        }

        if let Some(order) = self.generate_order(signal_event, data_handler) {
            match self.event_sender.send(Box::new(order)) {
                Ok(()) => {},
                Err(e) => eprintln!("Failed to send OrderEvent: {}", e),
            }
        }
    }

    /// Returns a summary of the final performance metrics.
    /// # Returns
    /// * `anyhow::Result<&PerformanceMetrics>` containing the final metrics.
    fn output_summary_stats(&self) -> anyhow::Result<&farukon_core::performance::PerformanceMetrics> {
        // Returns final performance metrics after backtest.
        if self.mode == "Debug".to_string() {
            println!("{:#?}", self.equity_series);
        }

        // self.export_results(); TODO
        if self.mode == "Debug" {
            farukon_core::utils::export_equity_to_csv(&self.equity_series, &self.strategy_settings)?;
        }

        let output_metrics = self.performance_manager.get_current_performance_metrics();
              
        anyhow::Ok(output_metrics)
    }

    /// Calculates final performance metrics after the backtest ends.
    fn calculate_final_performance(&mut self) {
        // Called after backtest ends to compute offline metrics.
        // Uses full equity curve for accurate drawdown and return calculations.

        if let Some(holdings) = self.get_all_holdings().first() {
            if let farukon_core::settings::MetricsMode::Offline = self.strategy_settings.portfolio_settings_for_strategy.metrics_calculation_mode {
                let equity_series = self.get_equity_capital_values();
                let start_date = holdings.datetime;
                let end_date = self.get_latest_holdings().unwrap().datetime;
                
                let deals_count: usize = self.get_all_positions()
                    .last()
                    .map(|snapshot| {
                        self.strategy_settings.symbols
                            .iter()
                            .filter_map(|symbol| snapshot.positions.get(symbol))
                            .map(|position_state| position_state.deal_number)
                            .sum()
                    })
                    .unwrap_or(0);
    
                self.performance_manager.calculate_final(
                    &equity_series,
                    start_date,
                    end_date,
                    deals_count,
                );
            }
        }

    }

    // Getters
    /// Returns a reference to the current positions.
    fn get_current_positions(&self) -> &std::collections::HashMap<String, farukon_core::portfolio::PositionState> {
        &self.current_positions
    }

    /// Returns a reference to all historical position snapshots.
    fn get_all_positions(&self) -> &Vec<farukon_core::portfolio::PositionSnapshot> {
        &self.all_positions
    }

    /// Returns a reference to the current holdings.
    fn get_current_holdings(&self) -> &std::collections::HashMap<String, farukon_core::portfolio::HoldingsState> {
        &self.current_holdings
    }

    /// Returns a reference to all historical holding snapshots.
    fn get_all_holdings(&self) -> &Vec<farukon_core::portfolio::HoldingSnapshot> {
        &self.all_holdings
    }

    /// Returns a reference to the latest equity snapshot.
    fn get_latest_holdings(&self) -> Option<&farukon_core::portfolio::HoldingSnapshot> {
        self.all_holdings.last()
    }

    /// Returns a vector of all capital values from the equity curve.
    fn get_equity_capital_values(&self) -> Vec<f64> {
        // self.all_holdings.iter().map(|point| point.capital).collect()
        self.equity_series
            .iter()
            .map(|a| a.1)
            .collect()
    }

}

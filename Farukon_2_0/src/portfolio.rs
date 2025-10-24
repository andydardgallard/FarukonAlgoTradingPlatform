// Farukon_2_0/src/portfolio.rs

use farukon_core::{self, portfolio::PortfolioHandler};

use crate::risks;
use std::io::Write;

pub struct Portfolio {
    mode: String,
    event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    strategy_settings: farukon_core::settings::StrategySettings,
    strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    current_positions: std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
    current_holdings: std::collections::HashMap<String, farukon_core::portfolio::HoldingsState>,
    current_equity_point: farukon_core::portfolio::EquityPoint,
    all_positions: Vec<farukon_core::portfolio::PositionSnapshot>,
    all_holdings: Vec<farukon_core::portfolio::HoldingSnapshot>,
    all_equity_points: Vec<farukon_core::portfolio::EquitySnapshot>,
    equity_series: Vec<(chrono::DateTime<chrono::Utc>, f64)>,
    performance_manager: farukon_core::performance::PerformanceManager,
}

impl Portfolio {
    pub fn new(
        mode: &String,
        event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        initial_capital_for_strategy: &f64,
    ) -> anyhow::Result<Self> {
        anyhow::Ok(
            Self {
            mode: mode.to_string(),
            event_sender,
            strategy_settings: strategy_settings.clone(),
                strategy_instruments_info: strategy_instruments_info.clone(),
            current_positions: Self::construct_current_positions(strategy_settings),
                current_holdings: Self::construct_current_holdings(strategy_settings),
                current_equity_point: farukon_core::portfolio::EquityPoint::default(*initial_capital_for_strategy),
            all_positions: Vec::new(),
            all_holdings: Vec::new(),
                all_equity_points: Vec::new(),
                equity_series: Vec::new(),
                performance_manager: farukon_core::performance::PerformanceManager::new(*initial_capital_for_strategy, &strategy_settings),
            }
        )
    }

    fn construct_current_positions(
        strategy_settings: &farukon_core::settings::StrategySettings,
    ) -> std::collections::HashMap<String, farukon_core::portfolio::PositionState> {
        let mut positions: std::collections::HashMap<String, farukon_core::portfolio::PositionState> = std::collections::HashMap::new();

        for symbol in &strategy_settings.symbols {
            let current_positions_for_symbol: farukon_core::portfolio::PositionState = farukon_core::portfolio::PositionState::new();
            positions.insert(symbol.clone(), current_positions_for_symbol);
        }
        positions
    }

    fn construct_current_holdings(
        strategy_settings: &farukon_core::settings::StrategySettings,
    ) -> std::collections::HashMap<String, farukon_core::portfolio::HoldingsState> {
        let mut holdings: std::collections::HashMap<String, farukon_core::portfolio::HoldingsState> = std::collections::HashMap::new();

        for symbol in &strategy_settings.symbols {
            let current_holdings_for_symbol: farukon_core::portfolio::HoldingsState = farukon_core::portfolio::HoldingsState::new();
            holdings.insert(symbol.clone(), current_holdings_for_symbol);
        }
        holdings
    }

    fn generate_order(
        &self,
        signal_event: &farukon_core::event::SignalEvent,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>,
    ) -> Option<farukon_core::event::OrderEvent> {
            let signal_name = &signal_event.signal_name;
            let symbol = &signal_event.symbol;
            let cur_quantity = self.get_current_positions().get(&symbol.clone()).unwrap().position;
            let cash = self.get_latest_equity_point().unwrap().equity_point.cash;
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
                self.get_latest_equity_point().unwrap(),
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

    #[allow(dead_code)] // TODO
    pub fn export_equity_to_csv(&self, filename: &str) -> anyhow::Result<()> {
        let mut file = std::fs::File::create(filename)?;
        writeln!(file, "datetime,capital")?;
        for point in &self.equity_series {
            writeln!(file, "{},{}", point.0.format("%Y-%m-%d %H:%M:%S"), point.1)?;
        }

        anyhow::Ok(())
    }

}

impl farukon_core::portfolio::PortfolioHandler for Portfolio {
    fn update_positions_from_fill(
        &mut self,
        fill_event: &farukon_core::event::FillEvent,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>
    ) {
        let timeindex = data_handler.get_latest_bar_datetime(&fill_event.symbol);

        if self.mode == "Debug".to_string() {
            println!("Start event, Current_positions, {:?}, {:?}", timeindex, self.current_positions);
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
        let current_cash  = self.get_latest_equity_point().unwrap().equity_point.cash;

        if let Some(position_state) = self.current_positions.get_mut(symbol) {
            position_state.position += fill_dir * quantity;

            match signal_name {
                "EXIT" => {
                    position_state.exit_price = fill_event.execution_price;
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
            println!("Finish event, Current_positions, {:?}, {:?}", timeindex, self.current_positions);
        }
    }

    fn update_holdings_from_fill(
        &mut self,
        fill_event: &farukon_core::event::FillEvent,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>,
    ) {
        let timeindex = data_handler.get_latest_bar_datetime(&fill_event.symbol);
        if self.mode == "Debug".to_string() {
            println!("Start event, Current_holdings, {:?}, {:?}", timeindex, self.current_holdings);
        }
        
        let quantity = fill_event.quantity;
        let symbol = &fill_event.symbol;
        let commission = fill_event.commission.unwrap();
        let signal_name = fill_event.signal_name.as_str();
        let direction = fill_event.direction.clone().unwrap();
        let execution_price = fill_event.execution_price.unwrap();
        let close = data_handler.get_latest_bar_value(symbol, "close").unwrap();
        let last_close = data_handler.get_latest_bars_values(symbol, "close", 2)[0];

        let strategy_instrument_info_for_symbol = self.strategy_instruments_info.get(symbol).unwrap();
        let step_price = strategy_instrument_info_for_symbol.step_price;
        let step = strategy_instrument_info_for_symbol.step;
        let cost_of_step_price = ((step_price / step) * 100_000.0).round() / 100_000.0;
      
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
                self.current_holdings.get_mut(symbol).unwrap().blocked -= strategy_instrument_info_for_symbol.margin * quantity;
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
                self.current_holdings.get_mut(symbol).unwrap().blocked += strategy_instrument_info_for_symbol.margin * quantity;
            }
        }
        if self.mode == "Debug".to_string() {
            println!("Finish event, Current_holdings, {:?}, {:?}", timeindex, self.current_holdings);
        }
    }

    fn update_fill(
        &mut self,
        fill_event: &farukon_core::event::FillEvent,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>,
    ) {
        self.update_positions_from_fill(fill_event, data_handler);
        self.update_holdings_from_fill(fill_event, data_handler);
    }

    fn update_timeindex(
        &mut self,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>,
    ) {
        let positions_snapshot_data = self.current_positions.clone();
        let mut holdings_snapshot_data = self.current_holdings.clone();
        let mut equity_snapshot_data = self.current_equity_point.clone();

        let current_bar_datetime = data_handler.get_latest_bar_datetime(
            &self.strategy_settings.symbols[0]
            ).unwrap();

        if self.mode == "Debug".to_string() {
            println!(
                "Start event, Update timeindex, {}, {:?}, {:?}, {:?}",
                current_bar_datetime,
                self.get_current_positions(),
                self.get_current_holdings(),
                self.get_current_equity_point(),
            );
        }

        // Update deals counter
        let mut deals_count = 0 as usize;
        for symbol in &self.strategy_settings.symbols {
            if let Some(position_state) = self.current_positions.get_mut(symbol) {
                if position_state.position == 0.0 {
                    position_state.exit_price = None;
                }
                deals_count += position_state.deal_number;
            }
        }

        // Update all_positions
        if self.all_positions.len() < 2 {
        self.all_positions.push(farukon_core::portfolio::PositionSnapshot::new(
            current_bar_datetime,
            positions_snapshot_data.clone(),
        ));
        } else {
            if let Some(last) = self.all_positions.last_mut() {
                *last = farukon_core::portfolio::PositionSnapshot::new(
                    current_bar_datetime,
                    positions_snapshot_data.clone(),
                );
            }
        }

        // Update all_holdings
        if self.all_holdings.len() < 2 {
            self.all_holdings.push(farukon_core::portfolio::HoldingSnapshot::new(
                current_bar_datetime,
                holdings_snapshot_data.clone(),
            ));
        } else {
            if let Some(last) = self.all_holdings.last_mut() {
                *last = farukon_core::portfolio::HoldingSnapshot::new(
                    current_bar_datetime,
                    holdings_snapshot_data.clone()
                );
            }
        }

        // construct current equity point
        if let Some(equity_point) = self.get_latest_equity_point() {
            let mut total_pnl = 0.0;
            let mut total_blocked = 0.0;
            let mut total_capital = equity_point.equity_point.capital;

            for symbol in &self.strategy_settings.symbols {
                if let Some(holdings_state) = holdings_snapshot_data.get(symbol) {
                    total_pnl += holdings_state.pnl;
                    total_blocked += holdings_state.blocked;
                }
            }
            total_capital += total_pnl;
            let total_cash = total_capital - total_blocked;

            equity_snapshot_data.capital = total_capital;
            equity_snapshot_data.blocked = total_blocked;
            equity_snapshot_data.cash = total_cash;
        }
        
        self.current_equity_point = equity_snapshot_data.clone();


        if self.all_equity_points.len() < 2 {
            self.all_equity_points.push(farukon_core::portfolio::EquitySnapshot::new(
                current_bar_datetime,
                equity_snapshot_data.clone(),
            ));
        } else {
            if let Some(last) = self.all_equity_points.last_mut() {
                *last = farukon_core::portfolio::EquitySnapshot::new(
                    current_bar_datetime,
                    equity_snapshot_data.clone()
                )
            }
        }

        // udate equity curve data
        self.equity_series.push((current_bar_datetime, equity_snapshot_data.capital));

        // update Metrics
        let start_date = self.get_all_holdings().first().unwrap().datetime;
        let end_date = data_handler.get_latest_bar_datetime(
            self.strategy_settings.symbols.first().unwrap()
        ).unwrap();
        
        if let farukon_core::settings::MetricsMode::RealTime { .. } = self.strategy_settings.portfolio_settings_for_strategy.metrics_calculation_mode {
            self.performance_manager.update_incremental(equity_snapshot_data.capital, start_date, end_date, deals_count);
            
            if self.mode == "Debug".to_string() {
                println!(
                    "Metrics, {:?}",
                    self.performance_manager.get_current_performance_metrics()
                );
            }
        }

        for symbol in &self.strategy_settings.symbols {
            let close = data_handler.get_latest_bar_value(symbol, "close").unwrap();
            let last_close = data_handler.get_latest_bars_values(symbol, "close", 2)[0];
            let strategy_instrument_info_for_symbol = self.strategy_instruments_info.get(symbol).unwrap();
            let step_price = strategy_instrument_info_for_symbol.step_price;
            let step = strategy_instrument_info_for_symbol.step;
            let cost_of_step_price = ((step_price / step) * 100_000.0).round() / 100_000.0;

            if let Some(holdings_state) = self.current_holdings.get_mut(symbol) {
                if let Some(position_state) = self.current_positions.get(symbol) {
                    if position_state.position == 0.0 {
                        holdings_state.pnl = 0.0;
                    }
                    else {
                        holdings_state.pnl = (((close - last_close) * cost_of_step_price) * position_state.position * 100.0).round() / 100.0;
                    }
                    holdings_snapshot_data.insert(symbol.clone(), holdings_state.clone());
                }
                
            }
        }

        // Close all positions if margin call
        let margin_call_monitoring = risks::margin_call_control_for_market(
            self.get_latest_equity_point().unwrap(),
            self.get_current_positions(),
            &self.strategy_settings,
            &self.strategy_instruments_info
        ).unwrap();
        if !margin_call_monitoring {
            for symbol in &self.strategy_settings.symbols {
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

        if self.mode == "Debug".to_string() {
            println!(
                "Finish event, Update timeindex, {}, {:?}, {:?}, {:?}",
                current_bar_datetime,
                positions_snapshot_data,
                holdings_snapshot_data,
                equity_snapshot_data,
            );
        }
    }

    fn update_signal(
        &mut self,
        signal_event: &farukon_core::event::SignalEvent,
        data_handler: &Box<dyn farukon_core::data_handler::DataHandler>,
    ) {
        if self.mode == "Debug".to_string() {
            println!("Start event, Current_positions, {}, {:?}", data_handler.get_latest_bar_datetime(&signal_event.symbol).unwrap(), self.current_positions);
        }

        if let Some(order) = self.generate_order(signal_event, data_handler) {
            match self.event_sender.send(Box::new(order)) {
                Ok(()) => {},
                Err(e) => eprintln!("Failed to send OrderEvent: {}", e),
            }
        }

        if self.mode == "Debug".to_string() {
            println!("Finish event, Current_positions, {}, {:?}", data_handler.get_latest_bar_datetime(&signal_event.symbol).unwrap(), self.current_positions);
        }
    }

    fn output_summary_stats(&self) -> anyhow::Result<&farukon_core::performance::PerformanceMetrics> {
        if self.mode == "Debug".to_string() {
            println!("{:#?}", self.equity_series);
        }

        // self.export_results(); TODO
        let output_metrics = self.performance_manager.get_current_performance_metrics();
              
        anyhow::Ok(output_metrics)
    }

    fn calculate_final_performance(&mut self) {
        if let farukon_core::settings::MetricsMode::Offline = self.strategy_settings.portfolio_settings_for_strategy.metrics_calculation_mode {
            let equity_series = self.get_equity_capital_values();
            let start_date = self.get_all_holdings().first().unwrap().datetime;
            let end_date = self.get_all_holdings().last().unwrap().datetime;
            
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

    fn get_current_positions(&self) -> &std::collections::HashMap<String, farukon_core::portfolio::PositionState> {
        &self.current_positions
    }

    fn get_all_positions(&self) -> &Vec<farukon_core::portfolio::PositionSnapshot> {
        &self.all_positions
    }

    fn get_current_holdings(&self) -> &std::collections::HashMap<String, farukon_core::portfolio::HoldingsState> {
        &self.current_holdings
    }

    fn get_all_holdings(&self) -> &Vec<farukon_core::portfolio::HoldingSnapshot> {
        &self.all_holdings
    }

    fn get_current_equity_point(&self) -> &farukon_core::portfolio::EquityPoint {
        &self.current_equity_point
    }

    fn get_all_equity_points(&self) -> &Vec<farukon_core::portfolio::EquitySnapshot> {
        &self.all_equity_points
    }

    fn get_latest_equity_point(&self) -> Option<&farukon_core::portfolio::EquitySnapshot> {
        self.all_equity_points.last()
    }

    fn get_equity_capital_values(&self) -> Vec<f64> {
        self.all_equity_points.iter().map(|point| point.equity_point.capital).collect()
    }

}

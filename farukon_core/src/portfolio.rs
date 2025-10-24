// farukon_core/src/portfolio.rs

use crate::event;
use crate::performance;
use crate::data_handler;

#[derive(Debug, Clone)]
pub struct PositionState {
    pub deal_number: usize,
    pub position: f64,
    pub entry_capital: f64,
    pub entry_price: Option<f64>,
    pub exit_price: Option<f64>,
}

impl PositionState {
    pub fn new() -> Self {
        Self {
            deal_number: 0,
            position: 0.0,
            entry_capital: 0.0,
            entry_price: None,
            exit_price: None,
        }
    }

}

#[derive(Debug, Clone)]
pub struct PositionSnapshot {
    pub datetime: chrono::DateTime<chrono::Utc>,
    pub positions: std::collections::HashMap<String, PositionState>,
}

impl PositionSnapshot {
    pub fn new(
        datetime: chrono::DateTime<chrono::Utc>,
        positions: std::collections::HashMap<String, PositionState>,
    ) -> Self {
        Self { datetime, positions }
    }

}

#[derive(Debug, Clone)]
pub struct HoldingsState {
    pub pnl: f64,
    pub blocked: f64,
}

impl HoldingsState {
    pub fn new() -> Self {
        Self { pnl: 0.0, blocked: 0.0 }
    }

}

#[derive(Debug, Clone)]
pub struct HoldingSnapshot {
    pub datetime: chrono::DateTime<chrono::Utc>,
    pub holdings: std::collections::HashMap<String, HoldingsState>,
}

impl HoldingSnapshot {
    pub fn new(
        datetime: chrono::DateTime<chrono::Utc>,
        holdings: std::collections::HashMap<String, HoldingsState>,
    ) -> Self {
        Self { datetime, holdings }
    }

}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EquityPoint {
    pub blocked: f64,
    pub cash: f64,
    pub capital: f64,
}

impl EquityPoint {
    pub fn new(
        blocked: f64,
        cash: f64,
        capital: f64,
    ) -> Self {
        Self {
            blocked,
            cash,
            capital
        }
    }

    pub fn default(
        initial_capital_for_strategy: f64
    ) -> Self {
        Self {
            blocked: 0.0,
            cash: initial_capital_for_strategy,
            capital: initial_capital_for_strategy
        }
    }

}

#[derive(Debug, Clone)]
pub struct EquitySnapshot {
    pub datetime: chrono::DateTime<chrono::Utc>,
    pub equity_point: EquityPoint,
}

impl EquitySnapshot {
    pub fn new(
        datetime: chrono::DateTime<chrono::Utc>,
        equity_point: EquityPoint,
    ) -> Self {
        Self { datetime, equity_point }
    }
    
}

pub trait PortfolioHandler {
    fn update_timeindex(&mut self, data_handler: &Box<dyn data_handler::DataHandler>);
    fn update_signal(&mut self, signal_event: &event::SignalEvent, data_handler: &Box<dyn data_handler::DataHandler>);
    fn update_fill(
        &mut self,
        fill_event: &event::FillEvent,
        data_handler: &Box<dyn data_handler::DataHandler>,
    );
    fn update_positions_from_fill(&mut self, fill_event: &event::FillEvent, data_handler: &Box<dyn data_handler::DataHandler>);
    fn update_holdings_from_fill(
        &mut self,
        fill_event: &event::FillEvent,
        data_handler: &Box<dyn data_handler::DataHandler>,
    );
    fn get_current_positions(&self) -> &std::collections::HashMap<String, PositionState>;
    fn get_all_positions(&self) -> &Vec<PositionSnapshot>;
    fn get_current_holdings(&self) -> &std::collections::HashMap<String, HoldingsState>;
    fn get_all_holdings(&self) -> &Vec<HoldingSnapshot>;
    fn get_current_equity_point(&self) -> &EquityPoint;
    fn get_all_equity_points(&self) -> &Vec<EquitySnapshot>;
    fn get_latest_equity_point(&self) -> Option<&EquitySnapshot>;
    fn get_equity_capital_values(&self) -> Vec<f64>;
    fn output_summary_stats(&self) -> anyhow::Result<&performance::PerformanceMetrics>;
    fn calculate_final_performance(&mut self);

}

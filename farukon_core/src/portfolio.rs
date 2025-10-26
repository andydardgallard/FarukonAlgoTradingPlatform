// farukon_core/src/portfolio.rs

//! Portfolio manager: tracks positions, holdings, equity, and risk.
//! Implements PortfolioHandler trait for integration with Backtest.
//! Handles fill events, signal events, and margin calls.

use crate::event;
use crate::performance;
use crate::data_handler;

/// Represents the state of a position for a specific symbol.
/// Tracks deal count, size, entry/exit prices, and capital involved.
#[derive(Debug, Clone)]
pub struct PositionState {
    /// Number of deals executed for this symbol.
    pub deal_number: usize,
    /// Current size of the position (positive for long, negative for short).
    pub position: f64,
    /// Capital allocated when entering the current position.
    pub entry_capital: f64,
    /// Price at which the current position was opened.
    pub entry_price: Option<f64>,
    /// Price at which the current position was closed (if applicable).
    pub exit_price: Option<f64>,
}

impl PositionState {
    /// Creates a new `PositionState` with default values (no position, no deals).
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

/// A snapshot of all position states at a specific point in time.
#[derive(Debug, Clone)]
pub struct PositionSnapshot {
    pub datetime: chrono::DateTime<chrono::Utc>,
    pub positions: std::collections::HashMap<String, PositionState>,
}

impl PositionSnapshot {
    /// Creates a new `PositionSnapshot`.
    ///
    /// # Arguments
    /// * `datetime` - The timestamp for the snapshot.
    /// * `positions` - The map of symbol states.
    pub fn new(
        datetime: chrono::DateTime<chrono::Utc>,
        positions: std::collections::HashMap<String, PositionState>,
    ) -> Self {
        Self { datetime, positions }
    }

}

/// Represents the state of holdings (PnL, blocked margin) for a specific symbol.
#[derive(Debug, Clone)]
pub struct HoldingsState {
    /// Profit and Loss for this symbol.
    pub pnl: f64,
    /// Margin blocked by open positions for this symbol.
    pub blocked: f64,
}

impl HoldingsState {
    /// Creates a new `HoldingsState` with default values (zero PnL, zero blocked margin).
    pub fn new() -> Self {
        Self { pnl: 0.0, blocked: 0.0 }
    }

}

/// A snapshot of all holding states at a specific point in time.
#[derive(Debug, Clone)]
pub struct HoldingSnapshot {
    /// The timestamp of this snapshot.
    pub datetime: chrono::DateTime<chrono::Utc>,
    /// A map of symbol names to their `HoldingsState`.
    pub holdings: std::collections::HashMap<String, HoldingsState>,
}

impl HoldingSnapshot {
    /// Creates a new `HoldingSnapshot`.
    ///
    /// # Arguments
    /// * `datetime` - The timestamp for the snapshot.
    /// * `holdings` - The map of symbol states.
    pub fn new(
        datetime: chrono::DateTime<chrono::Utc>,
        holdings: std::collections::HashMap<String, HoldingsState>,
    ) -> Self {
        Self { datetime, holdings }
    }

}

/// Represents the overall equity state (blocked margin, cash, total capital) at a point in time.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EquityPoint {
    /// Total margin blocked by all open positions.
    pub blocked: f64,
    /// Available cash (not blocked by positions).
    pub cash: f64,
    /// Total capital (cash + realized PnL + unrealized PnL).
    pub capital: f64,
}

impl EquityPoint {
    /// Creates a new `EquityPoint` with specified values.
    ///
    /// # Arguments
    /// * `blocked` - Blocked margin.
    /// * `cash` - Available cash.
    /// * `capital` - Total capital.
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

    /// Creates a default `EquityPoint` initialized with the strategy's starting capital.
    /// Initially, no margin is blocked, so cash equals capital.
    ///
    /// # Arguments
    /// * `initial_capital_for_strategy` - The starting capital for the strategy.
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

/// A snapshot of the overall equity state at a specific point in time.
#[derive(Debug, Clone)]
pub struct EquitySnapshot {
    /// The timestamp of this snapshot.
    pub datetime: chrono::DateTime<chrono::Utc>,
    /// The equity state at this time.
    pub equity_point: EquityPoint,
}

impl EquitySnapshot {
    /// Creates a new `EquitySnapshot`.
    ///
    /// # Arguments
    /// * `datetime` - The timestamp for the snapshot.
    /// * `equity_point` - The equity state.
    pub fn new(
        datetime: chrono::DateTime<chrono::Utc>,
        equity_point: EquityPoint,
    ) -> Self {
        Self { datetime, equity_point }
    }
    
}

/// Defines the interface for a portfolio manager.
/// Allows the backtesting engine to interact with the portfolio state without knowing its concrete implementation.
pub trait PortfolioHandler {
    /// Updates the portfolio's time-indexed state (e.g., equity, holdings, positions) based on the latest market data.
    /// Called on every `MARKET` event.
    fn update_timeindex(&mut self, data_handler: &Box<dyn data_handler::DataHandler>);
    
    /// Processes a `SIGNAL` event generated by a strategy.
    /// Typically converts the signal into an `ORDER` event.
    fn update_signal(&mut self, signal_event: &event::SignalEvent, data_handler: &Box<dyn data_handler::DataHandler>);
    
    /// Processes a `FILL` event received from the execution handler.
    /// Updates positions and holdings based on the executed trade details.
    fn update_fill(
        &mut self,
        fill_event: &event::FillEvent,
        data_handler: &Box<dyn data_handler::DataHandler>,
    );

    /// Updates the internal position state based on a `FILL` event.
    fn update_positions_from_fill(&mut self, fill_event: &event::FillEvent, data_handler: &Box<dyn data_handler::DataHandler>);
    
    /// Updates the internal holding state (PnL, blocked margin) based on a `FILL` event.
    fn update_holdings_from_fill(
        &mut self,
        fill_event: &event::FillEvent,
        data_handler: &Box<dyn data_handler::DataHandler>,
    );

    // --- Getters ---

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

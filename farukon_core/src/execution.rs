// farukon_core/src/execution.rs

//! Interface for order execution simulation.
//! Allows swapping between simulated and real brokers.
//!
//! The ExecutionHandler trait defines the contract for executing orders.
//! The SimulatedExecutionHandler implements this trait for backtesting.

use crate::event;
use crate::settings;
use crate::data_handler;
use crate::instruments_info;

/// Defines the interface for an execution handler.
/// All execution handlers must implement this trait.
pub trait ExecutionHandler {
    /// Executes an order.
    /// # Arguments
    /// * `event` - The OrderEvent to execute.
    /// * `strategy_instruments_info` - Instrument metadata for all traded symbols.
    /// * `strategy_settings` - Strategy settings, including slippage and commission plans.
    /// * `data_handler` - The data handler for accessing market data.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn execute_order(
        &self,
        event: &event::OrderEvent,
        strategy_instruments_info: &std::collections::HashMap<String, instruments_info::InstrumentInfo>,
        strategy_settings: &settings::StrategySettings,
        data_handler: &dyn data_handler::DataHandler,
    ) -> anyhow::Result<()>;
        
}

// farukon_core/src/strategy.rs

//! Trait definition for trading strategies.
//! Allows dynamic loading of strategies via shared libraries (.dylib/.so/.dll).

use crate::event;
use crate::portfolio;
use crate::data_handler;

/// Main strategy trait.
/// All trading strategies must implement this trait.
pub trait Strategy {
    /// Calculates signals based on market data and current portfolio state.
    /// # Arguments
    /// * `data_handler` - Interface to market data.
    /// * `current_positions` - Current positions for all symbols.
    /// * `latest_equity_point` - Latest equity point.
    /// * `symbol_list` - List of symbols to trade.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn calculate_signals(
        &mut self,
        data_handler: &dyn data_handler::DataHandler,
        current_positions: &std::collections::HashMap<String, portfolio::PositionState>,
        latest_equity_point: &portfolio::EquitySnapshot,
        symbol_list: &[String],
    ) -> anyhow::Result<()>;

    /// Opens a position by sending a limit order.
    /// # Arguments
    /// * `event_sender` - Sender for events.
    /// * `current_bar_datetime` - Current bar datetime.
    /// * `symbol` - Symbol to trade.
    /// * `signal_name` - Signal name (e.g., "LONG", "SHORT").
    /// * `quantity` - Quantity to trade.
    /// * `limit_price` - Limit price.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn open_by_limit(
        &self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    ) -> anyhow::Result<()> {
        event_sender.send(Box::new(event::SignalEvent::new(
            current_bar_datetime,
            symbol.clone(),
            signal_name.to_string(),
            "LMT".to_string(),
            quantity,
            limit_price,
        )))?;

        anyhow::Ok(())
    }

    /// Sends a `SIGNAL` event to close a position using a **limit order**.
    /// This method creates a `SignalEvent` with the specified parameters and sends it through the event channel.
    /// The order type is hardcoded as "LMT".
    ///
    /// # Arguments
    /// * `event_sender` - The channel sender for broadcasting events.
    /// * `current_bar_datetime` - The timestamp associated with this signal.
    /// * `symbol` - The trading symbol (e.g., "Si-12.23").
    /// * `signal_name` - The name of the signal (e.g., "EXIT").
    /// * `quantity` - The number of contracts to trade (use `None` if not applicable or to use current position size).
    /// * `limit_price` - The specific price at which the limit order should be placed.
    ///
    /// # Returns
    /// * `anyhow::Result<()>` - `Ok(())` on successful sending, `Err` if sending fails.
    fn close_by_limit(
        &self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    ) -> anyhow::Result<()> {
        event_sender.send(Box::new(event::SignalEvent::new(
            current_bar_datetime,
            symbol.clone(),
            signal_name.to_string(),
            "LMT".to_string(),
            quantity,
            limit_price,
        )))?;

        anyhow::Ok(())
    }

    /// Opens a position by sending a market order.
    /// # Arguments
    /// * `event_sender` - Sender for events.
    /// * `current_bar_datetime` - Current bar datetime.
    /// * `symbol` - Symbol to trade.
    /// * `signal_name` - Signal name (e.g., "LONG", "SHORT").
    /// * `quantity` - Quantity to trade.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn open_by_market(
        &self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
    ) -> anyhow::Result<()> {
        event_sender.send(Box::new(event::SignalEvent::new(
            current_bar_datetime,
            symbol.clone(),
            signal_name.to_string(),
            "MKT".to_string(),
            quantity,
            None,
        )))?;

        anyhow::Ok(())
    }

    /// Closes a position by sending a market order.
    /// # Arguments
    /// * `event_sender` - Sender for events.
    /// * `current_bar_datetime` - Current bar datetime.
    /// * `symbol` - Symbol to trade.
    /// * `signal_name` - Signal name (e.g., "EXIT").
    /// * `quantity` - Quantity to trade.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn close_by_market(
        &self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
    ) -> anyhow::Result<()> {
        event_sender.send(Box::new(event::SignalEvent::new(
            current_bar_datetime,
            symbol.clone(),
            signal_name.to_string(),
            "MKT".to_string(),
            quantity,
            None,
        )))?;

        anyhow::Ok(())
    }

}

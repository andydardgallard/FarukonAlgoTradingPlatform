// farukon_core/src/data_handler.rs

//! Trait definition for market data access.
//! Abstracts away data source (CSV, FlatBuffers, WebSocket).
//! Enables interchangeable data handlers.
//!
//! All data handlers must implement this trait to be used by the backtesting engine.
//! The trait defines methods for accessing OHLCV data, advancing to the next bar, and checking if the backtest should continue.

/// Represents a single bar of market data (OHLCV).
#[derive(Debug, Clone)]
pub struct MarketBar {
    pub datetime: chrono::DateTime<chrono::Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
}

/// Defines the interface for a data handler.
/// All data handlers must implement this trait.
pub trait DataHandler {
    /// Returns the latest bar for the specified symbol.
    /// # Arguments
    /// * `symbol` - The symbol to retrieve data for.
    /// # Returns
    /// * An optional reference to the latest `MarketBar`.
    fn get_latest_bar(&self, symbol: &str) -> Option<&MarketBar>;
    
    /// Returns the last `n` bars for the specified symbol.
    /// # Arguments
    /// * `symbol` - The symbol to retrieve data for.
    /// * `n` - The number of bars to retrieve.
    /// # Returns
    /// * A vector of references to the last `n` `MarketBar`s.
    fn get_latest_bars(&self, symbol: &str, n: usize) -> Vec<&MarketBar>;
    
    /// Returns the timestamp of the latest bar for the specified symbol.
    /// # Arguments
    /// * `symbol` - The symbol to retrieve data for.
    /// # Returns
    /// * An optional `chrono::DateTime<chrono::Utc>` representing the timestamp.
    fn get_latest_bar_datetime(&self, symbol: &str) -> Option<chrono::DateTime<chrono::Utc>>;
    
    /// Returns a specific value (open, high, low, close, volume) from the latest bar for the specified symbol.
    /// # Arguments
    /// * `symbol` - The symbol to retrieve data for.
    /// * `val_type` - The type of value to retrieve ("open", "high", "low", "close", "volume").
    /// # Returns
    /// * An optional `f64` representing the value, or `None` if not available.
    fn get_latest_bar_value(&self, symbol: &str, val_type: &str) -> Option<f64>;
    
    /// Returns a specific value (open, high, low, close, volume) from the last `n` bars for the specified symbol.
    /// # Arguments
    /// * `symbol` - The symbol to retrieve data for.
    /// * `val_type` - The type of value to retrieve ("open", "high", "low", "close", "volume").
    /// * `n` - The number of bars to retrieve.
    /// # Returns
    /// * A vector of `f64` values.
    fn get_latest_bars_values(&self, symbol: &str, val_type: &str, n: usize) -> Vec<f64>;
    
    /// Advances the data handler to the next bar.
    /// This method is called by the backtesting engine to simulate time passing.
    fn update_bars(&mut self);  // Advance to next bar
    
    /// Advances the data handler to the next bar.
    /// This method is called by the backtesting engine to simulate time passing.
    fn get_continue_backtest(&self) -> bool;    // Is data exhausted?
    
    /// Sets the flag to stop the backtest.
    /// # Arguments
    /// * `value` - The new value for the flag.
    fn set_continue_backtest(&mut self, value: bool);   // Stop backtest manually

}

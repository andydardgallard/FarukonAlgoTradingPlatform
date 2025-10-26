// farukon_core/src/data_handler.rs

//! Trait definition for market data access.
//! Abstracts away data source (CSV, FlatBuffers, WebSocket).
//! Enables interchangeable data handlers.

#[derive(Debug, Clone)]
pub struct MarketBar {
    pub datetime: chrono::DateTime<chrono::Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: u64,
}

pub trait DataHandler {
    // Core methods to access OHLCV data.
    fn get_latest_bar(&self, symbol: &str) -> Option<&MarketBar>;
    fn get_latest_bars(&self, symbol: &str, n: usize) -> Vec<&MarketBar>;
    fn get_latest_bar_datetime(&self, symbol: &str) -> Option<chrono::DateTime<chrono::Utc>>;
    fn get_latest_bar_value(&self, symbol: &str, val_type: &str) -> Option<f64>;
    fn get_latest_bars_values(&self, symbol: &str, val_type: &str, n: usize) -> Vec<f64>;
    fn update_bars(&mut self);  // Advance to next bar
    fn get_continue_backtest(&self) -> bool;    // Is data exhausted?
    fn set_continue_backtest(&mut self, value: bool);   // Stop backtest manually
}

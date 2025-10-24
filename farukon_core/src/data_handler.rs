// farukon_core/src/data_handler.rs

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
    fn get_latest_bar(&self, symbol: &str) -> Option<&MarketBar>;
    fn get_latest_bars(&self, symbol: &str, n: usize) -> Vec<&MarketBar>;
    fn get_latest_bar_datetime(&self, symbol: &str) -> Option<chrono::DateTime<chrono::Utc>>;
    fn get_latest_bar_value(&self, symbol: &str, val_type: &str) -> Option<f64>;
    fn get_latest_bars_values(&self, symbol: &str, val_type: &str, n: usize) -> Vec<f64>;
    fn update_bars(&mut self);
    fn get_continue_backtest(&self) -> bool;
    fn set_continue_backtest(&mut self, value: bool);
}

//! farukon_core/src/data_handler.rs

//! Trait definition for market data access.
//! Abstracts away data source (CSV, FlatBuffers, WebSocket).
//! Enables interchangeable data handlers.
//!
//! All data handlers must implement this trait to be used by the backtesting engine.
//! The trait defines methods for accessing OHLCV data, advancing to the next bar, and checking if the backtest should continue.

/// Represents a single bar of market data (OHLCV).
#[derive(Debug, Clone)]
pub struct MarketBar {
    datetime: chrono::DateTime<chrono::Utc>,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: u64,
}

impl MarketBar {
    pub fn new() -> Self {
        Self {
            datetime: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            open: 0.0,
            high: 0.0,
            low: 0.0,
            close: 0.0,
            volume: 0,
        }
    }

    pub fn with_datetime(mut self, datetime: chrono::DateTime<chrono::Utc>) -> Self {
        self.datetime = datetime;
        self
    }

    pub fn with_open(mut self, open: f64) -> Self {
        self.open = open;
        self
    }

    pub fn with_high(mut self, high: f64) -> Self {
        self.high = high;
        self
    }

    pub fn with_low(mut self, low: f64) -> Self {
        self.low = low;
        self
    }

    pub fn with_close(mut self, close: f64) -> Self {
        self.close = close;
        self
    }

    pub fn with_volume(mut self, volume: u64) -> Self {
        self.volume = volume;
        self
    }

    /// --- Getters ---
    pub fn get_datetime(&self) -> chrono::DateTime<chrono::Utc> {
        self.datetime
    }

    pub fn get_open(&self) -> f64 {
        self.open
    }

    pub fn get_high(&self) -> f64 {
        self.high
    }

    pub fn get_low(&self) -> f64 {
        self.low
    }

    pub fn get_close(&self) -> f64 {
        self.close
    }

    pub fn get_volume(&self) -> u64 {
        self.volume
    }
}

#[derive(Debug, Clone)]
pub struct SOAData {
    timestamps: Vec<chrono::DateTime<chrono::Utc>>,
    opens: Vec<f64>,
    highs: Vec<f64>,
    lows: Vec<f64>,
    closes: Vec<f64>,
    volumes: Vec<u64>,
}

impl SOAData {
    pub fn new() -> Self {
        Self {
            timestamps: Vec::new(),
            opens: Vec::new(),
            highs: Vec::new(),
            lows: Vec::new(),
            closes: Vec::new(),
            volumes: Vec::new(),
        }
    }

    pub fn deep_clone(&self) -> Self {
        Self {
            timestamps: self.timestamps.clone(),
            opens: self.opens.clone(),
            highs: self.highs.clone(),
            lows: self.lows.clone(),
            closes: self.closes.clone(),
            volumes: self.volumes.clone(),
        }
    }

    pub fn get_latest_bar(&self, current_index: usize) -> Self {
        Self {
            timestamps: vec![self.timestamps[current_index - 1]],
            opens: vec![self.opens[current_index - 1]],
            highs: vec![self.highs[current_index - 1]],
            lows: vec![self.lows[current_index - 1]],
            closes: vec![self.closes[current_index - 1]],
            volumes: vec![self.volumes[current_index - 1]],
        }
    }

    pub fn set_latest_bars(&self, start_index: usize, end_index: usize) -> Self {
        Self {
            timestamps: self.timestamps[start_index..end_index].to_vec(),
            opens: self.opens[start_index..end_index].to_vec(),
            highs: self.highs[start_index..end_index].to_vec(),
            lows: self.lows[start_index..end_index].to_vec(),
            closes: self.closes[start_index..end_index].to_vec(),
            volumes: self.volumes[start_index..end_index].to_vec(),
        }
    }

    pub fn add_bar(&mut self, bar: &MarketBar) -> () {
        self.timestamps.push(bar.get_datetime());
        self.opens.push(bar.get_open());
        self.highs.push(bar.get_high());
        self.lows.push(bar.get_low());
        self.closes.push(bar.get_close());
        self.volumes.push(bar.get_volume());
    }

    // --- Getters ---
    pub fn get_timestamp(&self, index: usize) -> Option<&chrono::DateTime<chrono::Utc>> {
        if let Some(timestamp) = self.timestamps.get(index) {
            Some(timestamp)
        } else {
            None
        }
    }

    pub fn get_open(&self, index: usize) -> Option<&f64> {
        if let Some(open) = self.opens.get(index) {
            Some(open)
        } else {
            None
        }
    }

    pub fn get_high(&self, index: usize) -> Option<&f64> {
        if let Some(high) = self.highs.get(index) {
            Some(high)
        } else {
            None
        }
    }

    pub fn get_low(&self, index: usize) -> Option<&f64> {
        if let Some(low) = self.lows.get(index) {
            Some(low)
        } else {
            None
        }
    }

    pub fn get_close(&self, index: usize) -> Option<&f64> {
        if let Some(close) = self.closes.get(index) {
            Some(close)
        } else {
            None
        }
    }

    pub fn get_volume(&self, index: usize) -> Option<&u64> {
        if let Some(volume) = self.volumes.get(index) {
            Some(volume)
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.timestamps.len()
    }

    pub fn get_timestamps(&self) -> Option<&Vec<chrono::DateTime<chrono::Utc>>> {
        Some(&self.timestamps)
    }

    pub fn get_opens(&self) -> Option<&Vec<f64>> {
        Some(&self.opens)
    }

    pub fn get_highs(&self) -> Option<&Vec<f64>> {
        Some(&self.highs)
    }

    pub fn get_lows(&self) -> Option<&Vec<f64>> {
        Some(&self.lows)
    }

    pub fn get_closes(&self) -> Option<&Vec<f64>> {
        Some(&self.closes)
    }

    pub fn get_volumes(&self) -> Option<&Vec<u64>> {
        Some(&self.volumes)
    }

    pub fn from_slice(
        timestamps: &[chrono::DateTime<chrono::Utc>],
        opens: &[f64],
        highs: &[f64],
        lows: &[f64],
        closes: &[f64],
        volumes: &[u64],
    ) -> Self {
        Self {
            timestamps: timestamps.to_vec(),
            opens: opens.to_vec(),
            highs: highs.to_vec(),
            lows: lows.to_vec(),
            closes: closes.to_vec(),
            volumes: volumes.to_vec(),
        }
    }
}

/// Defines the interface for a data handler.
/// All data handlers must implement this trait.
pub trait DataHandler {
    /// Returns the latest bar for the specified symbol.
    /// # Arguments
    /// * `symbol` - The symbol to retrieve data for.
    /// # Returns
    /// * An optional reference to the latest `MarketBar`.
    fn get_latest_bar(&self, symbol: &str) -> Option<SOAData>;

    /// Returns the last `n` bars for the specified symbol.
    /// # Arguments
    /// * `symbol` - The symbol to retrieve data for.
    /// * `n` - The number of bars to retrieve.
    /// # Returns
    /// * A vector of references to the last `n` `MarketBar`s.
    fn get_latest_bars(&self, symbol: &str, n: usize) -> Option<SOAData>;

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
    fn get_latest_bars_values(&self, symbol: &str, val_type: &str, n: usize) -> Option<&[f64]>;

    fn get_latest_bars_volume_values(&self, symbol: &str, n: usize) -> Option<&[u64]>;

    /// Advances the data handler to the next bar.
    /// This method is called by the backtesting engine to simulate time passing.
    fn update_bars(&mut self); // Advance to next bar

    /// Advances the data handler to the next bar.
    /// This method is called by the backtesting engine to simulate time passing.
    fn get_continue_backtest(&self) -> bool; // Is data exhausted?

    /// Sets the flag to stop the backtest.
    /// # Arguments
    /// * `value` - The new value for the flag.
    fn set_continue_backtest(&mut self, value: bool); // Stop backtest manually
}

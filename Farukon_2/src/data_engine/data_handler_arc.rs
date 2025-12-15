//! Farukon_2/src/data_engine/data_handler.rs

use crate::data_engine;

/// Data handler that provides access to pre-resampled and aligned Structure-of-Arrays (SOA) data.
/// This handler acts as a bridge between the event-driven backtesting engine and the central `GlobalDataStore`.
/// It holds a shared reference (`Arc`) to the global store and maintains its own `current_index` to track
/// the progression of the backtest in the combined timeline. This allows multiple backtest runs
/// (from parallel optimization) to share the same underlying data source while maintaining
/// their individual state and preventing look-ahead bias.
///
/// The handler implements the `farukon_core::data_handler::DataHandler` trait, providing methods
/// to retrieve the latest bar's data (OHLCV) for specific symbols based on the current index.
/// This ensures that strategies only see data up to the current simulated time.
pub struct SOADataHandlerArc {
    /// Shared reference to the global data store containing all pre-resampled SOA data for the strategy's symbols.
    /// This allows multiple `SOADataHandler` instances to access the same zero-copy data.
    global_data_store: std::sync::Arc<data_engine::global_data_storage::GlobalDataStore>,

    /// The current index in the combined timeline, representing the "current" bar for this specific handler instance.
    /// Each backtest thread managing its own `SOADataHandler` will increment this index independently,
    /// ensuring that it only accesses data up to its current time step.
    current_index: usize,
    
    /// Flag indicating whether the backtest should continue.
    /// Set to `false` when `current_index` reaches the end of the combined timeline.
    continue_backtest: bool,
}

impl SOADataHandlerArc {
    /// Creates a new `SOADataHandler` instance.
    /// Initializes the handler with the shared `GlobalDataStore` and sets the initial state (index = 0, continue = true).
    ///
    /// # Arguments
    /// * `global_data_store` - An `Arc` wrapped `GlobalDataStore` containing the SOA-formatted, resampled data.
    ///
    /// # Returns
    /// * `SOADataHandler` - The newly created handler instance.
    pub fn new(global_data_store: std::sync::Arc<data_engine::global_data_storage::GlobalDataStore>) -> Self {
        
        // Check if the combined timeline has any data points to determine the initial continue_backtest flag.
        let continue_backtest = global_data_store.get_combined_timeline().len() > 0;
        println!("data_pointer_arc: {:p}", global_data_store.get_combined_timeline().as_ptr());
        Self {
            global_data_store,
            current_index: 0,
            continue_backtest,
        }
    }

    /// Provides access to the current index of this handler.
    /// This index represents the current position in the `GlobalDataStore`'s combined timeline.
    ///
    /// # Returns
    /// * `usize` - The current index value.
    pub fn get_current_index(&self) -> usize {
        self.current_index
    }

}

impl farukon_core::data_handler::DataHandler for SOADataHandlerArc {
    /// Retrieves the latest bar for the specified symbol based on the current index.
    /// This method constructs a `MarketBar` on-the-fly from the SOA data arrays at the `current_index - 1`.
    /// It is deprecated in favor of direct value access methods (`get_latest_bar_value`, `get_latest_bar_datetime`)
    /// for zero-copy performance, but kept for compatibility.
    ///
    /// # Arguments
    /// * `symbol` - The trading symbol (e.g., "Si-12.23").
    ///
    /// # Returns
    /// * `Option<farukon_core::data_handler::MarketBar>` - The latest `MarketBar` or `None` if no data exists or index is 0.
    fn get_latest_bar(&self, symbol: &str) -> Option<farukon_core::data_handler::SOAData> {
        // Check if we have processed at least one bar (current_index > 0).
        if self.current_index > 0 {
            // Calculate the index of the *previous* bar (the one we just processed).
            let idx = self.current_index - 1;
            // Get the SOA data for the specified symbol from the global store.
            if let Some(soa_data)  = self. global_data_store.get_soa_data_for_symbol(symbol) {
                // Check if the calculated index is within the bounds of the symbol's data.
                if idx < soa_data.len() {
                    let soa_bar = soa_data.get_latest_bar(self.current_index);
                    Some(soa_bar)
                } else {
                    // Index is out of bounds for the symbol's data.
                    None
                }
            } else {
                // Symbol not found in the global data store.
                None
            }
        } else {
            // No data has been processed yet (current_index is 0).
            None
        }
    }

    /// Retrieves the last `n` bars for the specified symbol based on the current index.
    /// This method constructs multiple `MarketBar` structs on-the-fly from the SOA data arrays.
    /// It is deprecated in favor of `get_latest_bars_values_slice` for zero-copy performance.
    ///
    /// # Arguments
    /// * `symbol` - The trading symbol (e.g., "Si-12.23").
    /// * `n` - The number of latest bars to retrieve.
    ///
    /// # Returns
    /// * `Option<farukon_core::data_handler::SOAData>` - A `SOAData` struct containing the last `n` bars or `None` if symbol not found.
    fn get_latest_bars(&self, symbol: &str, n: usize) -> Option<farukon_core::data_handler::SOAData> {
        let soa_data = if let Some(data) = self.global_data_store.get_soa_data_for_symbol(symbol) {
            data
        } else {
            return None;
        };

        let start_index = if self.current_index as i64 - n as i64 <= 0 { 0 } else { self.current_index - n };
        
        Some(soa_data.set_latest_bars(start_index, self.get_current_index()))
    }

    /// Retrieves the timestamp of the latest bar for the specified symbol based on the current index.
    /// This method accesses the combined timeline directly.
    ///
    /// # Arguments
    /// * `symbol` - The trading symbol (e.g., "Si-12.23").
    ///
    /// # Returns
    /// * `Option<chrono::DateTime<chrono::Utc>>` - The timestamp of the latest bar or `None` if no data exists or index is 0.
    fn get_latest_bar_datetime(&self, symbol: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        if self.current_index > 0 {
            let idx = self.current_index - 1;
            if let Some(soa_data) = self.global_data_store.get_soa_data_for_symbol(symbol) {
                if idx < soa_data.len() {
                    let datetime = self.global_data_store.get_combined_timeline()[idx];
                    Some(datetime) 
                } else {
                    // Index is out of bounds for the symbol's data.
                    None
                }
            } else {
                // Symbol not found in the global data store.
                None
            }
        } else {
            // No data has been processed yet (current_index is 0).
            None
        }
    }

    /// Retrieves a specific value (open, high, low, close, volume) from the latest bar for the specified symbol based on the current index.
    /// This is the preferred method for zero-copy access to individual values.
    ///
    /// # Arguments
    /// * `symbol` - The trading symbol (e.g., "Si-12.23").
    /// * `val_type` - The type of value to retrieve ("open", "high", "low", "close", "volume").
    ///
    /// # Returns
    /// * `Option<f64>` - The requested value or `None` if not available (symbol not found, index out of bounds, or invalid val_type).
    fn get_latest_bar_value(&self, symbol: &str, val_type: &str) -> Option<f64> {
        if self.current_index > 0 {
            let idx = self.current_index - 1;
            if let Some(soa_data) = self.global_data_store.get_soa_data_for_symbol(symbol) {
                if idx < soa_data.len() {
                    match val_type {
                        "open" => Some(*soa_data.get_open(idx).unwrap()),
                        "high" => Some(*soa_data.get_high(idx).unwrap()),
                        "low" => Some(*soa_data.get_low(idx).unwrap()),
                        "close" => Some(*soa_data.get_close(idx).unwrap()),
                        "volume" => Some(*soa_data.get_volume(idx).unwrap() as f64),
                        _ => {
                            eprintln!("Warning: Unknown value type '{}' for symbol '{}'", val_type, symbol);
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Retrieves a slice of a specific value (open, high, low, close) from the last `n` bars for the specified symbol based on the current index.
    /// This is the most efficient way (zero-copy) to access multiple historical values of a single field.
    ///
    /// # Arguments
    /// * `symbol` - The trading symbol (e.g., "Si-12.23").
    /// * `val_type` - The type of value to retrieve ("open", "high", "low", "close").
    /// * `n` - The number of latest bars to retrieve values from.
    ///
    /// # Returns
    /// * `Option<&[f64]>` - A slice of the requested values or `None` if symbol not found, index out of bounds, or invalid val_type.
    fn get_latest_bars_values(&self, symbol: &str, val_type: &str, n: usize) -> Option<&[f64]> {
        if self.current_index > 0 {
            let idx = self.current_index - 1;
            if let Some(soa_data) = self.global_data_store.get_soa_data_for_symbol(symbol) {
                if idx < soa_data.len() {
                    let start_index = if self.current_index as i64 - n as i64 <= 0 { 0 } else { self.current_index - n };

                    match val_type {
                        "open" => Some(&soa_data.get_opens().unwrap()[start_index..self.current_index]),
                        "high" => Some(&soa_data.get_highs().unwrap()[start_index..self.current_index]),
                        "low" => Some(&soa_data.get_lows().unwrap()[start_index..self.current_index]),
                        "close" => Some(&soa_data.get_closes().unwrap()[start_index..self.current_index]),
                        // Note: This method returns &[f64], so it cannot directly return &[u64] for volume.
                        // A separate method like get_latest_bars_volume_values would be needed for volume.
                        _ => {
                            eprintln!("Warning: Unknown value type '{}' for symbol '{}'", val_type, symbol);
                            None
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Retrieves a slice of volume values (u64) from the last `n` bars for the specified symbol based on the current index.
    /// This is the most efficient way (zero-copy) to access multiple historical volume values.
    ///
    /// # Arguments
    /// * `symbol` - The trading symbol (e.g., "Si-12.23").
    /// * `n` - The number of latest bars to retrieve volume values from.
    ///
    /// # Returns
    /// * `Option<&[u64]>` - A slice of the volume values or `None` if symbol not found, index out of bounds.
    fn get_latest_bars_volume_values(&self, symbol: &str, n: usize) -> Option<&[u64]> {
        if self.current_index > 0 {
            let idx = self.current_index - 1;
            if let Some(soa_data) = self.global_data_store.get_soa_data_for_symbol(symbol) {
                if idx < soa_data.len() {
                    let start_index = if self.current_index as i64 - n as i64 <= 0 { 0 } else { self.current_index - n };
                    Some(&soa_data.get_volumes().unwrap()[start_index..self.current_index])
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Advances the internal index to the next bar in the combined timeline.
    /// This method is called by the `Backtest` loop when a `MARKET` event is received.
    /// It effectively moves the `SOADataHandler`'s view of "current" data forward in time.
    fn update_bars(&mut self) {
        // Check if the current index is within the bounds of the combined timeline.
        if self.current_index < self.global_data_store.get_combined_timeline().len() {
            // Increment the current index.
            self.current_index += 1;
            // If the index has reached or exceeded the timeline length, set the continue flag to false.
            if self.current_index >= self.global_data_store.get_combined_timeline().len() {
                self.continue_backtest = false;
            }
        } else {
            // If the index is already beyond the timeline, set the continue flag to false.
            self.continue_backtest = false;
        }
    }

    /// Checks if the backtest should continue based on the internal state.
    /// This flag is set to `false` when the `current_index` reaches the end of the combined timeline.
    ///
    /// # Returns
    /// * `bool` - `true` if the backtest should continue, `false` otherwise.
    fn get_continue_backtest(&self) -> bool {
        self.continue_backtest
    }

    /// Sets the flag to stop the backtest.
    /// This can be used by other components (e.g., `Portfolio` on negative capital) to signal termination.
    ///
    /// # Arguments
    /// * `value` - The new value for the continue flag.
    fn set_continue_backtest(&mut self, value: bool) {
        self.continue_backtest = value;
    }

}

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
pub struct SOADataHandler {
    /// Copy of the global data store containing all pre-resampled SOA data for the strategy's symbols.
    global_data_store: data_engine::global_data_storage::GlobalDataStore,

    /// The current index in the combined timeline, representing the "current" bar for this specific handler instance.
    /// Each backtest thread managing its own `SOADataHandler` will increment this index independently,
    /// ensuring that it only accesses data up to its current time step.
    current_index: usize,

    /// Flag indicating whether the backtest should continue.
    /// Set to `false` when `current_index` reaches the end of the combined timeline.
    continue_backtest: bool,
}

impl SOADataHandler {
    /// Creates a new `SOADataHandler` instance.
    /// Initializes the handler with the shared `GlobalDataStore` and sets the initial state (index = 0, continue = true).
    ///
    /// # Arguments
    /// * `global_data_store` - An `Arc` wrapped `GlobalDataStore` containing the SOA-formatted, resampled data.
    ///
    /// # Returns
    /// * `SOADataHandler` - The newly created handler instance.
    pub fn new(
        global_data_store: std::sync::Arc<data_engine::global_data_storage::GlobalDataStore>,
    ) -> Self {
        // Check if the combined timeline has any data points to determine the initial continue_backtest flag.
        let continue_backtest = !global_data_store.get_combined_timeline().is_empty();
        let global_data_store = global_data_store.deep_clone();
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

impl farukon_core::data_handler::DataHandler for SOADataHandler {
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
            if let Some(soa_data) = self.global_data_store.get_soa_data_for_symbol(symbol) {
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
    fn get_latest_bars(
        &self,
        symbol: &str,
        n: usize,
    ) -> Option<farukon_core::data_handler::SOAData> {
        let soa_data = self.global_data_store.get_soa_data_for_symbol(symbol)?;

        let start_index = if self.current_index as i64 - n as i64 <= 0 {
            0
        } else {
            self.current_index - n
        };

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
                        "volume" => Some(*soa_data.get_volume(idx).unwrap()),
                        _ => {
                            eprintln!(
                                "Warning: Unknown value type '{}' for symbol '{}'",
                                val_type, symbol
                            );
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

    /// Retrieves a slice of a specific value (open, high, low, close, volume) from the last `n` bars for the specified symbol based on the current index.
    /// This is the most efficient way (zero-copy) to access multiple historical values of a single field.
    ///
    /// # Arguments
    /// * `symbol` - The trading symbol (e.g., "Si-12.23").
    /// * `val_type` - The type of value to retrieve ("open", "high", "low", "close", "volume").
    /// * `n` - The number of latest bars to retrieve values from.
    ///
    /// # Returns
    /// * `Option<&[f64]>` - A slice of the requested values or `None` if symbol not found, index out of bounds, or invalid val_type.
    fn get_latest_bars_values(&self, symbol: &str, val_type: &str, n: usize) -> Option<&[f64]> {
        if self.current_index > 0 {
            let idx = self.current_index - 1;
            if let Some(soa_data) = self.global_data_store.get_soa_data_for_symbol(symbol) {
                if idx < soa_data.len() {
                    let start_index = if self.current_index as i64 - n as i64 <= 0 {
                        0
                    } else {
                        self.current_index - n
                    };

                    match val_type {
                        "open" => {
                            Some(&soa_data.get_opens().unwrap()[start_index..self.current_index])
                        }
                        "high" => {
                            Some(&soa_data.get_highs().unwrap()[start_index..self.current_index])
                        }
                        "low" => {
                            Some(&soa_data.get_lows().unwrap()[start_index..self.current_index])
                        }
                        "close" => {
                            Some(&soa_data.get_closes().unwrap()[start_index..self.current_index])
                        }
                        // Volume is `f64` since v3.1.0, so the documented generic API serves it with
                        // the same window as `close`; `get_latest_bars_volume_values` is only a shortcut.
                        "volume" => {
                            Some(&soa_data.get_volumes().unwrap()[start_index..self.current_index])
                        }
                        _ => {
                            eprintln!(
                                "Warning: Unknown value type '{}' for symbol '{}'",
                                val_type, symbol
                            );
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

    /// Retrieves a slice of volume values (f64) from the last `n` bars for the specified symbol based on the current index.
    /// This is the most efficient way (zero-copy) to access multiple historical volume values.
    ///
    /// # Arguments
    /// * `symbol` - The trading symbol (e.g., "Si-12.23").
    /// * `n` - The number of latest bars to retrieve volume values from.
    ///
    /// # Returns
    /// * `Option<&[f64]>` - A slice of the volume values or `None` if symbol not found, index out of bounds.
    fn get_latest_bars_volume_values(&self, symbol: &str, n: usize) -> Option<&[f64]> {
        if self.current_index > 0 {
            let idx = self.current_index - 1;
            if let Some(soa_data) = self.global_data_store.get_soa_data_for_symbol(symbol) {
                if idx < soa_data.len() {
                    let start_index = if self.current_index as i64 - n as i64 <= 0 {
                        0
                    } else {
                        self.current_index - n
                    };
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

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use data_engine::ohlcv_soa_generated::{
        OHLCVListSOA, OHLCVListSOAArgs, OHLCVSOA, OHLCVSOAArgs, finish_ohlcvlist_soa_buffer,
    };
    use farukon_core::data_handler::DataHandler;
    use farukon_core::index::{FullIndex, TimeIndexEntry};
    use farukon_core::settings;

    pub(crate) const SYMBOL: &str = "Si-12.23";
    /// Divisible by 60: the combined timeline rounds timestamps down to the 1-minute window,
    /// and a misaligned start would make every aligned bar a zero-volume padding bar.
    const START: u64 = 1_700_000_040;

    /// Writes a one-symbol dataset (`<symbol>.soa.bin` + `<symbol>.soa.idx`) holding `volumes`
    /// into a fresh temp directory and loads a `GlobalDataStore` from it. The `1min` timeframe
    /// takes the raw-copy path, so bar `i` keeps `volumes[i]` verbatim.
    ///
    /// Shared with `data_handler_arc.rs`: both handlers must expose the same contract.
    pub(crate) fn store_with_volumes(
        volumes: &[f64],
    ) -> std::sync::Arc<data_engine::global_data_storage::GlobalDataStore> {
        let dir = std::env::temp_dir().join(format!(
            "farukon-data-handler-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("temp dir");

        let bar_count = volumes.len();
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let timestamps: Vec<u64> = (0..bar_count as u64).map(|i| START + i * 60).collect();
        let timestamps = fbb.create_vector(&timestamps);
        let opens: Vec<f64> = (0..bar_count).map(|i| 100.0 + i as f64).collect();
        let highs: Vec<f64> = (0..bar_count).map(|i| 110.0 + i as f64).collect();
        let lows: Vec<f64> = (0..bar_count).map(|i| 90.0 + i as f64).collect();
        let closes: Vec<f64> = (0..bar_count).map(|i| 105.0 + i as f64).collect();
        let opens = fbb.create_vector(&opens);
        let highs = fbb.create_vector(&highs);
        let lows = fbb.create_vector(&lows);
        let closes = fbb.create_vector(&closes);
        let volumes_vector = fbb.create_vector(volumes);
        let soa = OHLCVSOA::create(
            &mut fbb,
            &OHLCVSOAArgs {
                timestamps: Some(timestamps),
                opens: Some(opens),
                highs: Some(highs),
                lows: Some(lows),
                closes: Some(closes),
                volumes: Some(volumes_vector),
            },
        );
        let list = OHLCVListSOA::create(&mut fbb, &OHLCVListSOAArgs { data: Some(soa) });
        finish_ohlcvlist_soa_buffer(&mut fbb, list);
        std::fs::write(dir.join(format!("{}.soa.bin", SYMBOL)), fbb.finished_data())
            .expect("write .soa.bin");

        let index = FullIndex {
            time_index: (0..bar_count as u64)
                .map(|i| TimeIndexEntry {
                    timestamp: START + i * 60,
                    index: i,
                })
                .collect(),
            daily_index: vec![],
            timeframe_index: std::collections::HashMap::new(),
        };
        std::fs::write(
            dir.join(format!("{}.soa.idx", SYMBOL)),
            bincode::serialize(&index).expect("serialize index"),
        )
        .expect("write .soa.idx");

        let settings = settings::StrategySettings {
            threads: Some(1),
            strategy_name: "test".to_string(),
            strategy_path: String::new(),
            exit_results_path: String::new(),
            strategy_weight: 1.0,
            slippage: settings::ParamSpec::Discrete(vec![]),
            data: settings::DataSettings {
                data_path: dir.to_string_lossy().into_owned(),
                timeframe: "1min".to_string(),
            },
            symbols: vec![SYMBOL.to_string()],
            strategy_params: std::collections::HashMap::new(),
            pos_sizer_params: settings::PosSizer {
                pos_sizer_name: "1".to_string(),
                pos_sizer_params: std::collections::HashMap::new(),
                pos_sizer_value: settings::ParamSpec::Discrete(vec![]),
            },
            margin_params: settings::MarginParams {
                min_margin: 0.0,
                margin_call_type: String::new(),
            },
            portfolio_settings_for_strategy: settings::PortfolioSettingsForStrategy {
                metrics_calculation_mode: settings::MetricsMode::Offline,
            },
            optimizer_type: settings::OptimizerType::GridSearch,
            commission_plans: None,
        };
        let store = data_engine::global_data_storage::GlobalDataStore::load(&settings)
            .expect("temp dataset must load");
        let _ = std::fs::remove_dir_all(&dir);

        std::sync::Arc::new(store)
    }

    /// `get_latest_bars_values` is documented for `"volume"`: the generic API must serve the `f64`
    /// volume slice over the same window as `"close"` (it used to fall into the unknown-type
    /// branch and return `None`).
    #[test]
    fn generic_bars_values_returns_volume_slice() {
        let mut handler = SOADataHandler::new(store_with_volumes(&[0.25, 0.5, 0.125]));
        for _ in 0..3 {
            handler.update_bars();
        }

        assert_eq!(
            handler.get_latest_bars_values(SYMBOL, "volume", 2),
            Some(&[0.5, 0.125][..])
        );
        // `n` larger than the processed bars still yields the whole window.
        assert_eq!(
            handler.get_latest_bars_values(SYMBOL, "volume", 10),
            Some(&[0.25, 0.5, 0.125][..])
        );
        // The dedicated volume accessor and the generic one agree.
        assert_eq!(
            handler.get_latest_bars_values(SYMBOL, "volume", 2),
            handler.get_latest_bars_volume_values(SYMBOL, 2)
        );
    }
}

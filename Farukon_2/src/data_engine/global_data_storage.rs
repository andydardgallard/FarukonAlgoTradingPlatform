//! Farukon_2/src/data_engine/global_data_storage.rs

use crate::data_engine;

/// Centralized storage for pre-processed market data used by all backtest runs.
/// This structure is designed to:
/// - Load raw market data (from FlatBuffers SOA files) once.
/// - Perform resampling to the target timeframe.
/// - Align data from all symbols to a single, combined timeline using padding.
/// - Store the final, ready-to-use Structure-of-Arrays (SOA) data for each symbol.
/// - Provide a thread-safe, shared reference to this data for multiple backtest instances.
///
/// This approach ensures ultra-fast, low-latency access to aligned, zero-copy data,
/// suitable for SIMD-optimized calculations and event-driven processing.
#[derive(Debug)]
pub struct GlobalDataStore {
    /// Shared reference (Arc) to a HashMap mapping symbol names (e.g., "Si-12.23") to their
    /// pre-resampled and aligned SOA data.
    /// Each `SOAData` vector's length corresponds to the length of `combined_timeline`.
    /// This allows multiple threads (e.g., different optimization runs) to access the same data
    /// without copying the underlying arrays.
    soa_data: std::sync::Arc<std::collections::HashMap<String, std::sync::Arc<farukon_core::data_handler::SOAData>>>,

    /// Shared reference (Arc) to the unified, sorted, and deduplicated timeline
    /// encompassing all timestamps across all symbols after resampling and alignment.
    /// This serves as the master clock for the backtest, ensuring all symbols are synchronized.
    combined_timeline: std::sync::Arc<Vec<chrono::DateTime<chrono::Utc>>>,

    /// Flag indicating whether the initial data loading and processing phase has been completed successfully.
    /// Useful for debugging or checking the readiness of the data store.
    is_loaded: bool,
}

impl GlobalDataStore {
    /// Constructs a new `GlobalDataStore` instance by loading, resampling, and aligning data for all symbols
    /// specified in the strategy settings.
    ///
    /// This method orchestrates the entire data preparation pipeline:
    /// 1. Loads raw SOA data from FlatBuffer files in parallel.
    /// 2. Resamples the data to the target timeframe.
    /// 3. Creates a combined timeline from all resampled data points.
    /// 4. Aligns and pads each symbol's resampled data to the combined timeline.
    ///
    /// # Arguments
    ///
    /// * `strategy_settings` - A reference to the `StrategySettings` containing:
    ///   - `data.data_path`: Directory where `.soa.bin` and `.idx` files are located.
    ///   - `symbols`: List of symbol names to load data for.
    ///   - `data.timeframe`: Target timeframe for resampling (e.g., "1min", "5min", "1d").
    ///   - `threads`: Number of threads to use for parallel processing (optional).
    ///
    /// # Returns
    ///
    /// * `anyhow::Result<Self>` - On success, returns the initialized `GlobalDataStore`.
    ///   On failure (e.g., file not found, parsing error), returns an `anyhow::Error`.
    pub fn load(strategy_settings: &farukon_core::settings::StrategySettings) -> anyhow::Result<Self> {
        let start = std::time::Instant::now();
        println!("Starting to load and process FlatBuffer SOA files from {}...", &strategy_settings.data.data_path);

        let resample_timeframe_str = &strategy_settings.data.timeframe;
        let resample_timeframe_sec = data_engine::utils::resample_timeframe_sec(resample_timeframe_str);
        let fbs_dir = &strategy_settings.data.data_path;
        let symbol_list = &strategy_settings.symbols;

        // --- PHASE 1: PARALLEL LOADING AND RESAMPLING ---
        // Creates a Rayon thread pool to handle loading and resampling of data for multiple symbols concurrently.
        // This significantly speeds up the initial data preparation phase.
        let threads = strategy_settings.threads.unwrap_or(num_cpus::get());
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build()?;

        // Delegates the heavy lifting of loading and resampling to a utility function.
        // This function uses the created thread pool.
        // It returns the resampled data per symbol and the partial timelines for each symbol before alignment.
        let (resampled_data, partial_timelines) = data_engine::utils::load_and_resample(
            &pool,
            symbol_list,
            fbs_dir,
            resample_timeframe_sec
        )?;
        
        // --- PHASE 2: CREATE COMBINED TIMELINE ---
        // Delegates the creation of the unified timeline to a utility function.
        // This function merges and sorts all partial timelines from Phase 1.
        let combined_timeline_arc = data_engine::utils::create_combined_timeline(
            &pool,
            &partial_timelines
        );
        let combined_timeline = combined_timeline_arc.as_ref().clone();

        // --- PHASE 3: FINAL PADDING AND ALIGNMENT ---
        // Delegates the alignment and padding logic to a utility function.
        // This function takes the resampled data and the unified timeline,
        // and creates the final `HashMap<String, Arc<SOAData>>` where each `SOAData` is aligned
        // to the `combined_timeline`.
        let soa_data = data_engine::utils::align_and_pad(
            &pool,
            resampled_data,
            &combined_timeline
        )?;

        println!("GlobalDataStore loaded and processed in {:.3} seconds", start.elapsed().as_secs_f64());
        println!("Symbols loaded: {}", soa_data.len());

        anyhow::Ok(
            GlobalDataStore {
                soa_data,
                combined_timeline: combined_timeline_arc,
                is_loaded: true,
            }
        )
    }

    /// Provides access to the combined timeline.
    /// This timeline represents all unique timestamps across all symbols for the backtest period,
    /// sorted chronologically.
    ///
    /// # Returns
    ///
    /// * A reference to the internal `Vec<chrono::DateTime<chrono::Utc>>` representing the timeline.
    pub fn get_combined_timeline(&self) -> &Vec<chrono::DateTime<chrono::Utc>> {
        &self.combined_timeline
    }

    /// Provides access to the `is_loaded` flag.
    ///
    /// # Returns
    ///
    /// * A reference to the boolean flag indicating if the data store has finished loading.
    pub fn is_loaded(&self) -> &bool {
        &self.is_loaded
    }

    /// Provides access to the SOA data for a specific symbol.
    ///
    /// # Arguments
    ///
    /// * `symbol` - The name of the symbol (e.g., "Si-12.23") to retrieve data for.
    ///
    /// # Returns
    ///
    /// * `Some(&Arc<SOAData>)` if data for the symbol exists, `None` otherwise.
    pub fn get_soa_data_for_symbol(&self, symbol: &str) -> Option<&std::sync::Arc<farukon_core::data_handler::SOAData>> {
        self.soa_data.get(symbol)
    } 

}

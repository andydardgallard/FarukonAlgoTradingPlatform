//! Farukon_2/src/data_engine/utils.rs

use rayon::prelude::*;

use crate::data_engine;

/// Finds the start and end indices in the raw FlatBuffer vector for a given time window.
/// Uses the pre-computed index data (FullIndex) for efficient lookup.
/// This function is used during the resampling process to identify which raw bars fall within a specific target timeframe window.
///
/// # Arguments
/// * `index` - The pre-loaded index data containing mappings from timestamps to vector indices.
/// * `start_timestamp` - The start timestamp of the target aggregation window (in seconds).
/// * `end_timestamp` - The end timestamp of the target aggregation window (in seconds).
///
/// # Returns
/// * `(Option<usize>, Option<usize>)` - A tuple containing the start and end indices within the raw FlatBuffer vector.
///   Returns `(None, None)` if the index is empty or if the window boundaries cannot be found.
fn find_window_indices_direct(
    index: &farukon_core::index::FullIndex,
    start_timestamp: u64,
    end_timestamp: u64,
) -> (Option<usize>, Option<usize>) {
    let time_entries = &index.time_index;

    if time_entries.is_empty() {
        return (None, None);
    }

    // Find the index of the first entry whose timestamp is >= start_timestamp.
    // This is the start of our aggregation window in the raw data vector.
    let start_idx =
        match time_entries.binary_search_by_key(&start_timestamp, |entry| entry.timestamp) {
            Ok(idx) => Some(time_entries[idx].index as usize),
            Err(idx) => {
                if idx < time_entries.len() {
                    Some(time_entries[idx].index as usize)
                } else {
                    None
                }
            }
        };

    // Find the index of the first entry whose timestamp is > end_timestamp.
    // The end of our aggregation window is the index from the entry *before* this one.
    let end_idx = match time_entries.binary_search_by_key(&end_timestamp, |entry| entry.timestamp) {
        Ok(idx) => Some(time_entries[idx].index as usize),
        Err(idx) => {
            if idx > 0 {
                Some(time_entries[idx - 1].index as usize)
            } else {
                None
            }
        }
    };

    (start_idx, end_idx)
}

/// Converts a string representation of a timeframe (e.g., "5min") into its equivalent duration in seconds.
/// This is used to configure the resampling window size.
///
/// # Arguments
/// * `resample_timeframe_str` - The timeframe as a string (e.g., "1min", "5min", "1d").
///
/// # Returns
/// * `u64` - The timeframe in seconds, or 0 if the string is unsupported.
pub fn resample_timeframe_sec(resample_timeframe_str: &str) -> u64 {
    match resample_timeframe_str {
        "1min" => 60,
        "2min" => 120,
        "3min" => 180,
        "4min" => 240,
        "5min" => 300,
        "1d" => 24 * 60 * 60,
        _ => {
            println!("Unsupported resample timeframe: {}", resample_timeframe_str);
            0
        }
    }
}

/// Creates a timeline of unique, aggregated timestamps for a single symbol based on its raw index data.
/// This is used during the initial phase to build the *combined* timeline across all symbols.
/// Each raw timestamp is rounded down to the start of its target aggregation window (e.g., 10:05:45 -> 10:05:00 for 5min).
///
/// # Arguments
/// * `full_index` - The pre-loaded index data for the symbol.
/// * `resample_timeframe_sec` - The target timeframe in seconds for aggregation.
///
/// # Returns
/// * `anyhow::Result<std::collections::HashSet<chrono::DateTime<chrono::Utc>>>` - A set of unique timestamps for the symbol.
fn create_symbol_timeline(
    full_index: &farukon_core::index::FullIndex,
    resample_timeframe_sec: u64,
) -> anyhow::Result<std::collections::HashSet<chrono::DateTime<chrono::Utc>>> {
    let mut symbol_timeline = std::collections::HashSet::new();

    for time_entry in &full_index.time_index {
        let raw_ts = time_entry.timestamp;
        let aggregated_window_start = raw_ts - (raw_ts % resample_timeframe_sec);
        let datetime =
            chrono::DateTime::<chrono::Utc>::from_timestamp(aggregated_window_start as i64, 0)
                .ok_or_else(|| {
                    anyhow::anyhow!("Invalid timestamp {} in index", aggregated_window_start)
                })?;
        symbol_timeline.insert(datetime);
    }

    anyhow::Ok(symbol_timeline)
}

/// A helper function to convert a byte slice (`&[u8]`) into a slice of a specific type (`&[T]`).
/// This is used to interpret the raw bytes from FlatBuffer vectors directly as typed data (e.g., f64, u64).
/// # Safety
/// This function uses `std::slice::from_raw_parts`, which is unsafe. It assumes the byte slice contains
/// a valid sequence of bytes for the type `T`.
///
/// # Arguments
/// * `bytes` - The byte slice to convert.
///
/// # Returns
/// * `&[T]` - A slice of type `T`, or an empty slice if the input is too short.
fn bytes_to_slice<T: Copy>(bytes: &[u8]) -> &[T] {
    // Check if the byte slice is large enough to hold at least one `T`.
    if bytes.len() >= std::mem::size_of::<T>() {
        // Cast the pointer to the byte slice to a pointer to `T`.
        let ptr = bytes.as_ptr() as *const T;
        // Cast the pointer to the byte slice to a pointer to `T`.
        let len = bytes.len() / std::mem::size_of::<T>();
        // Create a slice from the raw pointer. This is unsafe.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    } else {
        // If the slice is too short, return an empty slice.
        &[]
    }
}

/// Resamples raw SOA FlatBuffer data to a target timeframe using the pre-computed index.
/// This function performs the core aggregation logic (e.g., Open-High-Low-Close, volume sum) using SIMD where possible.
/// It relies on the `timeframe_index` within `FullIndex` which contains pre-aggregated timestamps for the target timeframe.
///
/// # Arguments
/// * `ohlcv_list_soa` - The root FlatBuffer object containing the SOA data structure.
/// * `full_index` - The pre-loaded index data, which includes the `timeframe_index` for the target timeframe.
/// * `_symbol_timeline` - The timeline for the symbol (currently unused, as `timeframe_index` is used directly).
/// * `resample_timeframe_sec` - The target timeframe in seconds (e.g., 300 for 5-minute bars).
///
/// # Returns
/// * `anyhow::Result<farukon_core::data_handler::SOAData>` - The resampled data in SOA format or an error.
fn resample_data(
    ohlcv_list_soa: &data_engine::ohlcv_soa_generated::OHLCVListSOA<'_>,
    full_index: &farukon_core::index::FullIndex,
    _symbol_timeline: &std::collections::HashSet<chrono::DateTime<chrono::Utc>>, // Не используем
    resample_timeframe_sec: u64,
) -> anyhow::Result<farukon_core::data_handler::SOAData> {
    let mut resampled_soa_data = farukon_core::data_handler::SOAData::new();
    // Get the nested `OHLCVSOA` object from the list.
    let data_soa = ohlcv_list_soa
        .data()
        .ok_or_else(|| anyhow::anyhow!("OHLCVSOA data missing"))?;

    // Get raw byte slices from the FlatBuffer vectors.
    let raw_opens = data_soa.opens().map(|v| v.bytes()).unwrap_or(&[]);
    let raw_highs = data_soa.highs().map(|v| v.bytes()).unwrap_or(&[]);
    let raw_lows = data_soa.lows().map(|v| v.bytes()).unwrap_or(&[]);
    let raw_closes = data_soa.closes().map(|v| v.bytes()).unwrap_or(&[]);
    let raw_volumes = data_soa.volumes().map(|v| v.bytes()).unwrap_or(&[]);

    // Convert raw byte slices to typed slices for efficient access.
    let opens_slice = bytes_to_slice::<f64>(raw_opens);
    let highs_slice = bytes_to_slice::<f64>(raw_highs);
    let lows_slice = bytes_to_slice::<f64>(raw_lows);
    let closes_slice = bytes_to_slice::<f64>(raw_closes);
    let volumes_slice = bytes_to_slice::<u64>(raw_volumes);

    // Map the resample timeframe (in seconds) to the key used in `FullIndex.timeframe_index`.
    let timeframe_key = match resample_timeframe_sec {
        120 => "2m",
        180 => "3m",
        240 => "4m",
        300 => "5m",
        86400 => "1d",
        _ => {
            eprintln!(
                "ERROR: Unsupported timeframe: {} sec",
                resample_timeframe_sec
            );
            return Err(anyhow::anyhow!(
                "Unsupported timeframe: {} sec",
                resample_timeframe_sec
            ));
        }
    };

    // Get the pre-computed list of timestamps for the target timeframe from the index.
    let resampled_timestamps = full_index
        .timeframe_index
        .get(timeframe_key)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No precomputed index found for timeframe: {}",
                timeframe_key
            )
        })?;

    // If there are no resampled timestamps, return an empty SOAData.
    if resampled_timestamps.is_empty() {
        return Ok(resampled_soa_data);
    }

    // Iterate through the pre-computed timestamps for the target timeframe.
    for (_window_idx, &resampled_timestamp) in resampled_timestamps.iter().enumerate() {
        let window_start_timestamp = resampled_timestamp;
        let window_end_timestamp = resampled_timestamp + resample_timeframe_sec - 1;

        // Find the start and end indices in the *raw* data vector for this specific window.
        let (start_idx, end_idx) =
            find_window_indices_direct(full_index, window_start_timestamp, window_end_timestamp);

        // If valid indices were found for this window, perform aggregation.
        if let (Some(start_idx), Some(end_idx)) = (start_idx, end_idx) {
            if start_idx > end_idx {
                continue;
            }

            let mut has_data = false;
            let mut agg_open: Option<f64> = None;
            let mut agg_high = std::f64::NEG_INFINITY;
            let mut agg_low = std::f64::INFINITY;
            let mut agg_close: Option<f64> = None;
            let mut agg_volume = 0u64;

            // SIMD variables for efficient max/min aggregation.
            let mut simd_max = wide::f64x4::splat(std::f64::NEG_INFINITY);
            let mut simd_min = wide::f64x4::splat(std::f64::INFINITY);

            let mut i = start_idx;
            // Ensure we don't go out of bounds of the raw slices.
            let end_safe = std::cmp::min(end_idx + 1, highs_slice.len());

            // Ensure we don't go out of bounds of the raw slices.
            while i + 3 < end_safe {
                let high_chunk = wide::f64x4::new([
                    highs_slice[i],
                    highs_slice[i + 1],
                    highs_slice[i + 2],
                    highs_slice[i + 3],
                ]);

                let low_chunk = wide::f64x4::new([
                    lows_slice[i],
                    lows_slice[i + 1],
                    lows_slice[i + 2],
                    lows_slice[i + 3],
                ]);

                simd_max = simd_max.max(high_chunk);
                simd_min = simd_min.min(low_chunk);

                // Process open, close, and volume for each of the 4 bars individually.
                for j in 0..4 {
                    let idx = i + j;
                    let open = opens_slice[idx];
                    let close = closes_slice[idx];
                    let volume = volumes_slice[idx];

                    if !open.is_nan() {
                        agg_open.get_or_insert(open);
                        agg_close = Some(close);
                        agg_volume += volume;
                        has_data = true;
                    }
                }

                i += 4;
            }

            // Process remaining bars that didn't fit into a full chunk of 4.
            while i < end_safe {
                let open = opens_slice[i];
                let high = highs_slice[i];
                let low = lows_slice[i];
                let close = closes_slice[i];
                let volume = volumes_slice[i];

                if !open.is_nan() {
                    agg_open.get_or_insert(open);
                    agg_close = Some(close);
                    agg_volume += volume;
                    has_data = true;
                }

                simd_max = simd_max.max(wide::f64x4::splat(high));
                simd_min = simd_min.min(wide::f64x4::splat(low));

                i += 1;
            }

            // Extract final max and min values from SIMD vectors.
            let max_values = simd_max.to_array();
            let min_values = simd_min.to_array();

            for &val in &max_values {
                if val.is_finite() && val > agg_high {
                    agg_high = val;
                }
            }

            for &val in &min_values {
                if val.is_finite() && val < agg_low {
                    agg_low = val;
                }
            }

            // If valid data was found and aggregated for this window, create a new bar.
            if has_data {
                let open = agg_open.unwrap();
                let high = if agg_high.is_finite() { agg_high } else { open };
                let low = if agg_low.is_finite() { agg_low } else { open };
                let close = agg_close.unwrap_or(open);

                let datetime =
                    chrono::DateTime::<chrono::Utc>::from_timestamp(resampled_timestamp as i64, 0)
                        .ok_or_else(|| {
                            anyhow::anyhow!("Invalid timestamp {}", resampled_timestamp)
                        })?;

                let final_bar = farukon_core::data_handler::MarketBar::new()
                    .with_datetime(datetime)
                    .with_open(open)
                    .with_high(high)
                    .with_low(low)
                    .with_close(close)
                    .with_volume(agg_volume);
                resampled_soa_data.add_bar(&final_bar);
            }
        }
    }

    anyhow::Ok(resampled_soa_data)
}

/// Copies raw SOA FlatBuffer data directly without resampling.
/// This is a special case when the target timeframe is the same as the raw data timeframe (e.g., "1min").
/// It simply converts the FlatBuffer vectors into the internal SOAData structure.
///
/// # Arguments
/// * `ohlcv_list_soa` - The root FlatBuffer object containing the SOA data structure.
///
/// # Returns
/// * `anyhow::Result<farukon_core::data_handler::SOAData>` - The raw data in SOA format or an error.
fn copy_raw_data_without_resampling(
    ohlcv_list_soa: &data_engine::ohlcv_soa_generated::OHLCVListSOA<'_>,
) -> anyhow::Result<farukon_core::data_handler::SOAData> {
    // Get the nested `OHLCVSOA` object from the list.
    let data_soa = ohlcv_list_soa
        .data()
        .ok_or_else(|| anyhow::anyhow!("OHLCVSOA data missing"))?;

    // Get raw byte slices and convert them to typed slices.
    let raw_timestamps = data_soa
        .timestamps()
        .map(|v| bytes_to_slice::<u64>(v.bytes()))
        .unwrap_or(&[]);
    let raw_opens = data_soa
        .opens()
        .map(|v| bytes_to_slice::<f64>(v.bytes()))
        .unwrap_or(&[]);
    let raw_highs = data_soa
        .highs()
        .map(|v| bytes_to_slice::<f64>(v.bytes()))
        .unwrap_or(&[]);
    let raw_lows = data_soa
        .lows()
        .map(|v| bytes_to_slice::<f64>(v.bytes()))
        .unwrap_or(&[]);
    let raw_closes = data_soa
        .closes()
        .map(|v| bytes_to_slice::<f64>(v.bytes()))
        .unwrap_or(&[]);
    let raw_volumes = data_soa
        .volumes()
        .map(|v| bytes_to_slice::<u64>(v.bytes()))
        .unwrap_or(&[]);

    // Find the minimum length among all vectors to ensure they are aligned.
    let min_len = raw_timestamps
        .len()
        .min(raw_opens.len())
        .min(raw_highs.len())
        .min(raw_lows.len())
        .min(raw_closes.len())
        .min(raw_volumes.len());

    // Convert raw timestamps (u64) to DateTime<chrono::Utc>.
    let mut timestamps = Vec::with_capacity(min_len);
    for i in 0..min_len {
        let ts = raw_timestamps[i];
        let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(ts as i64, 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid timestamp {}", ts))?;
        timestamps.push(datetime);
    }

    // Create the SOAData structure from the aligned slices.
    let soa_data = farukon_core::data_handler::SOAData::from_slice(
        &timestamps,
        &raw_opens[0..min_len],
        &raw_highs[0..min_len],
        &raw_lows[0..min_len],
        &raw_closes[0..min_len],
        &raw_volumes[0..min_len],
    );

    anyhow::Ok(soa_data)
}

/// Processes a single symbol: loads its raw FlatBuffer data, resamples it (or copies if timeframe is 1min), and returns the result along with its timeline.
/// This function is intended to be used within a parallel iterator (e.g., `par_iter`).
///
/// # Arguments
/// * `symbol` - The symbol name (e.g., "Si-12.23").
/// * `fbs_dir` - The directory containing the .soa.bin and .idx files.
/// * `resample_timeframe_sec` - The target timeframe in seconds.
///
/// # Returns
/// * `anyhow::Result<(String, farukon_core::data_handler::SOAData, Vec<chrono::DateTime<chrono::Utc>>)>` - The symbol name, its resampled SOA data, and its timeline.
fn process_symbol(
    symbol: &str,
    fbs_dir: &str,
    resample_timeframe_sec: u64,
) -> anyhow::Result<(
    String,
    farukon_core::data_handler::SOAData,
    Vec<chrono::DateTime<chrono::Utc>>,
)> {
    let bin_file_path = format!("{}/{}.soa.bin", fbs_dir, symbol);
    let idx_file_path = format!("{}/{}.soa.idx", fbs_dir, symbol);

    // Load .bin file via mmap (zero-copy access).
    let file = std::fs::File::open(&bin_file_path)?;
    let mmap = unsafe { memmap2::Mmap::map(&file)? };
    let ohlcv_list_soa = data_engine::ohlcv_soa_generated::root_as_ohlcvlist_soa(&mmap)?;

    // Load .idx file (deserialized via bincode).
    let idx_data = std::fs::read(&idx_file_path)?;
    let full_index: farukon_core::index::FullIndex = bincode::deserialize(&idx_data)?;

    // Create timeline for the symbol based on its raw index.
    let symbol_timeline = create_symbol_timeline(&full_index, resample_timeframe_sec)?;

    // Resample data if necessary, otherwise copy raw data.
    let resampled_soa_data = if resample_timeframe_sec == 60 {
        // If target timeframe is 1 minute, just copy the raw data.
        copy_raw_data_without_resampling(&ohlcv_list_soa)?
    } else {
        // Otherwise, perform resampling using the index.
        resample_data(
            &ohlcv_list_soa,
            &full_index,
            &symbol_timeline,
            resample_timeframe_sec,
        )?
    };

    // Sort the symbol's timeline for consistency (though index creation might already sort it).
    let mut sorted_timeline: Vec<chrono::DateTime<chrono::Utc>> =
        symbol_timeline.into_iter().collect();
    sorted_timeline.sort_unstable();

    anyhow::Ok((symbol.to_string(), resampled_soa_data, sorted_timeline))
}

/// Loads raw SOA FlatBuffer data for all symbols and performs resampling in parallel.
/// Each symbol is processed independently in its own Rayon thread.
/// This function orchestrates the parallel loading and resampling phase.
///
/// # Arguments
/// * `pool` - The Rayon thread pool to use for parallel execution.
/// * `symbol_list` - The list of symbols to load and resample.
/// * `fbs_dir` - The directory containing the .soa.bin and .idx files.
/// * `resample_timeframe_sec` - The target timeframe in seconds.
///
/// # Returns
/// * `anyhow::Result<(HashMap<String, Arc<SOAData>>, Vec<Vec<chrono::DateTime<chrono::Utc>>>)>` - A map of symbol names to their resampled SOA data (wrapped in Arc for sharing) and a vector of their individual timelines.
pub fn load_and_resample(
    pool: &rayon::ThreadPool,
    symbol_list: &[String],
    fbs_dir: &str,
    resample_timeframe_sec: u64,
) -> anyhow::Result<(
    std::collections::HashMap<String, std::sync::Arc<farukon_core::data_handler::SOAData>>,
    Vec<Vec<chrono::DateTime<chrono::Utc>>>,
)> {
    pool.install(|| {
        let results: anyhow::Result<Vec<_>> = symbol_list
            .par_iter()
            .map(|symbol| process_symbol(symbol, fbs_dir, resample_timeframe_sec))
            .collect();

        let results_vec = results?;

        let mut resampled_data = std::collections::HashMap::new();
        let mut all_partial_timelines = Vec::new();

        for (symbol, soa_data, timeline) in results_vec {
            resampled_data.insert(symbol, std::sync::Arc::new(soa_data));
            all_partial_timelines.push(timeline);
        }

        anyhow::Ok((resampled_data, all_partial_timelines))
    })
}

/// Creates a combined, unified timeline from multiple individual symbol timelines.
/// This timeline represents all unique timestamps across all symbols for the backtest period.
/// The timeline is sorted chronologically.
/// This function uses Rayon for parallel processing of the input timelines.
///
/// # Arguments
/// * `pool` - The Rayon thread pool to use for parallel execution.
/// * `partial_timelines` - A slice of vectors, where each vector represents the timeline for a single symbol.
///
/// # Returns
/// * `std::sync::Arc<Vec<chrono::DateTime<chrono::Utc>>>` - The combined, sorted timeline wrapped in Arc.
pub fn create_combined_timeline(
    pool: &rayon::ThreadPool,
    partial_timelines: &[Vec<chrono::DateTime<chrono::Utc>>],
) -> std::sync::Arc<Vec<chrono::DateTime<chrono::Utc>>> {
    pool.install(|| {
        let all_datetimes: std::collections::HashSet<chrono::DateTime<chrono::Utc>> =
            partial_timelines
                .par_iter()
                .fold(
                    || std::collections::HashSet::new(),
                    |mut acc, timeline| {
                        acc.extend(timeline.iter().copied());
                        acc
                    },
                )
                .reduce(
                    || std::collections::HashSet::new(),
                    |mut set1, set2| {
                        set1.extend(set2);
                        set1
                    },
                );

        let mut combined_timeline: Vec<chrono::DateTime<chrono::Utc>> =
            all_datetimes.into_iter().collect();
        combined_timeline.sort_unstable();
        std::sync::Arc::new(combined_timeline)
    })
}

/// Creates a `MarketBar` filled with NaN values for prices and 0 for volume.
/// This is used as a placeholder when no data is available for a specific symbol at a specific time in the combined timeline.
///
/// # Arguments
/// * `datetime` - The timestamp for the new bar.
///
/// # Returns
/// * `farukon_core::data_handler::MarketBar` - The constructed NaN bar.
fn create_bar_from_source(
    soa_data: &farukon_core::data_handler::SOAData,
    source_idx: usize,
    datetime: chrono::DateTime<chrono::Utc>,
) -> farukon_core::data_handler::MarketBar {
    farukon_core::data_handler::MarketBar::new()
        .with_datetime(datetime)
        .with_open(*soa_data.get_open(source_idx).unwrap_or(&std::f64::NAN))
        .with_high(*soa_data.get_high(source_idx).unwrap_or(&std::f64::NAN))
        .with_low(*soa_data.get_low(source_idx).unwrap_or(&std::f64::NAN))
        .with_close(*soa_data.get_close(source_idx).unwrap_or(&std::f64::NAN))
        .with_volume(*soa_data.get_volume(source_idx).unwrap_or(&0))
}

/// Creates a `MarketBar` filled with NaN values for prices and 0 for volume.
/// This is used as a placeholder when no data is available for a specific symbol at a specific time in the combined timeline.
///
/// # Arguments
/// * `datetime` - The timestamp for the new bar.
///
/// # Returns
/// * `farukon_core::data_handler::MarketBar` - The constructed NaN bar.
fn create_nan_bar(
    datetime: chrono::DateTime<chrono::Utc>,
) -> farukon_core::data_handler::MarketBar {
    farukon_core::data_handler::MarketBar::new()
        .with_datetime(datetime)
        .with_open(std::f64::NAN)
        .with_high(std::f64::NAN)
        .with_low(std::f64::NAN)
        .with_close(std::f64::NAN)
        .with_volume(0)
}

/// Aligns the resampled data for a single symbol to the global combined timeline using padding.
/// If the symbol has data for a specific timestamp in the combined timeline, that data is used.
/// If not, the last known data point is carried forward (padding), or NaN/0 is used if no prior data exists.
/// This function ensures all symbols have data points aligned to the same timeline for synchronized backtesting.
///
/// # Arguments
/// * `soa_data` - The resampled SOA data for the single symbol.
/// * `combined_timeline` - The global, unified timeline.
///
/// # Returns
/// * `farukon_core::data_handler::SOAData` - The aligned SOA data for the symbol, matching the length of `combined_timeline`.
fn align_symbol_data(
    soa_data: &farukon_core::data_handler::SOAData,
    combined_timeline: &[chrono::DateTime<chrono::Utc>],
) -> farukon_core::data_handler::SOAData {
    let mut aligned_soa_data = farukon_core::data_handler::SOAData::new();

    let mut source_map = std::collections::HashMap::new();
    for i in 0..soa_data.len() {
        if let Some(datetime) = soa_data.get_timestamp(i) {
            source_map.insert(*datetime, i);
        }
    }

    let trade_days: Vec<_> = source_map.keys().copied().collect();
    let first_trade_date = trade_days.iter().min().copied();

    let mut last_valid_index: Option<usize> = None;

    for &target_datetime in combined_timeline {
        let is_trading_started = first_trade_date.map_or(false, |first| target_datetime >= first);
        if let Some(&source_idx) = source_map.get(&target_datetime) {
            last_valid_index = Some(source_idx);
            let bar = create_bar_from_source(soa_data, source_idx, target_datetime);
            aligned_soa_data.add_bar(&bar);
        } else if is_trading_started {
            if let Some(idx) = last_valid_index {
                let bar = create_bar_from_source(soa_data, idx, target_datetime);
                aligned_soa_data.add_bar(&bar);
            } else {
                let nan_bar = create_nan_bar(target_datetime);
                aligned_soa_data.add_bar(&nan_bar);
            }
        } else {
            let nan_bar = create_nan_bar(target_datetime);
            aligned_soa_data.add_bar(&nan_bar);
        }
    }

    aligned_soa_data
}

/// Aligns and pads the resampled data for *all* symbols to the global combined timeline in parallel.
/// Each symbol's data is processed independently in its own Rayon thread.
/// This function produces the final SOAData structures that will be stored in GlobalDataStore.
///
/// # Arguments
/// * `pool` - The Rayon thread pool to use for parallel execution.
/// * `resampled_data` - A HashMap mapping symbol names to their resampled SOA data (Arc-ed).
/// * `combined_timeline` - The global, unified timeline.
///
/// # Returns
/// * `anyhow::Result<std::sync::Arc<std::collections::HashMap<String, std::sync::Arc<farukon_core::data_handler::SOAData>>>>` - A map of symbol names to their final, aligned SOA data (Arc-ed), wrapped in another Arc for sharing.
pub fn align_and_pad(
    pool: &rayon::ThreadPool,
    resampled_data: std::collections::HashMap<
        String,
        std::sync::Arc<farukon_core::data_handler::SOAData>,
    >,
    combined_timeline: &[chrono::DateTime<chrono::Utc>],
) -> anyhow::Result<
    std::sync::Arc<
        std::collections::HashMap<String, std::sync::Arc<farukon_core::data_handler::SOAData>>,
    >,
> {
    let final_soa_data = pool.install(|| {
        resampled_data
            .par_iter()
            .map(|(symbol, soa_data_arc)| {
                let aligned_soa_data = align_symbol_data(soa_data_arc.as_ref(), combined_timeline);
                (symbol.clone(), std::sync::Arc::new(aligned_soa_data))
            })
            .collect::<std::collections::HashMap<_, _>>()
    });

    anyhow::Ok(std::sync::Arc::new(final_soa_data))
}

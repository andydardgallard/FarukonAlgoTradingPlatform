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

/// Minimum length of a FlatBuffer header carrying a file identifier:
/// a 4-byte root offset followed by a 4-byte identifier.
const FILE_IDENTIFIER_HEADER_LEN: usize = 8;

/// Parses the root `OHLCVListSOA` of a `.soa.bin` buffer, refusing files written
/// before the volume contract.
///
/// The fixed schema (`ohlcv_soa.fbs`) declares `volumes: [double]` and stamps every
/// buffer with `file_identifier "OHS1"`. A buffer without that identifier was written
/// by the old producer, which stored volume as `uint64`: its bytes would be silently
/// reinterpreted as `f64`, so such a file is rejected with a migration instruction
/// instead of being loaded as corrupt data.
///
/// # Arguments
/// * `buf` - The raw (memory-mapped) bytes of a `.soa.bin` file.
/// * `source` - The file path, used in error messages.
///
/// # Returns
/// * `anyhow::Result<data_engine::ohlcv_soa_generated::OHLCVListSOA<'_>>` - The verified root object.
fn parse_ohlcv_list_soa_buffer<'a>(
    buf: &'a [u8],
    source: &str,
) -> anyhow::Result<data_engine::ohlcv_soa_generated::OHLCVListSOA<'a>> {
    // `buffer_has_identifier` asserts a minimum buffer length, so check it first.
    if buf.len() < FILE_IDENTIFIER_HEADER_LEN
        || !data_engine::ohlcv_soa_generated::ohlcvlist_soa_buffer_has_identifier(buf)
    {
        return Err(anyhow::anyhow!(
            "MIGRATION REQUIRED: '{}' is not an OHLCV SOA v2 buffer (the '{}' file identifier is missing). \
             Files written by the old csv-to-flatbuffer store volume as uint64 and would be misread as f64. \
             Regenerate this file with the fixed csv-to-flatbuffer (schema ohlcv_soa.fbs: volumes as double, \
             file_identifier \"{}\") before running Farukon.",
            source,
            data_engine::ohlcv_soa_generated::OHLCVLIST_SOA_IDENTIFIER,
            data_engine::ohlcv_soa_generated::OHLCVLIST_SOA_IDENTIFIER
        ));
    }

    let list = data_engine::ohlcv_soa_generated::root_as_ohlcvlist_soa(buf)
        .map_err(|e| anyhow::anyhow!("Failed to read OHLCV SOA buffer '{}': {}", source, e))?;

    anyhow::Ok(list)
}

/// Validates that the OHLCV vectors of one symbol all have the same number of bars.
///
/// Timestamps and the open/high/low/close/volume vectors are parallel arrays: a length
/// difference means the file is corrupt, and truncating to the smallest length would
/// silently misalign prices against timestamps (and volume against bars).
///
/// # Arguments
/// * `source` - The file path, used in error messages.
/// * `lengths` - Pairs of `(vector name, length)` for all six vectors.
///
/// # Returns
/// * `anyhow::Result<usize>` - The common bar count, or a descriptive error on mismatch.
fn validate_vector_lengths(source: &str, lengths: &[(&str, usize)]) -> anyhow::Result<usize> {
    let mut iter = lengths.iter();
    let Some(&(first_name, first_len)) = iter.next() else {
        return anyhow::Ok(0);
    };

    for &(name, len) in iter {
        if len != first_len {
            return Err(anyhow::anyhow!(
                "Corrupt OHLCV SOA data in '{}': vector '{}' holds {} bars but vector '{}' holds {}. \
                 Timestamps and the open/high/low/close/volume vectors must all have the same length.",
                source,
                name,
                len,
                first_name,
                first_len
            ));
        }
    }

    anyhow::Ok(first_len)
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
    source: &str,
) -> anyhow::Result<farukon_core::data_handler::SOAData> {
    let mut resampled_soa_data = farukon_core::data_handler::SOAData::new();
    // Get the nested `OHLCVSOA` object from the list.
    let data_soa = ohlcv_list_soa
        .data()
        .ok_or_else(|| anyhow::anyhow!("OHLCVSOA data missing in '{}'", source))?;

    // Get raw byte slices from the FlatBuffer vectors.
    let raw_timestamps = data_soa.timestamps().map(|v| v.bytes()).unwrap_or(&[]);
    let raw_opens = data_soa.opens().map(|v| v.bytes()).unwrap_or(&[]);
    let raw_highs = data_soa.highs().map(|v| v.bytes()).unwrap_or(&[]);
    let raw_lows = data_soa.lows().map(|v| v.bytes()).unwrap_or(&[]);
    let raw_closes = data_soa.closes().map(|v| v.bytes()).unwrap_or(&[]);
    let raw_volumes = data_soa.volumes().map(|v| v.bytes()).unwrap_or(&[]);

    // Convert raw byte slices to typed slices for efficient access.
    let timestamps_slice = bytes_to_slice::<u64>(raw_timestamps);
    let opens_slice = bytes_to_slice::<f64>(raw_opens);
    let highs_slice = bytes_to_slice::<f64>(raw_highs);
    let lows_slice = bytes_to_slice::<f64>(raw_lows);
    let closes_slice = bytes_to_slice::<f64>(raw_closes);
    let volumes_slice = bytes_to_slice::<f64>(raw_volumes);

    // All vectors are parallel arrays: reject a length mismatch up front instead of
    // silently aggregating only as many bars as the shortest vector holds.
    let bar_count = validate_vector_lengths(
        source,
        &[
            ("timestamps", timestamps_slice.len()),
            ("opens", opens_slice.len()),
            ("highs", highs_slice.len()),
            ("lows", lows_slice.len()),
            ("closes", closes_slice.len()),
            ("volumes", volumes_slice.len()),
        ],
    )?;

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
    for &resampled_timestamp in resampled_timestamps.iter() {
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
            let mut agg_high = f64::NEG_INFINITY;
            let mut agg_low = f64::INFINITY;
            let mut agg_close: Option<f64> = None;
            let mut agg_volume = 0.0_f64;

            // SIMD variables for efficient max/min aggregation.
            let mut simd_max = wide::f64x4::splat(f64::NEG_INFINITY);
            let mut simd_min = wide::f64x4::splat(f64::INFINITY);

            let mut i = start_idx;
            // All vectors were validated to hold `bar_count` bars, so this bound is safe
            // for every one of them (not just the highs).
            let end_safe = std::cmp::min(end_idx + 1, bar_count);

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
    source: &str,
) -> anyhow::Result<farukon_core::data_handler::SOAData> {
    // Get the nested `OHLCVSOA` object from the list.
    let data_soa = ohlcv_list_soa
        .data()
        .ok_or_else(|| anyhow::anyhow!("OHLCVSOA data missing in '{}'", source))?;

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
        .map(|v| bytes_to_slice::<f64>(v.bytes()))
        .unwrap_or(&[]);

    // All vectors are parallel arrays: a length mismatch is corrupt data, never a
    // reason to silently truncate every vector to the shortest one.
    let bar_count = validate_vector_lengths(
        source,
        &[
            ("timestamps", raw_timestamps.len()),
            ("opens", raw_opens.len()),
            ("highs", raw_highs.len()),
            ("lows", raw_lows.len()),
            ("closes", raw_closes.len()),
            ("volumes", raw_volumes.len()),
        ],
    )?;

    // Convert raw timestamps (u64) to DateTime<chrono::Utc>.
    let mut timestamps = Vec::with_capacity(bar_count);
    for &ts in &raw_timestamps[..bar_count] {
        let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(ts as i64, 0)
            .ok_or_else(|| anyhow::anyhow!("Invalid timestamp {}", ts))?;
        timestamps.push(datetime);
    }

    // Create the SOAData structure from the aligned slices.
    let soa_data = farukon_core::data_handler::SOAData::from_slice(
        &timestamps,
        &raw_opens[0..bar_count],
        &raw_highs[0..bar_count],
        &raw_lows[0..bar_count],
        &raw_closes[0..bar_count],
        &raw_volumes[0..bar_count],
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
    // The `OHS1` identifier is required: legacy (uint64-volume) files are rejected here.
    let ohlcv_list_soa = parse_ohlcv_list_soa_buffer(&mmap, &bin_file_path)?;

    // Load .idx file (deserialized via bincode).
    let idx_data = std::fs::read(&idx_file_path)?;
    let full_index: farukon_core::index::FullIndex = bincode::deserialize(&idx_data)?;

    // Create timeline for the symbol based on its raw index.
    let symbol_timeline = create_symbol_timeline(&full_index, resample_timeframe_sec)?;

    // Resample data if necessary, otherwise copy raw data.
    let resampled_soa_data = if resample_timeframe_sec == 60 {
        // If target timeframe is 1 minute, just copy the raw data.
        copy_raw_data_without_resampling(&ohlcv_list_soa, &bin_file_path)?
    } else {
        // Otherwise, perform resampling using the index.
        resample_data(
            &ohlcv_list_soa,
            &full_index,
            &symbol_timeline,
            resample_timeframe_sec,
            &bin_file_path,
        )?
    };

    // Sort the symbol's timeline for consistency (though index creation might already sort it).
    let mut sorted_timeline: Vec<chrono::DateTime<chrono::Utc>> =
        symbol_timeline.into_iter().collect();
    sorted_timeline.sort_unstable();

    anyhow::Ok((symbol.to_string(), resampled_soa_data, sorted_timeline))
}

/// Resampled SOA data per symbol (wrapped in `Arc` for sharing) plus each symbol's own timeline.
type ResampledSymbolData = (
    std::collections::HashMap<String, std::sync::Arc<farukon_core::data_handler::SOAData>>,
    Vec<Vec<chrono::DateTime<chrono::Utc>>>,
);

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
) -> anyhow::Result<ResampledSymbolData> {
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
                .fold(std::collections::HashSet::new, |mut acc, timeline| {
                    acc.extend(timeline.iter().copied());
                    acc
                })
                .reduce(std::collections::HashSet::new, |mut set1, set2| {
                    set1.extend(set2);
                    set1
                });

        let mut combined_timeline: Vec<chrono::DateTime<chrono::Utc>> =
            all_datetimes.into_iter().collect();
        combined_timeline.sort_unstable();
        std::sync::Arc::new(combined_timeline)
    })
}

/// Creates a `MarketBar` for a real bar of `soa_data` at the given timestamp.
/// Prices and the volume are copied from `source_idx`; missing values fall back to NaN
/// (prices) and 0.0 (volume).
///
/// # Arguments
/// * `soa_data` - The resampled SOA data for the single symbol.
/// * `source_idx` - The index of the bar to copy.
/// * `datetime` - The timestamp to stamp on the new bar.
///
/// # Returns
/// * `farukon_core::data_handler::MarketBar` - The constructed bar.
fn create_bar_from_source(
    soa_data: &farukon_core::data_handler::SOAData,
    source_idx: usize,
    datetime: chrono::DateTime<chrono::Utc>,
) -> farukon_core::data_handler::MarketBar {
    farukon_core::data_handler::MarketBar::new()
        .with_datetime(datetime)
        .with_open(*soa_data.get_open(source_idx).unwrap_or(&f64::NAN))
        .with_high(*soa_data.get_high(source_idx).unwrap_or(&f64::NAN))
        .with_low(*soa_data.get_low(source_idx).unwrap_or(&f64::NAN))
        .with_close(*soa_data.get_close(source_idx).unwrap_or(&f64::NAN))
        .with_volume(*soa_data.get_volume(source_idx).unwrap_or(&0.0))
}

/// Creates a padding bar for a timestamp at which the symbol has no bar of its own.
///
/// Prices are forward-filled from `source_idx`, but the volume is `0.0`: the volume of
/// the source bar was already booked at the source bar's own timestamp, and a padding bar
/// must never add it a second time (that would inflate the symbol's traded volume and
/// attribute one bar's turnover to another bar of the combined timeline).
///
/// # Arguments
/// * `soa_data` - The resampled SOA data for the single symbol.
/// * `source_idx` - The index of the last known bar of that symbol.
/// * `datetime` - The timestamp to pad.
///
/// # Returns
/// * `farukon_core::data_handler::MarketBar` - The forward-filled bar with zero volume.
fn create_padded_bar(
    soa_data: &farukon_core::data_handler::SOAData,
    source_idx: usize,
    datetime: chrono::DateTime<chrono::Utc>,
) -> farukon_core::data_handler::MarketBar {
    create_bar_from_source(soa_data, source_idx, datetime).with_volume(0.0)
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
        .with_open(f64::NAN)
        .with_high(f64::NAN)
        .with_low(f64::NAN)
        .with_close(f64::NAN)
        .with_volume(0.0)
}

/// Aligns the resampled data for a single symbol to the global combined timeline using padding.
/// If the symbol has data for a specific timestamp in the combined timeline, that data is used.
/// If not, the prices of the last known bar are carried forward with volume `0.0` (padding),
/// or NaN prices / 0 volume are used if no prior data exists.
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
        let is_trading_started = first_trade_date.is_some_and(|first| target_datetime >= first);
        if let Some(&source_idx) = source_map.get(&target_datetime) {
            last_valid_index = Some(source_idx);
            let bar = create_bar_from_source(soa_data, source_idx, target_datetime);
            aligned_soa_data.add_bar(&bar);
        } else if is_trading_started {
            if let Some(idx) = last_valid_index {
                let bar = create_padded_bar(soa_data, idx, target_datetime);
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

#[cfg(test)]
mod tests {
    use super::*;
    use data_engine::ohlcv_soa_generated::{
        OHLCVListSOA, OHLCVListSOAArgs, OHLCVSOA, OHLCVSOAArgs, finish_ohlcvlist_soa_buffer,
        ohlcvlist_soa_buffer_has_identifier, root_as_ohlcvlist_soa,
    };
    use farukon_core::data_handler::{MarketBar, SOAData};
    use farukon_core::index::{FullIndex, TimeIndexEntry};

    const START: u64 = 1_700_000_000;

    /// Builds an `OHLCVListSOA` buffer from explicit volumes.
    ///
    /// `price_len` sets the length of the timestamp and OHLC vectors, `volumes` the length
    /// of the volume vector — they are equal for well-formed data and deliberately differ
    /// in the corruption test. `with_identifier` selects the v2 (`OHS1`) finish helper or
    /// the legacy identifier-less finish, which is how pre-v2 files look.
    fn soa_buffer_with_lengths(
        price_len: usize,
        volumes: &[f64],
        with_identifier: bool,
    ) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let timestamps: Vec<u64> = (0..price_len as u64).map(|i| START + i * 60).collect();
        let timestamps = fbb.create_vector(&timestamps);
        let opens: Vec<f64> = (0..price_len).map(|i| 100.0 + i as f64).collect();
        let highs: Vec<f64> = (0..price_len).map(|i| 110.0 + i as f64).collect();
        let lows: Vec<f64> = (0..price_len).map(|i| 90.0 + i as f64).collect();
        let closes: Vec<f64> = (0..price_len).map(|i| 105.0 + i as f64).collect();
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
        if with_identifier {
            finish_ohlcvlist_soa_buffer(&mut fbb, list);
        } else {
            // Pre-v2 layout: no file identifier at all.
            fbb.finish(list, None);
        }
        fbb.finished_data().to_vec()
    }

    /// Builds an in-memory `OHLCVListSOA` buffer with the given volumes (fractional on purpose).
    fn soa_buffer(volumes: &[f64]) -> Vec<u8> {
        soa_buffer_with_lengths(volumes.len(), volumes, true)
    }

    /// Index over the three raw bars of [`soa_buffer`] plus one 5-minute window at `START`.
    fn soa_index() -> FullIndex {
        FullIndex {
            time_index: (0..3)
                .map(|i| TimeIndexEntry {
                    timestamp: START + i * 60,
                    index: i,
                })
                .collect(),
            daily_index: vec![],
            timeframe_index: std::collections::HashMap::from([("5m".to_string(), vec![START])]),
        }
    }

    /// A timestamp `offset` seconds after [`START`].
    fn datetime_at(offset: u64) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::<chrono::Utc>::from_timestamp((START + offset) as i64, 0)
            .expect("valid timestamp")
    }

    /// Builds `SOAData` from `(offset seconds after START, open, high, low, close, volume)` tuples.
    fn soa_from_bars(bars: &[(u64, f64, f64, f64, f64, f64)]) -> SOAData {
        let mut data = SOAData::new();
        for &(offset, open, high, low, close, volume) in bars {
            let bar = MarketBar::new()
                .with_datetime(datetime_at(offset))
                .with_open(open)
                .with_high(high)
                .with_low(low)
                .with_close(close)
                .with_volume(volume);
            data.add_bar(&bar);
        }
        data
    }

    /// Volumes are `f64`: a fractional volume must load and resample (sum) without truncation.
    #[test]
    fn fractional_volumes_load_and_resample() {
        let buffer = soa_buffer(&[0.25, 0.5, 0.125]);
        let list = parse_ohlcv_list_soa_buffer(&buffer, "X.soa.bin").expect("valid buffer");
        let index = soa_index();

        let resampled = resample_data(
            &list,
            &index,
            &std::collections::HashSet::new(),
            300,
            "X.soa.bin",
        )
        .expect("resampling must succeed");

        assert_eq!(resampled.len(), 1);
        let volume = *resampled.get_volume(0).expect("one resampled bar");
        assert!(
            (volume - 0.875).abs() < 1e-12,
            "fractional volumes must sum, got {}",
            volume
        );
        assert_eq!(*resampled.get_open(0).unwrap(), 100.0);
        assert_eq!(*resampled.get_close(0).unwrap(), 107.0);
    }

    /// The raw (no resampling) copy path keeps fractional volumes too.
    #[test]
    fn fractional_volumes_survive_raw_copy() {
        let buffer = soa_buffer(&[0.25, 0.5, 0.125]);
        let list = parse_ohlcv_list_soa_buffer(&buffer, "X.soa.bin").expect("valid buffer");

        let copied =
            copy_raw_data_without_resampling(&list, "X.soa.bin").expect("copy must succeed");

        assert_eq!(copied.len(), 3);
        assert_eq!(*copied.get_volume(0).unwrap(), 0.25);
        assert_eq!(*copied.get_volume(1).unwrap(), 0.5);
        assert_eq!(*copied.get_volume(2).unwrap(), 0.125);
    }

    /// A buffer without the `OHS1` identifier (a legacy uint64-volume file) is rejected
    /// with a migration error; the identifier-carrying buffer is accepted.
    #[test]
    fn buffer_without_identifier_is_rejected_with_migration_hint() {
        let legacy = soa_buffer_with_lengths(3, &[10.0, 20.0, 30.0], false);
        assert!(
            root_as_ohlcvlist_soa(&legacy).is_ok(),
            "a legacy buffer passes flatbuffer verification — only the identifier check catches it"
        );
        assert!(!ohlcvlist_soa_buffer_has_identifier(&legacy));

        let err = parse_ohlcv_list_soa_buffer(&legacy, "Si-12.23.soa.bin")
            .expect_err("a legacy buffer must be rejected");
        let message = format!("{}", err);
        assert!(
            message.contains("MIGRATION"),
            "expected a migration hint, got: {}",
            message
        );
        assert!(
            message.contains("OHS1"),
            "expected the missing identifier to be named, got: {}",
            message
        );
        assert!(
            message.contains("csv-to-flatbuffer"),
            "expected the producer to be named, got: {}",
            message
        );
        assert!(
            message.contains("Si-12.23.soa.bin"),
            "expected the file path to be named, got: {}",
            message
        );

        // A buffer too short to even hold a file identifier is rejected, not a panic.
        assert!(parse_ohlcv_list_soa_buffer(&[], "empty.soa.bin").is_err());

        let current = soa_buffer(&[0.25, 0.5, 0.125]);
        assert!(ohlcvlist_soa_buffer_has_identifier(&current));
        let list = parse_ohlcv_list_soa_buffer(&current, "Si-12.23.soa.bin")
            .expect("an identifier-carrying buffer must be accepted");
        assert_eq!(
            list.data().expect("OHLCVSOA data").volumes().unwrap().len(),
            3
        );
    }

    /// Differing vector lengths are corrupt data: both the raw-copy and the resampling
    /// path reject it instead of silently truncating to the shortest vector.
    #[test]
    fn mismatched_vector_lengths_are_rejected() {
        let buffer = soa_buffer_with_lengths(3, &[0.25, 0.5], true);
        let list = parse_ohlcv_list_soa_buffer(&buffer, "M.soa.bin").expect("valid buffer");

        let copy_err = copy_raw_data_without_resampling(&list, "M.soa.bin")
            .expect_err("raw copy must reject a length mismatch");
        let copy_message = format!("{}", copy_err);
        assert!(
            copy_message.contains("volumes") && copy_message.contains("timestamps"),
            "expected the mismatched vectors to be named, got: {}",
            copy_message
        );

        let resample_err = resample_data(
            &list,
            &soa_index(),
            &std::collections::HashSet::new(),
            300,
            "M.soa.bin",
        )
        .expect_err("resampling must reject a length mismatch");
        let resample_message = format!("{}", resample_err);
        assert!(
            resample_message.contains("volumes") && resample_message.contains("M.soa.bin"),
            "expected the mismatched vectors and the path to be named, got: {}",
            resample_message
        );
    }

    /// Padding bars (created because another symbol advanced the combined timeline)
    /// forward-fill prices only — their volume is 0.0, never the source bar's volume.
    #[test]
    fn padding_bars_forward_fill_prices_with_zero_volume() {
        // `slow` trades every second bar; `fast` trades every bar and therefore advances
        // the combined timeline past `slow`'s last bar.
        let slow = soa_from_bars(&[
            (0, 100.0, 110.0, 90.0, 105.0, 5.0),
            (120, 101.0, 111.0, 91.0, 106.0, 6.0),
        ]);
        let fast = soa_from_bars(&[
            (0, 200.0, 210.0, 190.0, 205.0, 1.0),
            (60, 201.0, 211.0, 191.0, 206.0, 2.0),
            (120, 202.0, 212.0, 192.0, 207.0, 3.0),
        ]);
        let combined: Vec<_> = [0_u64, 60, 120].iter().map(|&o| datetime_at(o)).collect();

        let aligned_slow = align_symbol_data(&slow, &combined);
        let aligned_fast = align_symbol_data(&fast, &combined);

        assert_eq!(aligned_slow.len(), 3);
        assert_eq!(
            aligned_slow.get_volumes().unwrap().as_slice(),
            &[5.0, 0.0, 6.0],
            "the padding bar must not carry the previous bar's volume"
        );
        // Prices of the padding bar are forward-filled from the last real bar.
        assert_eq!(*aligned_slow.get_open(1).unwrap(), 100.0);
        assert_eq!(*aligned_slow.get_high(1).unwrap(), 110.0);
        assert_eq!(*aligned_slow.get_low(1).unwrap(), 90.0);
        assert_eq!(*aligned_slow.get_close(1).unwrap(), 105.0);
        // Real bars keep their own volume and timestamps are untouched.
        assert_eq!(
            *aligned_slow.get_timestamp(1).unwrap(),
            datetime_at(60),
            "the padding bar keeps the combined-timeline timestamp"
        );

        // The always-trading symbol has no padding bars: its volumes are its own.
        assert_eq!(
            aligned_fast.get_volumes().unwrap().as_slice(),
            &[1.0, 2.0, 3.0]
        );

        // Total volume of `slow` is exactly the volume of its two real bars.
        let total: f64 = aligned_slow.get_volumes().unwrap().iter().sum();
        assert!((total - 11.0).abs() < 1e-12, "unexpected total: {}", total);
    }
}

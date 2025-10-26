// farukon_core/src/index.rs

//! Index structures for ultra-fast data navigation in FlatBuffers.
//! Saved as .idx files via bincode.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimeIndexEntry {
    pub timestamp: u64, // Unix timestamp (seconds)
    pub index: u64, // Index of bar in OHLCVList
}

/// Daily index for ultra-fast day-based navigation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DailyIndexEntry {
    pub date: String,   // "2025-07-08"
    pub start_index: u64,
    pub end_index: u64,
}

/// Full index structure saved as .idx file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FullIndex {
    pub time_index: Vec<TimeIndexEntry>,    // Every bar timestamp
    pub daily_index: Vec<DailyIndexEntry>,  // Per-day ranges
    pub timeframe_index: std::collections::HashMap<String, Vec<u64>>,       // Resampled timestamps (e.g., "5min")
}

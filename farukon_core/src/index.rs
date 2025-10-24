#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TimeIndexEntry {
    pub timestamp: u64,
    pub index: u64,
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
    pub time_index: Vec<TimeIndexEntry>,
    pub daily_index: Vec<DailyIndexEntry>,
    pub timeframe_index: std::collections::HashMap<String, Vec<u64>>,       // "3m" → [timestamp1, timestamp2...]
}

// farukon_core/src/indicators.rs

//! Simple technical indicators.
//! Uses DataHandler to access OHLCV data.
//!
//! Currently, only the Simple Moving Average (SMA) is implemented.
//! More indicators can be added here in the future.

use crate::data_handler;

/// Calculates the Simple Moving Average (SMA) for a specified symbol and value type.
/// # Arguments
/// * `dh` - The data handler for accessing market data.
/// * `symbol` - The symbol to calculate the SMA for.
/// * `val_type` - The type of value to use ("open", "high", "low", "close", "volume").
/// * `n` - The number of bars to average.
/// * `shift` - The number of bars to shift back (0 for current bar, 1 for previous bar, etc.).
/// # Returns
/// * An optional `f64` representing the SMA, or `None` if insufficient data is available.
pub fn sma(
    dh: &dyn data_handler::DataHandler,
    symbol: &str,
    val_type: &str,
    n: usize,
    shift: usize
) -> Option<f64> {
    // Simple Moving Average.
    // Returns average of last n bars, optionally shifted back by 'shift' bars.

    // Calculate the total number of bars needed.
    let need = n + shift;
    // If no bars are needed, return None.
    if need == 0 { return None; }

    // Get the last `need` bars for the specified symbol.
    let vals = dh.get_latest_bars_values(symbol, val_type, need);
    // If insufficient data is available, return None.
    if vals.len() < need { return None; }

    // Calculate the sum of the last `n` bars, starting from `shift` bars ago.
    let end = vals.len() - shift;
    let start = end.checked_sub(n)?;
    let sum: f64 = vals[start..end].iter().copied().sum();
    // Return the average.
    Some(sum / (n as f64))
}

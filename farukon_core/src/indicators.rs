// farukon_core/src/indicators.rs

//! Technical indicator library.
//!
//! This module provides foundational technical indicators used by trading strategies
//! within the Farukon event-driven backtesting framework. Indicators operate on
//! time-series data (e.g., OHLCV) supplied via iterators, enabling decoupling from
//! the underlying data storage mechanism.
//!
//! Currently implemented indicators:
//! - Simple Moving Average (`sma`)
//! - Highest High over N periods (`highest`)
//! - Lowest Low over N periods (`lowest`)
//!
//! All functions are pure, stateless, and return `None` when insufficient historical
//! data is available for the requested lookback window.

/// Computes the **Simple Moving Average (SMA)** over the last `n` data points.
///
/// The SMA is the unweighted mean of the previous `n` values. It is commonly used
/// to smooth price data and identify trend direction.
///
/// # Type Parameters
/// * `I` — An iterator yielding references to `f64` values (e.g., closing prices).
///
/// # Arguments
/// * `dh` — An iterator over historical data points (e.g., a slice of `&f64`).
/// * `n` — The lookback period (number of bars to average). Must be ≥ 1.
///
/// # Returns
/// * `Some(f64)` — The computed SMA value.
/// * `None` — If `n == 0` or fewer than `n` data points are available.
pub fn sma<'a, I>(
    dh: I,
    n: usize,
) -> Option<f64>
where
    I: IntoIterator<Item = &'a f64>,
    I::IntoIter: Clone,
{
    // Simple Moving Average.
    // Returns average of last n bars.
    if n == 0 { return None; }

    // Convert to Vec
    let data: Vec<f64> = dh.into_iter().copied().collect();
    let total_count = data.len();

    // If insufficient data is available, return None.
    if total_count < n { return None; }

    // Calculate the sum of the last `n` bars, starting from `shift` bars ago.
    let start = total_count - n;
    let end = start + n;
    let sum: f64 = data[start..end]
        .iter()
        .sum();
    // Return the average.
    Some(sum / (n as f64))
}

/// Finds the **highest value** over the last `n` bars, shifted back by `shift` bars.
///
/// This function is typically used with high prices to identify resistance levels
/// or channel boundaries. It skips any `None` values in the input sequence.
///
/// # Type Parameters
/// * `I` — An iterator yielding references to `Option<f64>` (e.g., high prices that may be missing).
///
/// # Arguments
/// * `dh` — An iterator over optional historical data points.
/// * `n` — The lookback period (number of bars to scan). Must be ≥ 1.
/// * `shift` — Number of bars to shift the window backward (0 = current bar included).
///
/// # Returns
/// * `Some(f64)` — The maximum value in the specified window.
/// * `None` — If `n == 0`, `n + shift == 0`, or insufficient data is available.
///
/// # Note
/// The effective required data length is `n + shift`. For example, to get the highest
/// high of the last 5 bars as of 2 bars ago, use `n=5, shift=2` (requires 7 bars total).
pub fn highest<'a, I>(
    dh: I,
    n: usize,
    shift: usize,
) -> Option<f64> 
where
    I: IntoIterator<Item = &'a Option<f64>>,
    I::IntoIter: Clone,
{
    if n == 0 { return None; }

    let need = n + shift;
    if need == 0 { return None; }
    
    // Convert to Vec, filtering out None values
    let data: Vec<Option<f64>> = dh
        .into_iter()
        .copied()
        .collect();
    let total_count = data.len();

    // If insufficient data is available, return None.
    if total_count < need { return None; }
    
    let start = total_count - need;
    let end = start + n;

    let window = &data[start..end];
    for item in window {
        if item.is_none() {
            return None;
        }
    }

    window
        .iter()
        .filter_map(|&x| x)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
}

/// Finds the **lowest value** over the last `n` bars, shifted back by `shift` bars.
///
/// This function is typically used with low prices to identify support levels
/// or channel boundaries. It skips any `None` values in the input sequence.
///
/// # Type Parameters
/// * `I` — An iterator yielding references to `Option<f64>`.
///
/// # Arguments
/// * `dh` — An iterator over optional historical data points.
/// * `n` — The lookback period (number of bars to scan). Must be ≥ 1.
/// * `shift` — Number of bars to shift the window backward (0 = current bar included).
///
/// # Returns
/// * `Some(f64)` — The minimum value in the specified window.
/// * `None` — If `n == 0`, `n + shift == 0`, or insufficient data is available.
///
/// # Note
/// The effective required data length is `n + shift`. For example, to get the lowest
/// low of the last 5 bars as of 2 bars ago, use `n=5, shift=2` (requires 7 bars total).
pub fn lowest<'a,I>(
    dh: I,
    n: usize,
    shift: usize,
) -> Option<f64> 
where
    I: IntoIterator<Item = &'a Option<f64>>,
    I::IntoIter: Clone,
{
    if n == 0 { return None; }

    let need = n + shift;
    if need == 0 { return None; }
    
    // Convert to Vec, filtering out None values
    let data: Vec<Option<f64>> = dh
        .into_iter()
        .copied()
        .collect();
    let total_count = data.len();

    // If insufficient data is available, return None.
    if total_count < need { return None; }
    
    let start = total_count - need;
    let end = start + n;
    let window = &data[start..end];

    window
        .iter()
        .flat_map(|&x| x)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
}

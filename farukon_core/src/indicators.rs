// farukon_core/src/indicators.rs

//! Simple technical indicators.
//! Uses DataHandler to access OHLCV data.

use crate::data_handler;

pub fn sma(
    dh: &dyn data_handler::DataHandler,
    symbol: &str,
    val_type: &str,
    n: usize,
    shift: usize
) -> Option<f64> {
    // Simple Moving Average.
    // Returns average of last n bars, optionally shifted back by 'shift' bars.

    let need = n + shift;
    if need == 0 { return None; }

    let vals = dh.get_latest_bars_values(symbol, val_type, need);
    if vals.len() < need { return None; }

    let end = vals.len() - shift;
    let start = end.checked_sub(n)?;
    let sum: f64 = vals[start..end].iter().copied().sum();
    Some(sum / (n as f64))
}

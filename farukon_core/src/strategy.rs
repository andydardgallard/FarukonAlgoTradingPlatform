// farukon_core/src/strategy.rs

//! Trait definition for trading strategies.
//! Allows dynamic loading of strategies via shared libraries (.dylib/.so/.dll).
//!
//! Signals are emitted through [`event::SignalEmitter`]: the strategy library calls a host callback
//! with plain arguments and the host allocates the `SignalEvent`. No heap object and no channel
//! endpoint may cross the FFI boundary (the two sides use different allocators).

use crate::data_handler;
use crate::event;
use crate::portfolio;

/// Main strategy trait.
/// All trading strategies must implement this trait.
pub trait Strategy {
    /// Calculates signals based on market data and current portfolio state.
    /// # Arguments
    /// * `data_handler` - Interface to market data.
    /// * `current_positions` - Current positions for all symbols.
    /// * `all_holdings` - Holdings snapshots (capital, cash, blocked) used for position sizing.
    /// * `symbol_list` - List of symbols to trade.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn calculate_signals(
        &mut self,
        data_handler: &dyn data_handler::DataHandler,
        current_positions: &std::collections::HashMap<String, portfolio::PositionState>,
        all_holdings: &[portfolio::HoldingSnapshot],
        symbol_list: &[String],
    ) -> anyhow::Result<()>;

    /// Opens a position by sending a limit order.
    /// # Arguments
    /// * `emitter` - Signal emitter of the host this strategy runs in.
    /// * `current_bar_datetime` - Current bar datetime.
    /// * `symbol` - Symbol to trade.
    /// * `signal_name` - Signal name (e.g., "LONG", "SHORT").
    /// * `quantity` - Quantity to trade.
    /// * `limit_price` - Limit price.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn open_by_limit(
        &self,
        emitter: &event::SignalEmitter,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &str,
        signal_name: &str,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    ) -> anyhow::Result<()> {
        emitter.emit(
            current_bar_datetime,
            symbol,
            signal_name,
            "LMT",
            quantity,
            limit_price,
        )
    }

    /// Sends a `SIGNAL` event to close a position using a **limit order**.
    /// This method emits a `SignalEvent` with the specified parameters through the host callback.
    /// The order type is hardcoded as "LMT".
    ///
    /// # Arguments
    /// * `emitter` - Signal emitter of the host this strategy runs in.
    /// * `current_bar_datetime` - The timestamp associated with this signal.
    /// * `symbol` - The trading symbol (e.g., "Si-12.23").
    /// * `signal_name` - The name of the signal (e.g., "EXIT").
    /// * `quantity` - The number of contracts to trade (use `None` if not applicable or to use current position size).
    /// * `limit_price` - The specific price at which the limit order should be placed.
    ///
    /// # Returns
    /// * `anyhow::Result<()>` - `Ok(())` on successful emission, `Err` if the host callback fails.
    fn close_by_limit(
        &self,
        emitter: &event::SignalEmitter,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &str,
        signal_name: &str,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    ) -> anyhow::Result<()> {
        emitter.emit(
            current_bar_datetime,
            symbol,
            signal_name,
            "LMT",
            quantity,
            limit_price,
        )
    }

    /// Opens a position by sending a market order.
    /// # Arguments
    /// * `emitter` - Signal emitter of the host this strategy runs in.
    /// * `current_bar_datetime` - Current bar datetime.
    /// * `symbol` - Symbol to trade.
    /// * `signal_name` - Signal name (e.g., "LONG", "SHORT").
    /// * `quantity` - Quantity to trade.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn open_by_market(
        &self,
        emitter: &event::SignalEmitter,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &str,
        signal_name: &str,
        quantity: Option<f64>,
    ) -> anyhow::Result<()> {
        emitter.emit(
            current_bar_datetime,
            symbol,
            signal_name,
            "MKT",
            quantity,
            None,
        )
    }

    /// Closes a position by sending a market order.
    /// # Arguments
    /// * `emitter` - Signal emitter of the host this strategy runs in.
    /// * `current_bar_datetime` - Current bar datetime.
    /// * `symbol` - Symbol to trade.
    /// * `signal_name` - Signal name (e.g., "EXIT").
    /// * `quantity` - Quantity to trade.
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    fn close_by_market(
        &self,
        emitter: &event::SignalEmitter,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &str,
        signal_name: &str,
        quantity: Option<f64>,
    ) -> anyhow::Result<()> {
        emitter.emit(
            current_bar_datetime,
            symbol,
            signal_name,
            "MKT",
            quantity,
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One signal as the host callback sees it, decoded back into Rust types.
    #[derive(Debug, PartialEq)]
    struct CapturedSignal {
        datetime_ns: i64,
        symbol: String,
        signal_name: String,
        order_kind: String,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    }

    static CAPTURED: std::sync::Mutex<Vec<CapturedSignal>> = std::sync::Mutex::new(Vec::new());

    /// Host-side callback used by the tests: records the arguments and reports success.
    ///
    /// # Safety
    /// The three string pointers must be valid NUL-terminated C strings for the duration of the
    /// call, which is what [`event::SignalEmitter::emit`] guarantees.
    unsafe extern "C" fn capturing_cb(
        _ctx: *const std::ffi::c_void,
        datetime_ns: i64,
        symbol: *const std::os::raw::c_char,
        signal_name: *const std::os::raw::c_char,
        order_kind: *const std::os::raw::c_char,
        quantity: f64,
        limit_price: f64,
    ) -> i32 {
        // SAFETY: guaranteed by the caller (see the `# Safety` section above).
        let read = |ptr: *const std::os::raw::c_char| {
            unsafe { std::ffi::CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned()
        };
        CAPTURED.lock().unwrap().push(CapturedSignal {
            datetime_ns,
            symbol: read(symbol),
            signal_name: read(signal_name),
            order_kind: read(order_kind),
            quantity: event::nan_to_optional(quantity),
            limit_price: event::nan_to_optional(limit_price),
        });
        0
    }

    /// Host-side callback that always fails, to check error propagation.
    unsafe extern "C" fn failing_cb(
        _ctx: *const std::ffi::c_void,
        _datetime_ns: i64,
        _symbol: *const std::os::raw::c_char,
        _signal_name: *const std::os::raw::c_char,
        _order_kind: *const std::os::raw::c_char,
        _quantity: f64,
        _limit_price: f64,
    ) -> i32 {
        -1
    }

    /// The trait methods under test are the default ones, so the strategy body stays empty.
    struct TestStrategy;

    impl Strategy for TestStrategy {
        fn calculate_signals(
            &mut self,
            _data_handler: &dyn data_handler::DataHandler,
            _current_positions: &std::collections::HashMap<String, portfolio::PositionState>,
            _all_holdings: &[portfolio::HoldingSnapshot],
            _symbol_list: &[String],
        ) -> anyhow::Result<()> {
            anyhow::Ok(())
        }
    }

    #[test]
    fn default_methods_marshal_signals_into_the_callback() {
        let datetime = chrono::DateTime::from_timestamp(1_700_000_000, 123_456_789).unwrap();
        let emitter = event::SignalEmitter {
            ctx: std::ptr::null(),
            cb: capturing_cb,
        };
        let strategy = TestStrategy;
        CAPTURED.lock().unwrap().clear();

        strategy
            .open_by_limit(&emitter, datetime, "Si-12.23", "LONG", Some(2.0), None)
            .unwrap();
        strategy
            .close_by_market(&emitter, datetime, "Si-12.23", "EXIT", Some(2.0))
            .unwrap();

        let captured = CAPTURED.lock().unwrap();
        assert_eq!(
            *captured,
            vec![
                CapturedSignal {
                    datetime_ns: event::datetime_to_nanos(datetime).unwrap(),
                    symbol: "Si-12.23".to_string(),
                    signal_name: "LONG".to_string(),
                    order_kind: "LMT".to_string(),
                    quantity: Some(2.0),
                    limit_price: None,
                },
                CapturedSignal {
                    datetime_ns: event::datetime_to_nanos(datetime).unwrap(),
                    symbol: "Si-12.23".to_string(),
                    signal_name: "EXIT".to_string(),
                    order_kind: "MKT".to_string(),
                    quantity: Some(2.0),
                    limit_price: None,
                },
            ]
        );
    }

    #[test]
    fn default_methods_propagate_callback_failure() {
        let emitter = event::SignalEmitter {
            ctx: std::ptr::null(),
            cb: failing_cb,
        };
        let strategy = TestStrategy;
        let datetime = chrono::DateTime::from_timestamp(0, 0).unwrap();

        let error = strategy
            .open_by_market(&emitter, datetime, "Si-12.23", "LONG", Some(1.0))
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("Signal emission callback failed")
        );
    }
}

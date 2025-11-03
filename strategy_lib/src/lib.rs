// strategy_lib/src/lib.rs

use farukon_core::{self, strategy::Strategy};

/// A simple moving average crossover strategy.
/// This strategy generates buy/sell signals based on the crossing of two SMAs.
/// It also handles position exits based on SMA crossover or contract expiration.
pub struct MovingAverageCrossStrategy {
    /// The operational mode (e.g., "Debug", "Optimize", "Visual").
    mode: String,
    /// The settings for this strategy, loaded from the JSON config.
    strategy_settings: farukon_core::settings::StrategySettings,
    /// Instrument metadata for all symbols traded by this strategy.
    strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    /// The event sender channel used to communicate signals to other components.
    event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    /// The window size for the short-term Simple Moving Average (SMA).
    short_window: usize,
    /// The window size for the long-term Simple Moving Average (SMA).
    long_window: usize,
}

impl MovingAverageCrossStrategy {
    /// Creates a new instance of the MovingAverageCrossStrategy.
    /// Initializes the strategy with the provided mode, settings, instrument info, and event sender.
    /// It also parses the required strategy parameters (`short_window`, `long_window`) from the settings.
    ///
    /// # Arguments
    /// * `mode` - The operational mode (e.g., "Debug", "Optimize").
    /// * `strategy_settings` - The settings for this strategy, loaded from the JSON config.
    /// * `strategy_instruments_info` - Instrument metadata for all symbols traded by this strategy.
    /// * `event_sender` - The event sender channel used to communicate signals.
    ///
    /// # Returns
    /// * `anyhow::Result<Self>` - A new instance of the strategy or an error if initialization fails.
    pub fn new(
        mode: String,
        strategy_settings: farukon_core::settings::StrategySettings,
        strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    ) -> anyhow::Result<Self> {
        // Extract the short and long window sizes from the strategy settings.
        let short_window = get_param_as_usize(&strategy_settings.strategy_params, "short_window")?;
        let long_window = get_param_as_usize(&strategy_settings.strategy_params, "long_window")?;

        // Validate that the short window is less than the long window.
        if short_window >= long_window{
            anyhow::bail!("'short_window' ({}) must be less than 'long_window' ({}).", short_window, long_window);
        }

        // Create and return the new strategy instance.
        anyhow::Ok(
            MovingAverageCrossStrategy {
                mode,
                strategy_settings,
                strategy_instruments_info,
                short_window: short_window as usize,
                long_window: long_window as usize,
                event_sender,                          
            }
        )
    }

}

/// Implementation of the core Strategy trait for MovingAverageCrossStrategy.
/// This defines the main logic for calculating signals based on market data and portfolio state.
impl farukon_core::strategy::Strategy for MovingAverageCrossStrategy {
    /// Calculates trading signals based on market data and portfolio state.
    /// This function iterates through each symbol in the symbol list, calculates SMAs,
    /// checks for crossovers, and sends appropriate signals (LONG, SHORT, EXIT) via the event channel.
    ///
    /// # Arguments
    /// * `data_handler` - Interface to access market data (OHLCV, timestamps).
    /// * `current_positions` - Current position states for all symbols.
    /// * `latest_equity_point` - The latest equity point (capital, blocked, cash).
    /// * `symbol_list` - List of symbols to process signals for.
    ///
    /// # Returns
    /// * `anyhow::Result<()>` - Indicates success or failure of the signal calculation.
    fn calculate_signals(
            &mut self,
            data_handler: &dyn farukon_core::data_handler::DataHandler,
            current_positions: &std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
            latest_holdings: &farukon_core::portfolio::HoldingSnapshot,
            symbol_list: &[String],
    ) -> anyhow::Result<()> {
        // Iterate through each symbol in the list.
        for symbol in symbol_list{

            // Get the current capital from the equity point.
            let capital = Some(latest_holdings.capital);
            // Get the current capital from the equity point.
            let strategy_instruments_info_for_symbol = self.strategy_instruments_info.get(symbol).unwrap();

            // Get the current datetime for the symbol
            let current_bar_datetime = data_handler.get_latest_bar_datetime(symbol).unwrap();
            // Get the latest close price for the symbol.
            let close = Some(data_handler.get_latest_bar_value(symbol, "close").unwrap());
 
            // Get the expiration datetime for the symbol
            let expiration_date = &strategy_instruments_info_for_symbol.expiration_date;
            let expiration_date_dt = farukon_core::utils::string_to_date_time(expiration_date, "%Y-%m-%d %H:%M:%S")?;

            // Get the expiration datetime for the symbol from instrument info and parse it.
            let trade_from_date = &strategy_instruments_info_for_symbol.trade_from_date;
            let trade_from_date_dt = farukon_core::utils::string_to_date_time(trade_from_date, "%Y-%m-%d %H:%M:%S")?;

            // Get the trade_from_date for the symbol from instrument info and parse it.
            let current_position_state = current_positions.get(symbol).unwrap();
            let current_position_quantity = current_position_state.position;

            // Calculate signals
            let short_sma_bars = data_handler.get_latest_bars_values(symbol, "close", self.short_window);
            let long_sma_bars = data_handler.get_latest_bars_values(symbol, "close", self.long_window);

            if let (Some(short_sma), Some(long_sma)) = (
                farukon_core::indicators::sma(&short_sma_bars, self.short_window),
                farukon_core::indicators::sma(&long_sma_bars, self.long_window),
            ) {
                // Print debug information if in Debug mode.
                if self.mode == "Debug".to_string() {
                    println!("Start event, Indicators, {}, {}, short_sma: {}, long_sma: {}, current_position: {}", symbol, current_bar_datetime, short_sma, long_sma, current_position_quantity);
                    println!("Start event, Indicators + equity_point, {:?}", latest_holdings);
                }
                                
                // if position exist
                if current_position_quantity != 0.0 {
                    let signal_name = "EXIT";
                    // if long position
                    if current_position_quantity > 0.0 {
                        // EXIT LONG
                        if short_sma < long_sma {
                            self.close_by_market(
                                &self.event_sender,
                                current_bar_datetime,
                                symbol,
                                signal_name,
                                Some(current_position_quantity),
                            )?;
                        }
                        // EXIT by expiration
                        else if current_bar_datetime >= expiration_date_dt {
                            self.close_by_market(
                                &self.event_sender,
                                current_bar_datetime,
                                symbol,
                                signal_name,
                                Some(current_position_quantity),
                            )?;
                        }
                    }
                    // if short position
                    else {
                        // EXIT SHORT
                        if short_sma > long_sma {
                            self.close_by_market(
                                &self.event_sender,
                                current_bar_datetime,
                                symbol,
                                signal_name,
                                Some(current_position_quantity),
                            )?;
                        }
                        // EXIT by expiration
                        else if current_bar_datetime >= expiration_date_dt {
                            self.close_by_market(
                                &self.event_sender,
                                current_bar_datetime,
                                symbol,
                                signal_name,
                                Some(current_position_quantity),
                            )?;
                        } 
                    }
                }
                // if no position exist
                else {
                    // LONG
                    if short_sma > long_sma &&
                    current_bar_datetime < expiration_date_dt &&
                    current_bar_datetime >= trade_from_date_dt 
                    {
                        let signal_name = "LONG";
                        let quantity = farukon_core::pos_sizers::get_pos_sizer_from_settings(
                            &self.mode,
                            capital,
                            close,
                            Some(long_sma),
                            &self.strategy_settings,
                            strategy_instruments_info_for_symbol,
                        );

                        self.open_by_limit(
                            &self.event_sender,
                            current_bar_datetime,
                            symbol,
                            signal_name,
                            quantity,
                            close,
                        )?;

                        if self.mode == "Debug" {
                            println!("quantity: {:?}", quantity);
                        }
                    }
                    // SHORT
                    else if
                    short_sma < long_sma &&
                    current_bar_datetime < expiration_date_dt &&
                    current_bar_datetime >= trade_from_date_dt 
                    {
                        let signal_name = "SHORT";
                        let quantity = farukon_core::pos_sizers::get_pos_sizer_from_settings(
                            &self.mode,
                            capital,
                            close,
                            Some(long_sma),
                            &self.strategy_settings,
                            strategy_instruments_info_for_symbol,
                        );
                        
                        self.open_by_limit(
                            &self.event_sender,
                            current_bar_datetime,
                            symbol,
                            signal_name,
                            quantity,
                            close,
                        )?;

                        if self.mode == "Debug" {
                            println!("quantity: {:?}", quantity);
                        }
                    }
                }

                if self.mode == "Debug".to_string() {
                    println!("Finish event, Indicators, {}, {}, short_sma: {}, long_sma: {}, current_position: {}", symbol, current_bar_datetime, short_sma, long_sma, current_position_quantity);
                    println!("Finish event, Indicators + equity_point, {:?}", latest_holdings);
                }
            }
        }

        anyhow::Ok(())
    }

}

/// Helper function to extract a parameter value as usize from the strategy settings.
/// It checks for the parameter in the map, verifies it's a number, and converts it to usize.
///
/// # Arguments
/// * `params` - The map of strategy parameters.
/// * `name` - The name of the parameter to extract.
///
/// # Returns
/// * `anyhow::Result<usize>` - The parameter value as usize or an error.
fn get_param_as_usize(params: &std::collections::HashMap<String, Vec<serde_json::Value>>, name: &str) -> anyhow::Result<usize> {
    let value = params
        .get(name)
        .and_then(|v| v.first())
        .ok_or_else(|| anyhow::anyhow!("Missing parameter '{}'", name))?;

    if let Some(val) = value.as_u64() {
        anyhow::Ok(val as usize)
    } else if let Some(val) = value.as_f64() {
        anyhow::Ok(val as usize)
    } else {
        Err(anyhow::anyhow!("Parameter '{}' must be a number, got: {:?}", name, value))
    }
}

/// C function exported for dynamic loading.
/// Creates a new instance of the MovingAverageCrossStrategy.
/// This function is called by the main application when loading the strategy library.
///
/// # Arguments
/// * `mode_cstr` - A C string representing the operational mode.
/// * `strategy_settings_ptr` - A pointer to the strategy settings struct.
/// * `strategy_instruments_info_ptr` - A pointer to the instrument info map.
/// * `event_sender_ptr` - A pointer to the event sender channel.
///
/// # Returns
/// * A raw pointer to the newly created MovingAverageCrossStrategy instance, or null on error.
#[unsafe(no_mangle)]
pub extern "C" fn create_strategy(
    mode_cstr: *const std::os::raw::c_char,
    strategy_settings_ptr: *const farukon_core::settings::StrategySettings,
    strategy_instruments_info_ptr: *const std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    event_sender_ptr: *const std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
) -> *mut MovingAverageCrossStrategy {
    // Check for null pointers to prevent crashes.
    if mode_cstr.is_null() || strategy_settings_ptr.is_null() || strategy_settings_ptr.is_null() || event_sender_ptr.is_null() {
        return std::ptr::null_mut();
    }
    // Convert the C string to a Rust String.
    let mode = unsafe { std::ffi::CStr::from_ptr(mode_cstr) }.to_string_lossy().into_owned();
    // Dereference the raw pointers to get the actual values.
    let strategy_settings_ref = unsafe { &*strategy_settings_ptr }.clone();
    let strategy_instruments_info_ref = unsafe { &*strategy_instruments_info_ptr }.clone();
    let event_sender_ref = unsafe { &*event_sender_ptr }.clone();
    
    // Attempt to create a new strategy instance.
    match MovingAverageCrossStrategy::new(
        mode,
        strategy_settings_ref,
        strategy_instruments_info_ref,
        event_sender_ref,
    ) {
        // If successful, box the strategy and return a raw pointer to it.
        Ok(strategy) => Box::into_raw(Box::new(strategy)),
        // If an error occurs, return a null pointer.
        Err(_) => std::ptr::null_mut(),
    }

}

/// C function exported for dynamic loading.
/// Destroys an instance of the MovingAverageCrossStrategy.
/// This function is called by the main application when unloading the strategy library.
///
/// # Arguments
/// * `strategy` - A raw pointer to the MovingAverageCrossStrategy instance to be destroyed.
#[unsafe(no_mangle)]
pub extern "C" fn destroy_strategy(strategy: *mut MovingAverageCrossStrategy) {
    if !strategy.is_null() {
        // Reconstruct the Box from the raw pointer and let it go out of scope, triggering the Drop trait.
        unsafe {
            let _ = Box::from_raw(strategy);
        }
    }
}

/// C function exported for dynamic loading.
/// Calls the calculate_signals method on the MovingAverageCrossStrategy instance.
/// This function is called by the main application during the backtest loop.
///
/// # Arguments
/// * `strategy_ptr` - A raw pointer to the MovingAverageCrossStrategy instance.
/// * `data_handler_vtable` - A pointer to the VTable for the DataHandler trait object.
/// * `data_handler_ptr` - A pointer to the DataHandler trait object data.
/// * `current_positions_ptr` - A pointer to the map of current positions.
/// * `latest_equity_point_ptr` - A pointer to the latest equity point.
/// * `symbol_list_ptr` - A pointer to an array of C string pointers representing the symbol list.
/// * `symbol_list_size` - The size of the symbol list array.
///
/// # Returns
/// * `i32` - 0 on success, -1 on error.
#[unsafe(no_mangle)]
pub extern "C" fn calculate_signals(
    strategy_ptr: *mut std::ffi::c_void,
    data_handler_vtable: *const farukon_core::DataHandlerVTable,
    data_handler_ptr: *const (),
    current_positions_ptr: *mut std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
    latest_holdings_ptr: *mut farukon_core::portfolio::HoldingSnapshot,
    symbol_list_ptr: *const *const std::os::raw::c_char,
    symbol_list_size: usize,
) -> i32 {
    if strategy_ptr.is_null() || current_positions_ptr.is_null() || /*latest_holdings_ptr.is_null() ||*/symbol_list_ptr.is_null() {
        return -1;
    }
    // Cast the void pointer to the correct type and get a mutable reference to the strategy.
    let strategy = unsafe { &mut *(strategy_ptr as *mut MovingAverageCrossStrategy) };

    // Reconstruct the DataHandler trait object from the VTable and data pointers.
    let data_handler: &dyn farukon_core::data_handler::DataHandler = unsafe {
        std::mem::transmute::<(*const (), *const ()), &dyn farukon_core::data_handler::DataHandler>((
            data_handler_ptr,
            data_handler_vtable as *const(),
        ))
    };

    // Get mutable references to the current positions and latest equity point.
    let current_positions = unsafe { &mut *current_positions_ptr };
    let latest_holdings = unsafe { &mut *latest_holdings_ptr };

    // Convert the C string array to a Vec<String>.
    let symbols: Vec<String> = (0..symbol_list_size)
        .filter_map(|i| unsafe {
            let str_ptr = *symbol_list_ptr.add(i);
            if str_ptr.is_null() { return None; }
            std::ffi::CStr::from_ptr(str_ptr)
                .to_str()
                .ok()
                .map(|s| s.to_string())
        })
        .collect();

    // Call the Rust calculate_signals method and return 0 on success or -1 on error.
    match strategy.calculate_signals(
        data_handler,
        current_positions,
        latest_holdings,
        &symbols,
    ) {
        Ok(_) => 0,
        Err(_) => -1,
    }
    
}

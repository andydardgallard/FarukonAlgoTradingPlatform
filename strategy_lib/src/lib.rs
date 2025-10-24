// strategy_lib/src/lib.rs

use farukon_core::{self, strategy::Strategy};

pub struct MovingAverageCrossStrategy {
    mode: String,
    strategy_settings: farukon_core::settings::StrategySettings,
    strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    short_window: usize,
    long_window: usize,
}

impl MovingAverageCrossStrategy {
    pub fn new(
        mode: String,
        strategy_settings: farukon_core::settings::StrategySettings,
        strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    ) -> anyhow::Result<Self> {
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

        let short_window = get_param_as_usize(&strategy_settings.strategy_params, "short_window")?;
        let long_window = get_param_as_usize(&strategy_settings.strategy_params, "long_window")?;

        if short_window >= long_window{
            anyhow::bail!("'short_window' ({}) must be less than 'long_window' ({}).", short_window, long_window);
        }

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

impl farukon_core::strategy::Strategy for MovingAverageCrossStrategy {
    fn calculate_signals(
            &mut self,
            data_handler: &dyn farukon_core::data_handler::DataHandler,
            current_positions: &std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
            latest_equity_point: &farukon_core::portfolio::EquitySnapshot,
            symbol_list: &[String],
    ) -> anyhow::Result<()> {
        for symbol in symbol_list{

            let capital = Some(latest_equity_point.equity_point.capital);
            let strategy_instruments_info_for_symbol = self.strategy_instruments_info.get(symbol).unwrap();

            // Get the current datetime for the symbol
            let current_bar_datetime = data_handler.get_latest_bar_datetime(symbol).unwrap();
            let close = Some(data_handler.get_latest_bar_value(symbol, "close").unwrap());
 
            // Get the expiration datetime for the symbol
            let expiration_date = &strategy_instruments_info_for_symbol.expiration_date;
            let expiration_date_dt = farukon_core::utils::string_to_date_time(expiration_date, "%Y-%m-%d %H:%M:%S")?;

            // Get trade_from_date for symbol
            let trade_from_date = &strategy_instruments_info_for_symbol.trade_from_date;
            let trade_from_date_dt = farukon_core::utils::string_to_date_time(trade_from_date, "%Y-%m-%d %H:%M:%S")?;

            let current_position_state = current_positions.get(symbol).unwrap();
            let current_position_quantity = current_position_state.position;

            // Calculate signals
            if let (Some(short_sma), Some(long_sma)) = (
                farukon_core::indicators::sma(data_handler, symbol, "close", self.short_window, 0),
                farukon_core::indicators::sma(data_handler, symbol, "close", self.long_window, 0),
            ) {
                if self.mode == "Debug".to_string() {
                    println!("Start event, Indicators, {}, {}, short_sma: {}, long_sma: {}, current_position: {}", symbol, current_bar_datetime, short_sma, long_sma, current_position_quantity);
                    println!("Start event, Indicators + equity_point, {:?}", latest_equity_point);
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
                    println!("Finish event, Indicators + equity_point, {:?}", latest_equity_point);
                }
            }
        }

        anyhow::Ok(())
    }

}

#[unsafe(no_mangle)]
pub extern "C" fn create_strategy(
    mode_cstr: *const std::os::raw::c_char,
    strategy_settings_ptr: *const farukon_core::settings::StrategySettings,
    strategy_instruments_info_ptr: *const std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    event_sender_ptr: *const std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
) -> *mut MovingAverageCrossStrategy {
    if mode_cstr.is_null() || strategy_settings_ptr.is_null() || strategy_settings_ptr.is_null() || event_sender_ptr.is_null() {
        return std::ptr::null_mut();
    }
    let mode = unsafe { std::ffi::CStr::from_ptr(mode_cstr) }.to_string_lossy().into_owned();
    let strategy_settings_ref = unsafe { &*strategy_settings_ptr }.clone();
    let strategy_instruments_info_ref = unsafe { &*strategy_instruments_info_ptr }.clone();
    let event_sender_ref = unsafe { &*event_sender_ptr }.clone();
    
    match MovingAverageCrossStrategy::new(
        mode,
        strategy_settings_ref,
        strategy_instruments_info_ref,
        event_sender_ref,
    ) {
        Ok(strategy) => Box::into_raw(Box::new(strategy)),
        Err(_) => std::ptr::null_mut(),
    }

}

#[unsafe(no_mangle)]
pub extern "C" fn destroy_strategy(strategy: *mut MovingAverageCrossStrategy) {
    if !strategy.is_null() {
        unsafe {
            let _ = Box::from_raw(strategy);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn calculate_signals(
    strategy_ptr: *mut std::ffi::c_void,
    data_handler_vtable: *const farukon_core::DataHandlerVTable,
    data_handler_ptr: *const (),
    current_positions_ptr: *mut std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
    latest_equity_point_ptr: *mut farukon_core::portfolio::EquitySnapshot,
    symbol_list_ptr: *const *const std::os::raw::c_char,
    symbol_list_size: usize,
) -> i32 {
    if strategy_ptr.is_null() || current_positions_ptr.is_null() || latest_equity_point_ptr.is_null() || symbol_list_ptr.is_null() {
        return -1;
    }
    let strategy = unsafe { &mut *(strategy_ptr as *mut MovingAverageCrossStrategy) };

    let data_handler: &dyn farukon_core::data_handler::DataHandler = unsafe {
        std::mem::transmute::<(*const (), *const ()), &dyn farukon_core::data_handler::DataHandler>((
            data_handler_ptr,
            data_handler_vtable as *const(),
        ))
    };

    let current_positions = unsafe { &mut *current_positions_ptr };
    let latest_equity_point = unsafe { &mut *latest_equity_point_ptr };

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

    match strategy.calculate_signals(
        data_handler,
        current_positions,
        latest_equity_point,
        &symbols,
    ) {
        Ok(_) => 0,
        Err(_) => -1,
    }
    
}

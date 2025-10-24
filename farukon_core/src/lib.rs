// farukon_core/src/lib.rs

pub mod event;
pub mod index;
pub mod utils;
pub mod strategy;
pub mod settings;
pub mod portfolio;
pub mod execution;
pub mod indicators;
pub mod pos_sizers;
pub mod performance;
pub mod data_handler;
pub mod optimization;
pub mod instruments_info;
pub mod commission_plans;

#[repr(C)]
pub struct DataHandlerVTable {
    get_latest_bar: unsafe fn(*const (), &str) -> Option<&'static data_handler::MarketBar>,
    get_latest_bars: unsafe fn(*const (), &str, usize) -> Vec<&'static data_handler::MarketBar>,
    get_latest_bar_datetime: unsafe fn(*const (), &str) -> Option<chrono::DateTime<chrono::Utc>>,
    get_latest_bar_value: unsafe fn(*const (), &str, &str) -> Option<f64>,
    get_latest_bar_values: unsafe fn(*const (), &str, &str, usize) -> Vec<f64>,
    update_bars: unsafe fn(*const ()) -> (),
    get_continue_backtest: unsafe fn(*const ()) -> bool,
    set_continue_backtest: unsafe fn(*const (), bool) -> (),
}

impl data_handler::DataHandler for DataHandlerVTable {
    fn get_latest_bar(&self, symbol: &str) -> Option<&data_handler::MarketBar> {
        unsafe {
            (self.get_latest_bar)(self as *const _ as *const (), symbol)
        }
    }
    fn get_latest_bars(&self, symbol: &str, size: usize) -> Vec<&data_handler::MarketBar> {
        unsafe {
            (self.get_latest_bars)(self as *const _ as *const (), symbol, size)
        }
    }
    fn get_latest_bar_datetime(&self, symbol: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        unsafe {
            (self.get_latest_bar_datetime)(self as *const _ as *const (), symbol)
        }
    }
    fn get_latest_bar_value(&self, symbol: &str, val_type: &str) -> Option<f64> {
        unsafe {
            (self.get_latest_bar_value)(self as *const _ as *const (), symbol, val_type)
        }
    }
    fn get_latest_bars_values(&self, symbol: &str, val_type: &str, n: usize) -> Vec<f64> {
        unsafe {
            (self.get_latest_bar_values)(self as *const _ as *const (), symbol, val_type, n)
        }
    }
    fn update_bars(&mut self) {
        unsafe {
            (self.update_bars)(self as *const _ as *const ())
        }
    }
    fn get_continue_backtest(&self) -> bool {
        unsafe {
            (self.get_continue_backtest)(self as *const _ as *const ())
        }
    }
    fn set_continue_backtest(&mut self, value: bool) {
        unsafe {
            (self.set_continue_backtest)(self as *const _ as *const (), value)
        }
    }

}

impl DataHandlerVTable {
    pub unsafe fn from_raw_parts<'a>(
        vtable: *const Self,
        _data: *const ()
    ) -> &'a Self {
        unsafe { &*vtable }
    }

}

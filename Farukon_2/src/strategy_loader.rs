// Farukon_2_0/src/strategy_loader.rs

//! Dynamic strategy loader: loads compiled Rust libraries (.dylib/.so/.dll) at runtime.
//! Enables hot-swapping of trading logic without recompiling the core engine.
//! Uses `libloading` to load symbols: create_strategy, destroy_strategy, calculate_signals.

pub struct DynamicStratagy {
    _lib: libloading::Library,  // Holds reference to loaded library
    strategy_ptr: *mut std::ffi::c_void,    // Pointer to strategy instance
    destroy_fn: libloading::Symbol<'static, extern "C" fn(*mut std::ffi::c_void)>,  // Destructor
}

impl DynamicStratagy {
    pub fn load_from_path(
        mode: &str,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        event_sender: &std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    ) -> anyhow::Result<Self> {
        // Loads dynamic strategy library and creates strategy instance.
        // Expects 3 exported C functions: create_strategy, destroy_strategy, calculate_signals.

        let lib_path = &strategy_settings.strategy_path;
        let lib = unsafe { libloading::Library::new(lib_path)? };

        let create_strategy: libloading::Symbol<extern "C" fn(
            *const std::os::raw::c_char,
            *const farukon_core::settings::StrategySettings,
            *const std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
            *const std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
        ) -> *mut std::ffi::c_void> = unsafe { lib.get(b"create_strategy")? };
 
        let destroy_strategy: libloading::Symbol<extern "C" fn(*mut std::ffi::c_void)> =
            unsafe { lib.get(b"destroy_strategy")? };
        let mode_c = std::ffi::CString::new(mode)?;
            
        let strategy_ptr = create_strategy(
            mode_c.as_ptr(),
            strategy_settings as *const _,
            strategy_instruments_info as *const _,
            event_sender as *const _,
        );

        if strategy_ptr.is_null() {
            return Err(anyhow::anyhow!("Failed to create strategy"));
        }

        let destroy_fn: libloading::Symbol<'static, extern "C" fn(*mut std::ffi::c_void)> = 
            unsafe { std::mem::transmute(destroy_strategy) };

        anyhow::Ok(DynamicStratagy {
            _lib: lib,
            strategy_ptr,
            destroy_fn,
        })
    }

    pub fn calculate_signals(
        &self,
        data_handler: &dyn farukon_core::data_handler::DataHandler,
        current_positions: &std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
        latest_holdings: &farukon_core::portfolio::HoldingSnapshot,
        symbol_list: &[String],
    ) -> anyhow::Result<()> {
        // Calls calculate_signals() from the loaded library.
        // Transforms Rust types into C-compatible pointers.
        // Returns 0 on success, -1 on error.

        let calculate_signals_fn: libloading::Symbol<extern "C" fn (
            *mut std::ffi::c_void,
            *const farukon_core::DataHandlerVTable,
            *const (),
            *const std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
            *const farukon_core::portfolio::HoldingSnapshot,
            *const *const std::os::raw::c_char,
            usize,
        ) -> i32> = unsafe {
            self._lib.get(b"calculate_signals")?
        };

        let (data_handler_ptr, data_handler_vtable) = unsafe {
            std::mem::transmute::<_, (*const (), *const farukon_core::DataHandlerVTable)>(data_handler)
        };

        let c_strings: Vec<std::ffi::CString> = symbol_list
            .iter()
            .map(|s| std::ffi::CString::new(s.as_str()))
            .collect::<anyhow::Result<Vec<_>, _>>()?;

        let c_str_ptrs: Vec<*const std::os::raw::c_char> = c_strings
            .iter()
            .map(|s| s.as_ptr())
            .collect();

        let result = calculate_signals_fn(
            self.strategy_ptr,
            data_handler_vtable,
            data_handler_ptr,
            current_positions as *const _,
            latest_holdings as *const _,
            c_str_ptrs.as_ptr(),
            symbol_list.len(),
        );

        if result == 0{
            anyhow::Ok(())
        } else {
            Err(anyhow::anyhow!("Strategy calculate_signals failed with code: {}", result))
        }
    }

}

impl Drop for DynamicStratagy {
    fn drop(&mut self) {
        // Ensures strategy is destroyed when this object goes out of scope.
        // Prevents memory leaks.

        if !self.strategy_ptr.is_null() {
                (self.destroy_fn)(self.strategy_ptr);
        }
    }
}

// Allows DynamicStratagy to be safely shared between threads (used in Grid Search).
unsafe impl Send for DynamicStratagy {}
unsafe impl Sync for DynamicStratagy {}

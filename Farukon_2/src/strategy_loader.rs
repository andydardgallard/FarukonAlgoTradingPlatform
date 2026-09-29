//! Farukon_2_0/src/strategy_loader.rs

//! Dynamic strategy loader: loads compiled Rust libraries (.dylib/.so/.dll) at runtime.
//! Enables hot-swapping of trading logic without recompiling the core engine.
//! Uses `libloading` to load symbols: create_strategy, destroy_strategy, calculate_signals.

/// FFI signature of the strategy's `calculate_signals` entry point.
///
/// Resolved once at construction and cached in `DynamicStratagy::calculate_signals_fn`.
/// The portfolio pointers are declared `*const` because the strategy only reads them — this must
/// match the exported function in `strategy_lib` (see its `# Safety` section) exactly.
/// Caller invariant: one instance is never invoked concurrently (see the `unsafe impl` notes below).
type CalculateSignalsFn = unsafe extern "C" fn(
    *mut std::ffi::c_void,
    *const farukon_core::DataHandlerVTable,
    *const (),
    *const std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
    *const Vec<farukon_core::portfolio::HoldingSnapshot>,
    *const *const std::os::raw::c_char,
    usize,
) -> i32;

/// FFI signature of the strategy's `create_strategy` entry point.
///
/// Must match the exported function in the loaded strategy library exactly. The last two arguments
/// form the signal-emission contract: `emitter_ctx` is an opaque host pointer that the library only
/// stores and passes back, and `Option<EmitSignalFn>` carries the host callback (`None` models a
/// null function pointer so both sides can reject it). Neither a heap object nor a channel endpoint
/// is handed over by value — the library owns nothing that the host allocates.
type CreateStrategyFn = unsafe extern "C" fn(
    *const std::os::raw::c_char,
    *const farukon_core::settings::StrategySettings,
    *const std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    *const std::ffi::c_void,
    Option<farukon_core::event::EmitSignalFn>,
) -> *mut std::ffi::c_void;

/// FFI signature of the strategy's `destroy_strategy` entry point.
type DestroyStrategyFn = unsafe extern "C" fn(*mut std::ffi::c_void);

/// Host-side implementation of [`farukon_core::event::EmitSignalFn`], handed to the strategy
/// library inside `create_strategy`.
///
/// The library cannot allocate the `SignalEvent` itself: host and library have separate allocators
/// (the host links mimalloc), so an event boxed there would be dropped here through the wrong
/// allocator. Instead the library marshals the signal into copyable arguments, this callback (which
/// always runs inside the host) turns them into an exe-allocated `Box<dyn Event>` and sends it into
/// the host's event channel.
///
/// # Safety
/// Called by the strategy library through the `ctx`/`cb` pair it received from `create_strategy`:
/// * `ctx` must be exactly the pointer the host passed there — a live
///   `Sender<Box<dyn Event>>` that outlives the strategy instance (see `new_from_library`). It is
///   only borrowed here, never freed or cloned out of the host.
/// * `symbol`, `signal_name`, `order_kind` must be null or point to valid NUL-terminated UTF-8 C
///   strings that stay alive for the duration of the call; they are copied immediately.
///
/// Null or invalid arguments are reported as `-1` instead of being dereferenced.
unsafe extern "C" fn emit_signal(
    ctx: *const std::ffi::c_void,
    datetime_ns: i64,
    symbol: *const std::os::raw::c_char,
    signal_name: *const std::os::raw::c_char,
    order_kind: *const std::os::raw::c_char,
    quantity: f64,
    limit_price: f64,
) -> i32 {
    if ctx.is_null() || symbol.is_null() || signal_name.is_null() || order_kind.is_null() {
        return -1;
    }

    // SAFETY: `ctx` is the pointer the host itself handed to `create_strategy` (see the `# Safety`
    // section above): it points to an event sender that stays alive for the whole strategy lifetime
    // and is only read here.
    let event_sender =
        unsafe { &*(ctx as *const std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>) };

    // SAFETY: the three pointers are non-null and, per the contract, NUL-terminated UTF-8 C strings
    // valid for the call; the contents are copied into owned `String`s right away.
    let (Some(symbol), Some(signal_name), Some(order_kind)) = (unsafe {
        (
            std::ffi::CStr::from_ptr(symbol).to_str().ok(),
            std::ffi::CStr::from_ptr(signal_name).to_str().ok(),
            std::ffi::CStr::from_ptr(order_kind).to_str().ok(),
        )
    }) else {
        return -1;
    };

    let Some(datetime) = farukon_core::event::nanos_to_datetime(datetime_ns) else {
        return -1;
    };

    // The event is allocated here, in the host, so the host owns it end to end.
    let event = farukon_core::event::SignalEvent::new(
        datetime,
        symbol.to_owned(),
        signal_name.to_owned(),
        order_kind.to_owned(),
        farukon_core::event::nan_to_optional(quantity),
        farukon_core::event::nan_to_optional(limit_price),
    );

    match event_sender.send(Box::new(event)) {
        Ok(()) => 0,
        Err(_) => -1,
    }
}

pub struct DynamicStratagy {
    _lib: std::sync::Arc<libloading::Library>, // Holds reference to loaded library
    strategy_ptr: *mut std::ffi::c_void,       // Pointer to strategy instance
    destroy_fn: libloading::Symbol<'static, unsafe extern "C" fn(*mut std::ffi::c_void)>, // Destructor
    /// `calculate_signals` resolved once per instance (previously one symbol lookup per bar).
    ///
    /// SAFETY: the symbol borrows from the loaded library. The transmute to `'static` follows the
    /// same pattern as `destroy_fn` and is sound because `_lib` (`Arc<Library>`) is owned by this
    /// struct, so the library cannot be unloaded while the struct is alive, and `Drop::drop` (the
    /// last use of the library) runs before the struct's fields are released.
    calculate_signals_fn: libloading::Symbol<'static, CalculateSignalsFn>,
    /// `strategy_settings.symbols` converted to C strings once per instance.
    ///
    /// INVARIANT: never mutated after construction — `symbol_c_ptrs` points into these buffers.
    symbol_c_strings: Vec<std::ffi::CString>,
    /// Pointers into `symbol_c_strings`, built after that `Vec` is complete and never mutated.
    ///
    /// Moving the `Vec<CString>` (or the struct) only moves the owning pointer, not the heap
    /// buffers the pointers refer to, so they stay valid for the whole lifetime of the struct.
    symbol_c_ptrs: Vec<*const std::os::raw::c_char>,
}

impl DynamicStratagy {
    pub fn new_from_library(
        mode: &str,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<
            String,
            farukon_core::instruments_info::InstrumentInfo,
        >,
        event_sender: &std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
        library: std::sync::Arc<libloading::Library>,
    ) -> anyhow::Result<Self> {
        let create_strategy: libloading::Symbol<CreateStrategyFn> =
            unsafe { library.get(b"create_strategy")? };

        let destroy_strategy: libloading::Symbol<DestroyStrategyFn> =
            unsafe { library.get(b"destroy_strategy")? };
        let mode_c = std::ffi::CString::new(mode)?;

        // SAFETY: the three pointers come from values that are alive for the call (`mode_c` and the
        // caller-provided references); the strategy clones what it needs and retains neither. The
        // emission pair borrows `event_sender` instead of moving it: the library stores the pointer
        // and hands it back on every signal, so the caller must keep this sender alive for as long
        // as the returned `DynamicStratagy` lives (both callers in `optimizers.rs` declare
        // `event_sender` before the backtest and only drop it afterwards). `emit_signal` is a
        // function with static lifetime, never owned by the library.
        let strategy_ptr = unsafe {
            create_strategy(
                mode_c.as_ptr(),
                strategy_settings as *const _,
                strategy_instruments_info as *const _,
                event_sender as *const _ as *const std::ffi::c_void,
                Some(emit_signal),
            )
        };

        if strategy_ptr.is_null() {
            return Err(anyhow::anyhow!("Failed to create strategy"));
        }

        let destroy_fn: libloading::Symbol<'static, DestroyStrategyFn> =
            unsafe { std::mem::transmute(destroy_strategy) };

        let calculate_signals_fn: libloading::Symbol<'static, CalculateSignalsFn> = unsafe {
            std::mem::transmute(library.get::<CalculateSignalsFn>(b"calculate_signals")?)
        };

        // `symbols` is immutable for the whole backtest, so the C buffer is built once.
        let (symbol_c_strings, symbol_c_ptrs) =
            Self::build_symbol_buffers(&strategy_settings.symbols)?;

        anyhow::Ok(DynamicStratagy {
            _lib: std::sync::Arc::clone(&library),
            strategy_ptr,
            destroy_fn,
            calculate_signals_fn,
            symbol_c_strings,
            symbol_c_ptrs,
        })
    }

    pub fn _load_from_path(
        mode: &str,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<
            String,
            farukon_core::instruments_info::InstrumentInfo,
        >,
        event_sender: &std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    ) -> anyhow::Result<Self> {
        // Loads dynamic strategy library and creates strategy instance.
        // Expects 3 exported C functions: create_strategy, destroy_strategy, calculate_signals.

        let lib_path = &strategy_settings.strategy_path;
        let lib = unsafe { libloading::Library::new(lib_path)? };

        let create_strategy: libloading::Symbol<CreateStrategyFn> =
            unsafe { lib.get(b"create_strategy")? };

        let destroy_strategy: libloading::Symbol<DestroyStrategyFn> =
            unsafe { lib.get(b"destroy_strategy")? };
        let mode_c = std::ffi::CString::new(mode)?;

        // SAFETY: the three pointers come from values that are alive for the call (`mode_c` and the
        // caller-provided references); the strategy clones what it needs and retains neither. The
        // emission pair borrows `event_sender` (same contract as `new_from_library`).
        let strategy_ptr = unsafe {
            create_strategy(
                mode_c.as_ptr(),
                strategy_settings as *const _,
                strategy_instruments_info as *const _,
                event_sender as *const _ as *const std::ffi::c_void,
                Some(emit_signal),
            )
        };

        if strategy_ptr.is_null() {
            return Err(anyhow::anyhow!("Failed to create strategy"));
        }

        let destroy_fn: libloading::Symbol<'static, DestroyStrategyFn> =
            unsafe { std::mem::transmute(destroy_strategy) };

        let calculate_signals_fn: libloading::Symbol<'static, CalculateSignalsFn> =
            unsafe { std::mem::transmute(lib.get::<CalculateSignalsFn>(b"calculate_signals")?) };

        // `symbols` is immutable for the whole backtest, so the C buffer is built once.
        let (symbol_c_strings, symbol_c_ptrs) =
            Self::build_symbol_buffers(&strategy_settings.symbols)?;

        anyhow::Ok(DynamicStratagy {
            _lib: std::sync::Arc::new(lib),
            strategy_ptr,
            destroy_fn,
            calculate_signals_fn,
            symbol_c_strings,
            symbol_c_ptrs,
        })
    }

    /// Converts a symbol list into C strings plus the matching array of pointers.
    ///
    /// The pointers are taken only after the `Vec<CString>` is fully built, and the returned
    /// `Vec` is never resized or mutated afterwards, so the pointers stay valid while it lives.
    fn build_symbol_buffers(
        symbols: &[String],
    ) -> anyhow::Result<(Vec<std::ffi::CString>, Vec<*const std::os::raw::c_char>)> {
        let c_strings: Vec<std::ffi::CString> = symbols
            .iter()
            .map(|s| std::ffi::CString::new(s.as_str()))
            .collect::<anyhow::Result<Vec<_>, _>>()?;

        let c_str_ptrs: Vec<*const std::os::raw::c_char> =
            c_strings.iter().map(|s| s.as_ptr()).collect();

        anyhow::Ok((c_strings, c_str_ptrs))
    }

    pub fn calculate_signals(
        &self,
        data_handler: &dyn farukon_core::data_handler::DataHandler,
        current_positions: &std::collections::HashMap<
            String,
            farukon_core::portfolio::PositionState,
        >,
        all_holdings: &Vec<farukon_core::portfolio::HoldingSnapshot>,
        symbol_list: &[String],
    ) -> anyhow::Result<()> {
        // Calls calculate_signals() from the loaded library.
        // Transforms Rust types into C-compatible pointers.
        // Returns 0 on success, -1 on error.
        // The FFI symbol and the `symbol_list` C buffers are resolved/built once in the
        // constructor: this method runs on every bar of every candidate (see backtest.rs).

        // Fast path: the event loop passes `strategy_settings.symbols`, the same list the
        // instance was built from, so the cached buffers are reused for every bar.
        let matches_cached_symbols = symbol_list.len() == self.symbol_c_strings.len()
            && symbol_list
                .iter()
                .zip(self.symbol_c_strings.iter())
                .all(|(symbol, cached)| cached.as_bytes() == symbol.as_bytes());

        if matches_cached_symbols {
            return self.invoke_calculate_signals(
                data_handler,
                current_positions,
                all_holdings,
                &self.symbol_c_ptrs,
            );
        }

        // Defensive fallback for a symbol list that differs from the construction-time settings
        // (no caller does this today): rebuild locally, keeping the previous semantics.
        let (_c_strings, c_str_ptrs) = Self::build_symbol_buffers(symbol_list)?;
        self.invoke_calculate_signals(data_handler, current_positions, all_holdings, &c_str_ptrs)
    }

    /// Invokes the cached `calculate_signals` entry point with an already-built symbol buffer.
    fn invoke_calculate_signals(
        &self,
        data_handler: &dyn farukon_core::data_handler::DataHandler,
        current_positions: &std::collections::HashMap<
            String,
            farukon_core::portfolio::PositionState,
        >,
        all_holdings: &Vec<farukon_core::portfolio::HoldingSnapshot>,
        c_str_ptrs: &[*const std::os::raw::c_char],
    ) -> anyhow::Result<()> {
        let (data_handler_ptr, data_handler_vtable) = unsafe {
            std::mem::transmute::<
                &dyn farukon_core::data_handler::DataHandler,
                (*const (), *const farukon_core::DataHandlerVTable),
            >(data_handler)
        };

        // SAFETY: the cached symbol was resolved from the library this instance keeps alive; the
        // portfolio pointers come from references that outlive the call and are only read by the
        // strategy; the caller invariant is one instance per candidate, never called concurrently
        // (see the `Sync` comment below).
        let result = unsafe {
            (self.calculate_signals_fn)(
                self.strategy_ptr,
                data_handler_vtable,
                data_handler_ptr,
                current_positions as *const _,
                all_holdings as *const _,
                c_str_ptrs.as_ptr(),
                c_str_ptrs.len(),
            )
        };

        if result == 0 {
            anyhow::Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Strategy calculate_signals failed with code: {}",
                result
            ))
        }
    }
}

impl Drop for DynamicStratagy {
    fn drop(&mut self) {
        // Ensures strategy is destroyed when this object goes out of scope.
        // Prevents memory leaks.

        if !self.strategy_ptr.is_null() {
            // SAFETY: `strategy_ptr` was returned by this library's `create_strategy`, is destroyed
            // exactly once here, and the owning library is still loaded (`_lib` is dropped after
            // this function returns).
            unsafe { (self.destroy_fn)(self.strategy_ptr) };
        }
    }
}

// SAFETY (`Send`): `DynamicStratagy` owns a heap-allocated strategy instance created inside the
// loaded library (`strategy_ptr` is the value `Box::into_raw` returned) plus the library handle
// `_lib: Arc<Library>`. Grid Search creates a candidate's strategy on the worker thread that runs
// it and may move the whole `DynamicStratagy` to another rayon thread, so `Send` is required.
// Moving it transfers the sole owner, and the raw pointers it carries (`strategy_ptr`,
// `symbol_c_ptrs`) point at heap allocations that are independent of the struct's address, so they
// stay valid after the move. The instance also holds the emission context handed to
// `create_strategy` (a pointer to the caller's event sender): that sender is owned by the caller and
// outlives the instance, and moving this struct does not move it. The cached `Symbol<'static>`
// values and both buffers are backed by `_lib`, which this struct keeps alive (see the field docs
// above); `Drop` destroys the strategy before that handle is released.
unsafe impl Send for DynamicStratagy {}

// SAFETY (`Sync`): sound only under the caller invariant that no two threads ever call
// `calculate_signals` (or `Drop`) on the same instance concurrently. Today this holds because
// exactly one candidate owns each instance for its whole lifetime (see AGENTS.md "Isolation per
// candidate"); `Sync` is declared so the instance can be borrowed from whichever rayon thread is
// processing its candidate, not to permit parallel calls. The FFI entry point takes `&mut` to the
// strategy and `&` to the portfolio state, and a signal emitted from there dereferences the host's
// event sender through the emission context, so concurrent calls would create aliasing `&mut`s and
// interleaved access to the host's portfolio and channel — UB. Any future change that shares one
// instance between threads must wrap it in a lock instead of relying on these impls.
unsafe impl Sync for DynamicStratagy {}

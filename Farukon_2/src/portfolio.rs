//! Farukon_2_0/src/portfolio.rs

//! Portfolio manager: tracks positions, holdings, equity, and risk.
//! Implements PortfolioHandler trait for integration with Backtest.
//! Handles fill events, signal events, and margin calls.

use farukon_core::{self, portfolio::PortfolioHandler};

use crate::risks;

pub struct Portfolio {
    /// Operational mode (Debug, Optimize, Visual).
    mode: String,

    initial_capital_for_strategy: f64,

    /// Event sender for communicating with other components.
    event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,

    /// Strategy settings for this portfolio.
    strategy_settings: farukon_core::settings::StrategySettings,

    /// Instrument metadata for all traded instruments.
    strategy_instruments_info:
        std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,

    /// Current position state for each symbol.
    current_positions: std::collections::HashMap<String, farukon_core::portfolio::PositionState>,

    /// Current holding state for each symbol.
    current_holdings: std::collections::HashMap<String, farukon_core::portfolio::HoldingsState>,

    /// Historical snapshots of positions.
    all_positions: Vec<farukon_core::portfolio::PositionSnapshot>,

    /// Historical snapshots of holdings.
    all_holdings: Vec<farukon_core::portfolio::HoldingSnapshot>,

    /// Equity curve for plotting.
    equity_series: Vec<(chrono::DateTime<chrono::Utc>, f64)>,

    /// Performance manager for calculating metrics.
    performance_manager: farukon_core::performance::PerformanceManager,

    /// `true` once the data is exhausted (`begin_final_settle`): fills from then on are booked
    /// against [`Portfolio::last_marked_close`] instead of the previous bar's close.
    final_settle: bool,

    /// Close at which each symbol was last marked to market, i.e. the last close whose market
    /// move is already part of the recorded capital. It is the booked-through reference for the
    /// fills that arrive after the last regular `update_timeindex`:
    /// * advanced to the bar's close whenever `update_timeindex` books a market move;
    /// * advanced to the bar's close by an entry fill, whose PnL marks the new position there;
    /// * left untouched by an `EXIT` fill, whose PnL realizes the position at the execution price
    ///   while any remaining contracts are still marked at the previous close.
    last_marked_close: std::collections::HashMap<String, f64>,

    /// Net PnL (already minus commission) of the fills that arrived in the final settle phase and
    /// have not been booked yet, accumulated per symbol.
    ///
    /// `HoldingsState::pnl` keeps only the latest fill for a symbol, so several final fills for the
    /// same symbol (e.g. two partial `EXIT`s) would otherwise lose all but the last one. Cleared by
    /// `update_final_timeindex` once the amount is booked. Always empty outside the final settle
    /// phase, so the regular loop behaviour is unchanged.
    pending_fill_pnl: std::collections::HashMap<String, f64>,
}

impl Portfolio {
    /// Creates a new Portfolio instance.
    /// # Arguments
    /// * `mode` - Operational mode.
    /// * `event_sender` - Sender for events.
    /// * `strategy_settings` - Strategy configuration.
    /// * `strategy_instruments_info` - Instrument metadata.
    /// * `initial_capital_for_strategy` - Starting capital for this strategy.
    pub fn new(
        mode: &String,
        initial_capital_for_strategy: &f64,
        event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<
            String,
            farukon_core::instruments_info::InstrumentInfo,
        >,
    ) -> anyhow::Result<Self> {
        anyhow::Ok(Self {
            mode: mode.to_string(),
            initial_capital_for_strategy: *initial_capital_for_strategy,
            event_sender,
            strategy_settings: strategy_settings.clone(),
            strategy_instruments_info: strategy_instruments_info.clone(),
            current_positions: Self::construct_current_positions(strategy_settings),
            current_holdings: Self::construct_current_holdings(strategy_settings),
            all_positions: Vec::new(),
            all_holdings: Vec::new(),
            equity_series: Vec::new(),
            performance_manager: farukon_core::performance::PerformanceManager::new(
                *initial_capital_for_strategy,
                strategy_settings,
            ),
            final_settle: false,
            last_marked_close: std::collections::HashMap::new(),
            pending_fill_pnl: std::collections::HashMap::new(),
        })
    }

    /// Constructs the initial position state for each symbol in the strategy.
    fn construct_current_positions(
        strategy_settings: &farukon_core::settings::StrategySettings,
    ) -> std::collections::HashMap<String, farukon_core::portfolio::PositionState> {
        // Initializes empty position state for each symbol in strategy.

        let mut positions: std::collections::HashMap<
            String,
            farukon_core::portfolio::PositionState,
        > = std::collections::HashMap::new();
        for symbol in &strategy_settings.symbols {
            let current_positions_for_symbol: farukon_core::portfolio::PositionState =
                farukon_core::portfolio::PositionState::new();
            positions.insert(symbol.clone(), current_positions_for_symbol);
        }
        positions
    }

    /// Constructs the initial holding state for each symbol in the strategy.
    fn construct_current_holdings(
        strategy_settings: &farukon_core::settings::StrategySettings,
    ) -> std::collections::HashMap<String, farukon_core::portfolio::HoldingsState> {
        // Initializes empty holding state for each symbol.

        let mut holdings: std::collections::HashMap<
            String,
            farukon_core::portfolio::HoldingsState,
        > = std::collections::HashMap::new();
        for symbol in &strategy_settings.symbols {
            let current_holdings_for_symbol: farukon_core::portfolio::HoldingsState =
                farukon_core::portfolio::HoldingsState::new();
            holdings.insert(symbol.clone(), current_holdings_for_symbol);
        }
        holdings
    }

    /// Generates an order event from a signal event.
    /// Applies position sizing and margin control.
    /// # Arguments
    /// * `signal_event` - The signal event received from the strategy.
    /// * `data_handler` - The data handler for accessing market data.
    /// # Returns
    /// * `Ok(Some(order))` when the signal becomes an executable order, `Ok(None)` when the
    ///   signal produces no trade (zero size, margin call rejection), and `Err` when the signal
    ///   carries no quantity (the position sizer returned `None`) or market data is missing.
    fn generate_order(
        &self,
        signal_event: &farukon_core::event::SignalEvent,
        data_handler: &dyn farukon_core::data_handler::DataHandler,
    ) -> anyhow::Result<Option<farukon_core::event::OrderEvent>> {
        // Converts a SIGNAL event into an ORDER event.
        // Uses position sizer to determine quantity.
        // Applies margin control to prevent over-leverage.

        let signal_name = &signal_event.signal_name;
        let symbol = &signal_event.symbol;
        let cur_quantity = self
            .get_current_positions()
            .get(&symbol.clone())
            .unwrap()
            .position;
        let cash = self.get_latest_holdings().unwrap().cash;
        let current_datetime = data_handler.get_latest_bar_datetime(symbol).unwrap();
        let order_type = &signal_event.order_type;
        let limit_price = signal_event.limit_price;

        // A signal without a quantity cannot become an order. `get_pos_sizer_from_settings`
        // reports an unsupported sizer, a missing commission plan or an invalid parameter as
        // `None`; that is a normal error here (never a panic, the strategy runs across FFI).
        let Some(signal_quantity) = signal_event.quantity else {
            anyhow::bail!(
                "Signal '{}' for symbol '{}' carries no quantity: position sizer '{}' could not \
                 size the position (check pos_sizer_params and the commission plans)",
                signal_name,
                symbol,
                self.strategy_settings.pos_sizer_params.pos_sizer_name,
            );
        };

        let mut quantity = signal_quantity.abs();
        let mut direction = None;

        if signal_name != "EXIT" && cur_quantity == 0.0 && cash > 0.0 {
            quantity = farukon_core::utils::calculate_max_available_quantity(
                cash,
                quantity,
                self.strategy_instruments_info.get(symbol).unwrap(),
            );

            if signal_name == "LONG" {
                direction = Some("BUY".to_string());
            } else if signal_name == "SHORT" {
                direction = Some("SELL".to_string());
            }
        } else if cur_quantity > 0.0 {
            direction = Some("SELL".to_string());
        } else if cur_quantity < 0.0 {
            direction = Some("BUY".to_string());
        }

        // Zero contracts (unaffordable request, or a sizer that rounded down to nothing) must not
        // be padded up to one minimum contract: skipping keeps the signal without inventing a
        // trade and its commission.
        if quantity <= 0.0 {
            if self.mode == "Debug" {
                println!(
                    "Signal '{}' for symbol '{}' produces zero quantity — no order generated",
                    signal_name, symbol
                );
            }
            return anyhow::Ok(None);
        }

        if risks::margin_call_control_for_signal(
            quantity,
            self.get_latest_holdings().unwrap(),
            signal_event,
            self.strategy_instruments_info.get(symbol).unwrap(),
        )? {
            let order = Some(farukon_core::event::OrderEvent::new(
                current_datetime,
                symbol.to_string(),
                order_type.to_string(),
                quantity,
                direction,
                signal_name.to_string(),
                limit_price,
            ));

            return anyhow::Ok(order);
        }

        anyhow::Ok(None)
    }

    /// Appends/replaces the position snapshot with `datetime`.
    ///
    /// The snapshots are capped at 2 entries; only `datetime` is consumed downstream (the
    /// per-symbol position map is not read from snapshots), so the map is left empty to avoid an
    /// O(symbols) HashMap clone on every MARKET event.
    fn push_position_snapshot(&mut self, datetime: chrono::DateTime<chrono::Utc>) {
        if self.all_positions.len() < 2 {
            self.all_positions
                .push(farukon_core::portfolio::PositionSnapshot::new(
                    datetime,
                    std::collections::HashMap::new(),
                ));
        } else if let Some(last) = self.all_positions.last_mut() {
            *last = farukon_core::portfolio::PositionSnapshot::new(
                datetime,
                std::collections::HashMap::new(),
            );
        }
    }

    /// Appends/replaces the holdings snapshot with `datetime`, adding `pnl` to the running capital.
    ///
    /// Only the scalar fields (datetime, capital, cash, blocked) are consumed downstream, so the
    /// per-symbol holdings map is left empty to avoid an O(symbols) HashMap clone on every MARKET
    /// event. [`Portfolio::update_timeindex`] passes this bar's summed PnL;
    /// [`Portfolio::update_final_timeindex`] passes only the PnL of fills that were not booked yet.
    fn push_holdings_snapshot(&mut self, datetime: chrono::DateTime<chrono::Utc>, pnl: f64) {
        if self.all_holdings.is_empty() {
            self.all_holdings
                .push(farukon_core::portfolio::HoldingSnapshot::new(
                    datetime,
                    self.initial_capital_for_strategy,
                    self.initial_capital_for_strategy,
                    0.0,
                    std::collections::HashMap::new(),
                ));
            return;
        }

        let blocked: f64 = self.current_holdings.values().map(|a| a.blocked).sum();

        if self.all_holdings.len() == 1 {
            if let Some(first) = self.all_holdings.first_mut() {
                let capital = first.capital + pnl;
                let cash = capital - blocked;

                self.all_holdings
                    .push(farukon_core::portfolio::HoldingSnapshot::new(
                        datetime,
                        capital,
                        cash,
                        blocked,
                        std::collections::HashMap::new(),
                    ));
            }
        } else if let Some(last) = self.all_holdings.last_mut() {
            let capital = last.capital + pnl;
            let cash = capital - blocked;

            *last = farukon_core::portfolio::HoldingSnapshot::new(
                datetime,
                capital,
                cash,
                blocked,
                std::collections::HashMap::new(),
            );
        }
    }
}

impl farukon_core::portfolio::PortfolioHandler for Portfolio {
    /// Updates position state based on a fill event.
    /// # Arguments
    /// * `fill_event` - The fill event received from the execution engine.
    /// * `data_handler` - The data handler for accessing market data.
    fn update_positions_from_fill(
        &mut self,
        fill_event: &farukon_core::event::FillEvent,
        data_handler: &dyn farukon_core::data_handler::DataHandler,
    ) {
        // Updates position state on FILL.
        // Adjusts position size, records entry/exit prices.

        let timeindex = data_handler.get_latest_bar_datetime(&fill_event.symbol);
        if self.mode == "Debug" {
            println!(
                "for Fill event, start Current_positions, {:?}, {:?}",
                timeindex, self.current_positions
            );
        }

        let fill_dir = match fill_event.direction.as_deref() {
            Some("BUY") => 1.0,
            Some("SELL") => -1.0,
            _ => {
                eprintln!(
                    "Unknown fill direction: {}",
                    fill_event.direction.clone().unwrap()
                );
                0.0
            }
        };
        let symbol = &fill_event.symbol;
        let quantity = fill_event.quantity;
        let signal_name = fill_event.signal_name.as_str();
        let current_cash = self.get_latest_holdings().unwrap().cash;

        if let Some(position_state) = self.current_positions.get_mut(symbol) {
            position_state.position += fill_dir * quantity;

            match signal_name {
                "EXIT" => {
                    position_state.entry_price = None;
                    position_state.entry_capital = 0.0;
                }
                _ => {
                    position_state.deal_number += 1;
                    position_state.entry_price = fill_event.execution_price;
                    position_state.entry_capital = current_cash;
                }
            }
        }

        if self.mode == "Debug" {
            println!(
                "for Fill event, finish Current_positions, {:?}, {:?}",
                timeindex, self.current_positions
            );
        }
    }

    /// Updates holding state based on a fill event.
    /// # Arguments
    /// * `fill_event` - The fill event received from the execution engine.
    /// * `data_handler` - The data handler for accessing market data.
    fn update_holdings_from_fill(
        &mut self,
        fill_event: &farukon_core::event::FillEvent,
        data_handler: &dyn farukon_core::data_handler::DataHandler,
    ) {
        // Updates PnL and blocked margin on FILL.
        // Uses step_price and step to convert price to currency value.

        let timeindex = data_handler.get_latest_bar_datetime(&fill_event.symbol);
        if self.mode == "Debug" {
            println!(
                "for Fill event, start Current_holdings, {:?}, {:?}",
                timeindex, self.current_holdings
            );
        }

        let quantity = fill_event.quantity;
        let symbol = &fill_event.symbol;
        // INVARIANT: `FillEvent`s are only built in `SimulatedExecutionHandler::execute_order`,
        // which resolves the commission before sending the event and returns an error (instead of
        // sending a fill) when no commission plan matched. So a fill always carries `Some`.
        let commission = fill_event.commission.expect(
            "FillEvent without a commission — the execution handler must resolve it (see \
             SimulatedExecutionHandler::execute_order)",
        );
        let signal_name = fill_event.signal_name.as_str();
        let direction = fill_event.direction.clone().unwrap();
        let execution_price = fill_event.execution_price.unwrap_or(0.0);
        let close = match data_handler.get_latest_bar_value(symbol, "close") {
            Some(value) if value.is_nan() => 0.0,
            Some(value) => value,
            None => 0.0,
        };

        let last_close = match data_handler
            .get_latest_bars_values(symbol, "close", 2)
            .unwrap()
            .first()
        {
            Some(value) if value.is_nan() => 0.0,
            Some(value) => *value,
            None => 0.0,
        };

        let strategy_instrument_info_for_symbol =
            self.strategy_instruments_info.get(symbol).unwrap();
        let step_price = strategy_instrument_info_for_symbol.step_price;
        let step = strategy_instrument_info_for_symbol.step;
        let cost_of_step_price = ((step_price / step) * 100_000.0).round() / 100_000.0;

        self.current_holdings.get_mut(symbol).unwrap().signal_name =
            Some(fill_event.signal_name.clone());

        // An exit realizes the position at `execution_price`. During the main loop the last booked
        // point is the previous bar's close (`last_close`). In the final settle phase the last
        // regular `update_timeindex` has already booked this bar's market move for the position, so
        // only the execution-vs-marked-close component is still pending: booking the full
        // `execution_price - last_close` would count that market move twice.
        let exit_reference = if self.final_settle {
            self.last_marked_close
                .get(symbol)
                .copied()
                .unwrap_or(last_close)
        } else {
            last_close
        };

        let pnl = match signal_name {
            "EXIT" => {
                let pnl = match direction.as_str() {
                    "BUY" => {
                        (((exit_reference - execution_price) * cost_of_step_price)
                            * quantity
                            * 100.0)
                            .round()
                            / 100.0
                            - commission
                    }
                    "SELL" => {
                        (((execution_price - exit_reference) * cost_of_step_price)
                            * quantity
                            * 100.0)
                            .round()
                            / 100.0
                            - commission
                    }
                    _ => {
                        eprintln!("Unknown fill direction: {:?}", fill_event.direction);
                        return;
                    }
                };

                match self
                    .strategy_instruments_info
                    .get(symbol)
                    .unwrap()
                    .instrument_type
                    .as_str()
                {
                    "futures" => {
                        self.current_holdings.get_mut(symbol).unwrap().blocked -=
                            strategy_instrument_info_for_symbol.margin * quantity;
                    }
                    _ => {
                        eprintln!("Unknown type of instrument!");
                    }
                }

                pnl
            }
            _ => {
                let pnl = match direction.as_str() {
                    "BUY" => {
                        (((close - execution_price) * cost_of_step_price) * quantity * 100.0)
                            .round()
                            / 100.0
                            - commission
                    }
                    "SELL" => {
                        (((execution_price - close) * cost_of_step_price) * quantity * 100.0)
                            .round()
                            / 100.0
                            - commission
                    }
                    _ => {
                        eprintln!("Unknown fill direction: {:?}", fill_event.direction);
                        return;
                    }
                };

                match self
                    .strategy_instruments_info
                    .get(symbol)
                    .unwrap()
                    .instrument_type
                    .as_str()
                {
                    "futures" => {
                        self.current_holdings.get_mut(symbol).unwrap().blocked +=
                            strategy_instrument_info_for_symbol.margin * quantity;
                    }
                    _ => {
                        eprintln!("Unknown type of instrument!");
                    }
                }

                // An entry marks the new position at this bar's close (its PnL is measured against
                // `close`), so a fill settled later must be measured from there, not from the
                // previous bar's close.
                self.last_marked_close.insert(symbol.clone(), close);

                pnl
            }
        };

        self.current_holdings.get_mut(symbol).unwrap().pnl = pnl;

        if self.final_settle {
            // Several fills for one symbol can arrive between the last regular snapshot and the
            // final one (e.g. two partial EXITs). `holdings.pnl` holds only the latest of them, so
            // they are accumulated here: every final fill's execution delta and commission must be
            // booked exactly once. Each exit is measured against the same booked-through close (an
            // EXIT does not advance `last_marked_close`), so the deltas simply add up.
            *self.pending_fill_pnl.entry(symbol.clone()).or_insert(0.0) += pnl;
        }

        if self.mode == "Debug" {
            println!(
                "for Fill event, finish Current_holdings, {:?}, {:?}",
                timeindex, self.current_holdings
            );
        }
    }

    /// Updates the portfolio state based on a fill event.
    /// # Arguments
    /// * `fill_event` - The fill event received from the execution engine.
    /// * `data_handler` - The data handler for accessing market data.
    fn update_fill(
        &mut self,
        fill_event: &farukon_core::event::FillEvent,
        data_handler: &dyn farukon_core::data_handler::DataHandler,
    ) {
        self.update_positions_from_fill(fill_event, data_handler);
        self.update_holdings_from_fill(fill_event, data_handler);
    }

    /// Updates the portfolio state based on a fill event.
    /// # Arguments
    /// * `fill_event` - The fill event received from the execution engine.
    /// * `data_handler` - The data handler for accessing market data.
    fn update_timeindex(&mut self, data_handler: &dyn farukon_core::data_handler::DataHandler) {
        // Called on every MARKET event to update equity curve, snapshots, and metrics.
        // Also triggers margin call monitoring.

        let current_bar_datetime = data_handler
            .get_latest_bar_datetime(&self.strategy_settings.symbols[0])
            .unwrap();

        if self.mode == "Debug" {
            println!(
                "Start event, Update timeindex, {}, {:?}, {:?}",
                current_bar_datetime, self.current_positions, self.current_holdings,
            );
        }

        self.push_position_snapshot(current_bar_datetime);

        // Update unrealized PnL for open positions
        {
            for symbol in &self.strategy_settings.symbols {
                let close = match data_handler.get_latest_bar_value(symbol, "close") {
                    Some(value) if value.is_nan() => 0.0,
                    Some(value) => value,
                    None => 0.0,
                };

                let last_close = match data_handler
                    .get_latest_bars_values(symbol, "close", 2)
                    .unwrap()
                    .first()
                {
                    Some(value) if value.is_nan() => 0.0,
                    Some(value) => *value,
                    None => 0.0,
                };

                let strategy_instrument_info_for_symbol =
                    self.strategy_instruments_info.get(symbol).unwrap();
                let step_price = strategy_instrument_info_for_symbol.step_price;
                let step = strategy_instrument_info_for_symbol.step;
                let cost_of_step_price = ((step_price / step) * 100_000.0).round() / 100_000.0;

                if let Some(holdings_state) = self.current_holdings.get_mut(symbol)
                    && let Some(position_state) = self.current_positions.get(symbol)
                {
                    if holdings_state.signal_name.is_some() {
                        // The PnL of this symbol was set by a fill that arrived after the previous
                        // snapshot; it is booked as is, so the mark stays where the fill left it.
                        holdings_state.signal_name = None;
                    } else {
                        holdings_state.pnl = (((close - last_close) * cost_of_step_price)
                            * position_state.position
                            * 100.0)
                            .round()
                            / 100.0;
                        // The position is now marked through this bar's close.
                        self.last_marked_close.insert(symbol.clone(), close);
                    }
                }
            }
        }

        // Book this bar's PnL (fill results and market moves) into the holdings snapshot.
        let pnl: f64 = self.current_holdings.iter().map(|a| a.1.pnl).sum();
        self.push_holdings_snapshot(current_bar_datetime, pnl);

        // Udate equity curve data
        {
            if let Some(latest_holdings) = self.get_latest_holdings() {
                self.equity_series
                    .push((current_bar_datetime, latest_holdings.capital));
            }
        }

        // Update metrics incrementally if in RealTime mode
        {
            if let farukon_core::settings::MetricsMode::RealTime { .. } = self
                .strategy_settings
                .portfolio_settings_for_strategy
                .metrics_calculation_mode
            {
                let start_date = self.get_all_holdings().first().unwrap().datetime;
                let end_date = data_handler
                    .get_latest_bar_datetime(self.strategy_settings.symbols.first().unwrap())
                    .unwrap();

                // Update deals counter
                let mut deals_count = 0_usize;
                for symbol in &self.strategy_settings.symbols {
                    if let Some(position_state) = self.current_positions.get_mut(symbol) {
                        deals_count += position_state.deal_number;
                    }
                }

                if let Some(latest_holdings) = self.get_latest_holdings() {
                    self.performance_manager.update_incremental(
                        latest_holdings.capital,
                        start_date,
                        end_date,
                        deals_count,
                    );

                    if self.mode == "Debug" {
                        println!(
                            "Metrics, {:?}",
                            self.performance_manager.get_current_performance_metrics()
                        );
                    }
                }
            }
        }

        // Margin call monitoring: if capital falls below min_margin, close all positions
        {
            let margin_call_monitoring = risks::margin_call_control_for_market(
                self.get_latest_holdings().unwrap(),
                self.get_current_positions(),
                &self.strategy_settings,
                &self.strategy_instruments_info,
            )
            .unwrap();
            if !margin_call_monitoring {
                for symbol in &self.strategy_settings.symbols {
                    let quantity = Some(self.get_current_positions().get(symbol).unwrap().position);
                    if quantity != Some(0.0) {
                        let _ = self.event_sender.send(Box::new(
                            farukon_core::event::SignalEvent::new(
                                current_bar_datetime,
                                symbol.clone(),
                                "EXIT".to_string(),
                                "MKT".to_string(),
                                quantity,
                                None,
                            ),
                        ));
                    }
                }
            }
        }

        if self.mode == "Debug" {
            println!(
                "Finish event, Update timeindex, {}, {:?}, {:?}",
                current_bar_datetime, self.current_positions, self.current_holdings,
            );
        }
    }

    /// Records a final snapshot after the last market data update, booking the PnL of every fill
    /// that arrived after the last `update_timeindex`.
    ///
    /// The pending amount is accumulated per symbol while the fills are processed
    /// ([`Portfolio::pending_fill_pnl`]), so several final fills for one symbol (e.g. two partial
    /// `EXIT`s) each contribute their execution delta and commission exactly once.
    ///
    /// Calling `update_timeindex` again would re-add the last bar's market move for every symbol
    /// that has no pending fill (its `pnl` still holds that bar's delta), so this variant keeps
    /// those symbols out of the capital delta. Market data is not advanced and no risk check is
    /// run: nothing can trade any more.
    fn update_final_timeindex(
        &mut self,
        data_handler: &dyn farukon_core::data_handler::DataHandler,
    ) {
        let current_bar_datetime = data_handler
            .get_latest_bar_datetime(&self.strategy_settings.symbols[0])
            .unwrap();

        // Every fill that arrived since the last snapshot, accumulated per symbol: `holdings.pnl`
        // alone keeps only the latest fill of a symbol, so two partial exits would lose one.
        let pending_pnl: f64 = self.pending_fill_pnl.values().sum();
        self.pending_fill_pnl.clear();
        for holdings in self.current_holdings.values_mut() {
            holdings.signal_name = None;
        }

        self.push_position_snapshot(current_bar_datetime);
        self.push_holdings_snapshot(current_bar_datetime, pending_pnl);

        if let Some(latest_holdings) = self.get_latest_holdings() {
            self.equity_series
                .push((current_bar_datetime, latest_holdings.capital));
        }
    }

    /// Switches the portfolio into the final settle phase.
    ///
    /// From here on an `EXIT` fill books only the not-yet-booked component of its PnL, measured
    /// against [`Portfolio::last_marked_close`] instead of the previous bar's close, because the
    /// last regular `update_timeindex` has already booked the final bar's market move for the open
    /// position (see `update_holdings_from_fill`).
    fn begin_final_settle(&mut self) {
        self.final_settle = true;
    }

    /// Updates the portfolio state based on a signal event.
    /// # Arguments
    /// * `signal_event` - The signal event received from the strategy.
    /// * `data_handler` - The data handler for accessing market data.
    fn update_signal(
        &mut self,
        signal_event: &farukon_core::event::SignalEvent,
        data_handler: &dyn farukon_core::data_handler::DataHandler,
    ) -> anyhow::Result<()> {
        // Converts SIGNAL to ORDER and sends to execution engine.
        if self.mode == "Debug" {
            println!(
                "for Signal event, Current_positions, {}, {:?}",
                data_handler
                    .get_latest_bar_datetime(&signal_event.symbol)
                    .unwrap(),
                self.current_positions
            );
            println!(
                "for Signal event, Current_holdings, {}, {:?}",
                data_handler
                    .get_latest_bar_datetime(&signal_event.symbol)
                    .unwrap(),
                self.current_holdings
            );
            println!(
                "for Signal event, latest_holdings, {}, {:?}",
                data_handler
                    .get_latest_bar_datetime(&signal_event.symbol)
                    .unwrap(),
                self.get_latest_holdings()
            );
        }

        if let Some(order) = self.generate_order(signal_event, data_handler)? {
            match self.event_sender.send(Box::new(order)) {
                Ok(()) => {}
                Err(e) => eprintln!("Failed to send OrderEvent: {}", e),
            }
        }

        anyhow::Ok(())
    }

    /// Returns a summary of the final performance metrics.
    /// # Returns
    /// * `anyhow::Result<&PerformanceMetrics>` containing the final metrics.
    fn output_summary_stats(
        &self,
    ) -> anyhow::Result<&farukon_core::performance::PerformanceMetrics> {
        // Returns final performance metrics after backtest.
        if self.mode == "Debug" {
            println!("{:#?}", self.equity_series);
        }

        let output_metrics = self.performance_manager.get_current_performance_metrics();

        let drawdowns = self.performance_manager.get_drawdons();
        let drawdowns_pct = self.performance_manager.get_drawdowns_pct();

        if self.mode == "Visual" || self.mode == "Debug" || self.mode == "Portfolio" {
            let filename = if self.mode == "Portfolio" {
                format!("equity_curve_{}.csv", self.strategy_settings.strategy_name)
            } else {
                "equity_series.csv".to_string()
            };
            farukon_core::utils::export_equity_drawdowns_to_csv(
                drawdowns,
                drawdowns_pct,
                &self.equity_series,
                &self.strategy_settings,
                &filename,
            )?;
        }

        anyhow::Ok(output_metrics)
    }

    /// Calculates final performance metrics after the backtest ends.
    ///
    /// The equity timestamps are passed to `PerformanceManager::calculate_final`, so the final
    /// metrics include the `Max_Drawdown_DateTime` of the drawdown trough.
    ///
    /// This runs in every metrics mode, not only in `Offline`: the full equity curve is the only
    /// source of the drawdown timestamp (`update_incremental` of the `RealTime` path has no
    /// timestamps and therefore always reports `None`). `RealTime` keeps its per-bar incremental
    /// updates during a run and gets the same accurate, final metrics here at the end.
    fn calculate_final_performance(&mut self) {
        // Called after backtest ends to compute final metrics.
        // Uses full equity curve for accurate drawdown and return calculations.

        if let Some(holdings) = self.get_all_holdings().first() {
            // Materialize the equity curve once: both buffers are built in a single pass
            // over `self.equity_series` (this path runs once per candidate).
            let mut equity_series: Vec<f64> = Vec::with_capacity(self.equity_series.len());
            let mut equity_datetimes: Vec<chrono::DateTime<chrono::Utc>> =
                Vec::with_capacity(self.equity_series.len());
            for (datetime, capital) in self.equity_series.iter() {
                equity_datetimes.push(*datetime);
                equity_series.push(*capital);
            }

            let start_date = holdings.datetime;
            let end_date = self.get_latest_holdings().unwrap().datetime;

            // Deals are counted from the live position state (same source as the
            // incremental RealTime metrics); snapshots no longer carry position maps.
            let deals_count: usize = self
                .strategy_settings
                .symbols
                .iter()
                .filter_map(|symbol| self.current_positions.get(symbol))
                .map(|position_state| position_state.deal_number)
                .sum();

            self.performance_manager.calculate_final(
                &equity_series,
                &equity_datetimes,
                start_date,
                end_date,
                deals_count,
            );
        }
    }

    // Getters
    /// Returns a reference to the current positions.
    fn get_current_positions(
        &self,
    ) -> &std::collections::HashMap<String, farukon_core::portfolio::PositionState> {
        &self.current_positions
    }

    /// Returns a reference to all historical position snapshots.
    fn get_all_positions(&self) -> &Vec<farukon_core::portfolio::PositionSnapshot> {
        &self.all_positions
    }

    /// Returns a reference to the current holdings.
    fn get_current_holdings(
        &self,
    ) -> &std::collections::HashMap<String, farukon_core::portfolio::HoldingsState> {
        &self.current_holdings
    }

    /// Returns a reference to all historical holding snapshots.
    fn get_all_holdings(&self) -> &Vec<farukon_core::portfolio::HoldingSnapshot> {
        &self.all_holdings
    }

    /// Returns a reference to the latest equity snapshot.
    fn get_latest_holdings(&self) -> Option<&farukon_core::portfolio::HoldingSnapshot> {
        self.all_holdings.last()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use farukon_core::execution::ExecutionHandler;
    use farukon_core::settings;

    /// Minimal strategy settings; `metrics_calculation_mode` decides which metrics path is used.
    fn test_strategy_settings(
        metrics_calculation_mode: settings::MetricsMode,
    ) -> settings::StrategySettings {
        settings::StrategySettings {
            threads: None,
            strategy_name: "test".to_string(),
            strategy_path: String::new(),
            exit_results_path: String::new(),
            strategy_weight: 1.0,
            slippage: settings::ParamSpec::Discrete(vec![]),
            data: settings::DataSettings {
                data_path: String::new(),
                timeframe: String::new(),
            },
            symbols: vec![],
            strategy_params: std::collections::HashMap::new(),
            pos_sizer_params: settings::PosSizer {
                pos_sizer_name: "1".to_string(),
                pos_sizer_params: std::collections::HashMap::new(),
                pos_sizer_value: settings::ParamSpec::Discrete(vec![]),
            },
            margin_params: settings::MarginParams {
                min_margin: 0.0,
                margin_call_type: String::new(),
            },
            portfolio_settings_for_strategy: settings::PortfolioSettingsForStrategy {
                metrics_calculation_mode,
            },
            optimizer_type: settings::OptimizerType::GridSearch,
            commission_plans: None,
        }
    }

    /// Seeds the state that `update_timeindex` would have collected during a backtest:
    /// two holding snapshots (first/latest) and an equity curve with a drawdown trough at index 2.
    fn portfolio_with_equity_series(mode: settings::MetricsMode) -> Portfolio {
        let (event_sender, _event_receiver) = std::sync::mpsc::channel();
        let strategy_instruments_info: std::collections::HashMap<
            String,
            farukon_core::instruments_info::InstrumentInfo,
        > = std::collections::HashMap::new();

        let mut portfolio = Portfolio::new(
            &"Optimize".to_string(),
            &100.0,
            event_sender,
            &test_strategy_settings(mode),
            &strategy_instruments_info,
        )
        .unwrap();

        let base = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let datetimes: Vec<chrono::DateTime<chrono::Utc>> = (0..4)
            .map(|i| base + chrono::Duration::minutes(i as i64))
            .collect();
        let caps = [100.0, 80.0, 60.0, 90.0];

        portfolio.equity_series = datetimes.iter().copied().zip(caps).collect();
        portfolio.all_holdings = vec![
            farukon_core::portfolio::HoldingSnapshot::new(
                datetimes[0],
                caps[0],
                caps[0],
                0.0,
                std::collections::HashMap::new(),
            ),
            farukon_core::portfolio::HoldingSnapshot::new(
                datetimes[3],
                caps[3],
                caps[3],
                0.0,
                std::collections::HashMap::new(),
            ),
        ];

        portfolio
    }

    /// The optimizers write `optimization_results.csv` from these metrics, so every metrics mode
    /// must report the drawdown timestamp (Grid Search / GA rows were empty in `RealTime` mode).
    #[test]
    fn final_performance_reports_max_drawdown_datetime_in_every_metrics_mode() {
        for mode in [
            settings::MetricsMode::Offline,
            settings::MetricsMode::RealTime {
                modified_kelly_creterion: settings::KellyMode::Off,
            },
        ] {
            let mut portfolio = portfolio_with_equity_series(mode);
            let drawdown_datetime = portfolio.equity_series[2].0;

            portfolio.calculate_final_performance();

            assert_eq!(
                portfolio
                    .performance_manager
                    .get_current_performance_metrics()
                    .get_max_drawdown_datetime(),
                &Some(drawdown_datetime)
            );
        }
    }

    /// Market data stub: one symbol, a fixed set of bars, `current_index` as in the real handler.
    /// Shared with the `backtest` tests (they drive the same event pipeline).
    pub(crate) struct StubDataHandler {
        symbol: String,
        soa_data: farukon_core::data_handler::SOAData,
        current_index: usize,
        continue_backtest: bool,
    }

    impl StubDataHandler {
        /// Two bars: high 100 / low 90 / close 95, with a fractional traded volume.
        pub(crate) fn new(symbol: &str) -> Self {
            let mut soa_data = farukon_core::data_handler::SOAData::new();
            for (i, close) in [95.0_f64, 96.0].into_iter().enumerate() {
                let bar = farukon_core::data_handler::MarketBar::new()
                    .with_datetime(
                        chrono::DateTime::from_timestamp(1_700_000_000 + 60 * i as i64, 0).unwrap(),
                    )
                    .with_open(close)
                    .with_high(100.0)
                    .with_low(90.0)
                    .with_close(close)
                    .with_volume(0.5);
                soa_data.add_bar(&bar);
            }

            Self {
                symbol: symbol.to_string(),
                current_index: 2,
                soa_data,
                continue_backtest: true,
            }
        }

        fn start_index(&self, n: usize) -> usize {
            if self.current_index as i64 - n as i64 <= 0 {
                0
            } else {
                self.current_index - n
            }
        }

        /// Points the stub at the `index`-th bar (1-based, like the real handler's
        /// `current_index`), as if the backtest loop had advanced that far.
        pub(crate) fn set_current_index(&mut self, index: usize) {
            self.current_index = index;
            self.continue_backtest = index < self.soa_data.len();
        }
    }

    impl farukon_core::data_handler::DataHandler for StubDataHandler {
        fn get_latest_bar(&self, symbol: &str) -> Option<farukon_core::data_handler::SOAData> {
            (symbol == self.symbol).then(|| self.soa_data.get_latest_bar(self.current_index))
        }

        fn get_latest_bars(
            &self,
            symbol: &str,
            n: usize,
        ) -> Option<farukon_core::data_handler::SOAData> {
            (symbol == self.symbol).then(|| {
                self.soa_data
                    .set_latest_bars(self.start_index(n), self.current_index)
            })
        }

        fn get_latest_bar_datetime(&self, symbol: &str) -> Option<chrono::DateTime<chrono::Utc>> {
            (symbol == self.symbol)
                .then(|| *self.soa_data.get_timestamp(self.current_index - 1).unwrap())
        }

        fn get_latest_bar_value(&self, symbol: &str, val_type: &str) -> Option<f64> {
            if symbol != self.symbol {
                return None;
            }
            let idx = self.current_index - 1;
            match val_type {
                "open" => Some(*self.soa_data.get_open(idx).unwrap()),
                "high" => Some(*self.soa_data.get_high(idx).unwrap()),
                "low" => Some(*self.soa_data.get_low(idx).unwrap()),
                "close" => Some(*self.soa_data.get_close(idx).unwrap()),
                "volume" => Some(*self.soa_data.get_volume(idx).unwrap()),
                _ => None,
            }
        }

        fn get_latest_bars_values(&self, symbol: &str, val_type: &str, n: usize) -> Option<&[f64]> {
            if symbol != self.symbol {
                return None;
            }
            let start = self.start_index(n);
            match val_type {
                "open" => Some(&self.soa_data.get_opens().unwrap()[start..self.current_index]),
                "high" => Some(&self.soa_data.get_highs().unwrap()[start..self.current_index]),
                "low" => Some(&self.soa_data.get_lows().unwrap()[start..self.current_index]),
                "close" => Some(&self.soa_data.get_closes().unwrap()[start..self.current_index]),
                "volume" => Some(&self.soa_data.get_volumes().unwrap()[start..self.current_index]),
                _ => None,
            }
        }

        fn get_latest_bars_volume_values(&self, symbol: &str, n: usize) -> Option<&[f64]> {
            if symbol != self.symbol {
                return None;
            }
            let start = self.start_index(n);
            Some(&self.soa_data.get_volumes().unwrap()[start..self.current_index])
        }

        fn update_bars(&mut self) {
            if self.current_index < self.soa_data.len() {
                self.current_index += 1;
            } else {
                self.continue_backtest = false;
            }
        }

        fn get_continue_backtest(&self) -> bool {
            self.continue_backtest
        }

        fn set_continue_backtest(&mut self, value: bool) {
            self.continue_backtest = value;
        }
    }

    /// Settings/instruments for a fractional-quantity instrument with a fractional volume feed.
    /// Shared with the `backtest` tests.
    pub(crate) fn fractional_test_setup() -> (
        farukon_core::settings::StrategySettings,
        std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    ) {
        let symbol = "Si-12.23".to_string();
        let mut strategy_settings = test_strategy_settings(settings::MetricsMode::Offline);
        strategy_settings.symbols = vec![symbol.clone()];
        strategy_settings.slippage =
            settings::ParamSpec::Discrete(vec![serde_json::Value::from(0.001)]);
        strategy_settings.commission_plans =
            Some(farukon_core::commission_plans::CommissionPlans {
                exchanges: std::collections::HashMap::from([(
                    "FORTS".to_string(),
                    std::collections::HashMap::from([(
                        "default".to_string(),
                        serde_json::json!({ "currency": 0.5 }),
                    )]),
                )]),
            });

        let instrument = farukon_core::instruments_info::InstrumentInfo {
            exchange: "FORTS".to_string(),
            instrument_type: "futures".to_string(),
            contract_precision: 2,
            margin: 100.0,
            commission_type: "currency".to_string(),
            trade_from_date: "2025-01-01 00:00:00".to_string(),
            expiration_date: "2025-12-31 00:00:00".to_string(),
            marginal_costs: 0.0,
            step: 1.0,
            step_price: 1.0,
        };

        let instruments = std::collections::HashMap::from([(symbol, instrument)]);
        (strategy_settings, instruments)
    }

    fn portfolio_with_seeded_holdings(
        strategy_settings: &farukon_core::settings::StrategySettings,
        instruments: &std::collections::HashMap<
            String,
            farukon_core::instruments_info::InstrumentInfo,
        >,
        event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    ) -> Portfolio {
        let mut portfolio = Portfolio::new(
            &"Optimize".to_string(),
            &10_000.0,
            event_sender,
            strategy_settings,
            instruments,
        )
        .unwrap();
        // The first `update_timeindex` creates this snapshot before any signal arrives.
        portfolio.all_holdings = vec![farukon_core::portfolio::HoldingSnapshot::new(
            chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            10_000.0,
            10_000.0,
            0.0,
            std::collections::HashMap::new(),
        )];
        portfolio
    }

    /// Quantities are `f64` end to end: a fractional signal quantity must reach the order, the
    /// fill and the portfolio position unchanged, with the market fill carrying adverse slippage.
    #[test]
    fn fractional_quantity_flows_from_signal_to_order_fill_and_position() {
        let (strategy_settings, instruments) = fractional_test_setup();
        let (event_sender, event_receiver) = std::sync::mpsc::channel();
        let mut portfolio =
            portfolio_with_seeded_holdings(&strategy_settings, &instruments, event_sender.clone());
        let data_handler = StubDataHandler::new("Si-12.23");

        // 1. SIGNAL: fractional quantity requested by the strategy.
        let signal = farukon_core::event::SignalEvent::new(
            chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            "Si-12.23".to_string(),
            "LONG".to_string(),
            "MKT".to_string(),
            Some(0.5),
            None,
        );
        portfolio
            .update_signal(&signal, &data_handler)
            .expect("the signal must become an order");

        // 2. ORDER: the fractional quantity survives order generation.
        let order_box = event_receiver.try_recv().expect("an order must be emitted");
        let order = order_box
            .get_order_event_params()
            .expect("the event must be an ORDER");
        assert_eq!(order.quantity, 0.5);
        assert_eq!(order.direction.as_deref(), Some("BUY"));

        // 3. FILL: market order filled at the bar high plus adverse slippage.
        let execution = crate::execution::SimulatedExecutionHandler::new(event_sender).unwrap();
        execution
            .execute_order(order, &instruments, &strategy_settings, &data_handler)
            .expect("the order must be filled");

        let fill_box = event_receiver.try_recv().expect("a fill must be emitted");
        let fill = fill_box
            .get_fill_event_params()
            .expect("the event must be a FILL");
        assert_eq!(fill.quantity, 0.5);
        let price = fill.execution_price.expect("a fill carries a price");
        assert!(
            (price - 100.1).abs() < 1e-9,
            "BUY must fill at high * (1 + slippage), got {}",
            price
        );
        assert!(fill.commission.expect("a fill carries a commission") > 0.0);

        // 4. PORTFOLIO: the fractional position and its margin are recorded.
        portfolio.update_fill(fill, &data_handler);

        let position = portfolio.get_current_positions()["Si-12.23"].position;
        assert!(
            (position - 0.5).abs() < 1e-12,
            "the portfolio position must keep the fractional size, got {}",
            position
        );
        let blocked = portfolio.get_current_holdings()["Si-12.23"].blocked;
        assert!(
            (blocked - 50.0).abs() < 1e-9,
            "blocked margin must be margin * quantity, got {}",
            blocked
        );
    }

    /// A signal whose sizer returned `None` must surface as a normal error, not as a panic.
    #[test]
    fn signal_without_quantity_is_reported_as_an_error() {
        let (strategy_settings, instruments) = fractional_test_setup();
        let (event_sender, _event_receiver) = std::sync::mpsc::channel();
        let mut portfolio =
            portfolio_with_seeded_holdings(&strategy_settings, &instruments, event_sender);
        let data_handler = StubDataHandler::new("Si-12.23");

        let signal = farukon_core::event::SignalEvent::new(
            chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            "Si-12.23".to_string(),
            "LONG".to_string(),
            "MKT".to_string(),
            None,
            None,
        );

        let error = portfolio
            .update_signal(&signal, &data_handler)
            .expect_err("a signal without a quantity must not become an order");

        assert!(
            error.to_string().contains("carries no quantity"),
            "unexpected error: {}",
            error
        );
    }
}

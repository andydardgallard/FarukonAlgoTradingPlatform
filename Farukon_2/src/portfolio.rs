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
    /// move is already part of the recorded capital. It is the booked-through reference for both
    /// the fill PnL and the next bar's mark-to-market:
    /// * advanced to the bar's close whenever `update_timeindex` books a market move;
    /// * advanced to the bar's close by an entry fill, whose PnL marks the contracts it opened
    ///   there, so this bar's close-to-close move must not be booked for them a second time. A
    ///   fill that only *adds to* an existing position first books the move those contracts
    ///   already earned (see `update_holdings_from_fill`), so advancing the mark cannot erase it;
    /// * left untouched by an `EXIT` fill, which realizes only the exited contracts while the
    ///   remaining position is still marked at the booked-through close — that residual position
    ///   therefore still gets the whole close-to-close move of the bar.
    last_marked_close: std::collections::HashMap<String, f64>,

    /// Net PnL (already minus commission) of the fills that arrived since the last snapshot and
    /// have not been booked yet, accumulated per symbol.
    ///
    /// `HoldingsState::pnl` keeps only the latest fill for a symbol, so several fills between two
    /// snapshots (two entries, two partial `EXIT`s) would otherwise lose all but the last one.
    /// Every fill adds its net result here (see `update_holdings_from_fill`); the snapshot that
    /// books the amount clears it — `update_timeindex` in the main loop, `update_final_timeindex`
    /// in the final settle phase. An add-on entry also adds the market move of the contracts that
    /// were already open, which its own PnL does not cover.
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
    /// event. [`Portfolio::update_timeindex`] passes this bar's market moves plus the PnL of the
    /// fills still unbooked; [`Portfolio::update_final_timeindex`] passes only the PnL of fills
    /// that were not booked yet.
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
                    // A partial exit leaves a nonzero position behind, and that residual still
                    // needs the basis it was opened at: `risks::margin_call_control_for_market`
                    // sums `entry_capital` over the open positions to size the margin requirement,
                    // and an empty `entry_price` loses the entry the remaining contracts were
                    // bought at. The basis is kept unchanged on a partial exit (the residual keeps
                    // the original entry, as if the closed part had never existed) and cleared only
                    // once the position is flat and there is no basis left to keep.
                    if position_state.position == 0.0 {
                        position_state.entry_price = None;
                        position_state.entry_capital = 0.0;
                    }
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

        // The close this symbol's capital is booked through — `last_marked_close`: the previous
        // bar's close in the main loop, and the close of the last regular snapshot in the final
        // settle phase (whose market move is already booked). Both an `EXIT` and an add-on entry
        // need it:
        // * an exit realizes the exited contracts at `execution_price` against it; measuring from
        //   the previous bar's close unconditionally would count this bar's move twice whenever a
        //   fill earlier in the same bar already advanced the mark (an entry does); and because an
        //   `EXIT` never advances the mark, several exits in one bar are all measured from the same
        //   point, so their deltas simply add up;
        // * an entry books the move the contracts it does NOT touch earned since it (see below).
        let marked_close = self
            .last_marked_close
            .get(symbol)
            .copied()
            .unwrap_or(last_close);

        let pnl = match signal_name {
            "EXIT" => {
                let pnl = match direction.as_str() {
                    "BUY" => {
                        (((marked_close - execution_price) * cost_of_step_price) * quantity * 100.0)
                            .round()
                            / 100.0
                            - commission
                    }
                    "SELL" => {
                        (((execution_price - marked_close) * cost_of_step_price) * quantity * 100.0)
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

                // The contracts this fill opened are marked at this bar's close (their PnL is
                // measured against `close`), so a fill settled later must be measured from there,
                // not from the previous bar's close.
                //
                // The contracts that were already open are NOT part of this fill: their move from
                // the booked-through close to this bar's close is not in `pnl`, and advancing the
                // mark for the whole position would erase it from this bar's snapshot (an add-on
                // entry used to swallow the previous contracts' close-to-close PnL). So that move
                // is booked here first — for `close - marked_close` on `old_quantity` — which leaves
                // the bar's total equal to
                // `(close - marked_close) * old_quantity + (close - execution_price) * delta` for an
                // add-on, a reduction and a reversal alike; for an entry on a flat position
                // `old_quantity` is 0 and nothing is added.
                //
                // `update_positions_from_fill` has already applied this fill, hence the subtraction.
                let filled_quantity = match direction.as_str() {
                    "BUY" => quantity,
                    "SELL" => -quantity,
                    // Unreachable: the match above returns early on an unknown direction.
                    _ => 0.0,
                };
                let old_quantity = self
                    .current_positions
                    .get(symbol)
                    .map(|position_state| position_state.position)
                    .unwrap_or(0.0)
                    - filled_quantity;
                let carried_market_move =
                    (((close - marked_close) * cost_of_step_price) * old_quantity * 100.0).round()
                        / 100.0;

                self.last_marked_close.insert(symbol.clone(), close);

                pnl + carried_market_move
            }
        };

        self.current_holdings.get_mut(symbol).unwrap().pnl = pnl;

        // Several fills for one symbol can arrive between two snapshots (two entries, two partial
        // EXITs) — in the main loop as well as in the final settle phase. `holdings.pnl` holds only
        // the latest of them, so every fill's execution delta and commission is accumulated per
        // symbol here and booked exactly once by the next snapshot: `update_timeindex` in the main
        // loop, `update_final_timeindex` in the final settle phase. Both measure their fills from
        // the same booked-through close, so the deltas simply add up.
        *self.pending_fill_pnl.entry(symbol.clone()).or_insert(0.0) += pnl;

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

        // Book this bar's market move together with the fills that arrived since the previous
        // snapshot.
        //
        // A fill keeps its own PnL and no longer suppresses the whole symbol: an `EXIT` books only
        // the exited contracts, while the position it leaves behind is still measured from the
        // close its capital is booked through to this bar's close. `last_marked_close` is exactly
        // that point, so the two components add up without counting one market move twice:
        // * an entry fill advanced the mark to this bar's close, so its market move is zero — the
        //   contracts it opened are marked by its own PnL, and the contracts that were already open
        //   had their move booked with the fill (see `update_holdings_from_fill`);
        // * an `EXIT` left the mark at the previous close, so the residual position gets the whole
        //   close-to-close move of this bar.
        let mut booked_pnl: f64 = 0.0;
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

            let marked_close = if self.final_settle {
                // After `begin_final_settle` the last regular snapshot has booked this bar's market
                // move already (`update_final_timeindex` books the remaining fills without one), so
                // a regular snapshot taken here must not mark the positions to market again.
                close
            } else {
                // The close this symbol's capital is booked through — the previous bar's close, or
                // this bar's close when an entry fill already marked the position there.
                self.last_marked_close
                    .get(symbol)
                    .copied()
                    .unwrap_or(last_close)
            };

            let position = self
                .current_positions
                .get(symbol)
                .map(|position_state| position_state.position)
                .unwrap_or(0.0);
            // A flat position has no market move, so a fully closed symbol books only its fills.
            let market_move =
                (((close - marked_close) * cost_of_step_price) * position * 100.0).round() / 100.0;
            // Every fill that arrived since the previous snapshot, accumulated per symbol.
            let fill_pnl = self.pending_fill_pnl.remove(symbol).unwrap_or(0.0);

            if let Some(holdings_state) = self.current_holdings.get_mut(symbol) {
                // `signal_name` records the last fill; it stops being "unbooked" once its PnL is
                // part of this snapshot.
                holdings_state.signal_name = None;
                holdings_state.pnl = market_move + fill_pnl;
            }
            // The position is now marked through this bar's close.
            self.last_marked_close.insert(symbol.clone(), close);
            booked_pnl += market_move + fill_pnl;
        }
        // Defensive: a fill for a symbol outside the strategy list would still be booked once.
        booked_pnl += self.pending_fill_pnl.values().sum::<f64>();
        self.pending_fill_pnl.clear();

        // Book this bar's PnL (fill results and market moves) into the holdings snapshot.
        self.push_holdings_snapshot(current_bar_datetime, booked_pnl);

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
    /// Calling `update_timeindex` again would re-run the per-bar metrics and the margin-call check
    /// for a bar that cannot trade any more, so this variant keeps the snapshot to the pending
    /// fills alone. Market data is not advanced and no risk check is run: nothing can trade any
    /// more.
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
    /// against [`Portfolio::last_marked_close`] — the close of the last regular snapshot, whose
    /// market move is already booked (see `update_holdings_from_fill`). A regular snapshot must
    /// not mark the positions to market a second time, which is what the flag also tells
    /// `update_timeindex`.
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

    /// Builds a `FILL` for the accounting tests below. `update_holdings_from_fill` reads only the
    /// direction, the execution price and the commission, so no execution handler is involved.
    fn fill(
        symbol: &str,
        quantity: f64,
        direction: &str,
        execution_price: f64,
        commission: f64,
        signal_name: &str,
    ) -> farukon_core::event::FillEvent {
        farukon_core::event::FillEvent::new(
            chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap(),
            symbol.to_string(),
            "FORTS".to_string(),
            quantity,
            Some(direction.to_string()),
            Some(execution_price),
            Some(commission),
            signal_name.to_string(),
        )
    }

    /// A partial exit must keep the basis of the position it leaves behind: the margin-call check
    /// (`risks::margin_call_control_for_market`) sums `entry_capital` over the open positions, and
    /// the remaining contracts were opened at the original entry price. Only a flat position has no
    /// basis left to keep.
    #[test]
    fn partial_exit_keeps_the_entry_basis_of_the_remaining_position() {
        let symbol = "Si-12.23";
        let (strategy_settings, instruments) = fractional_test_setup();
        let (event_sender, _event_receiver) = std::sync::mpsc::channel();
        let mut portfolio =
            portfolio_with_seeded_holdings(&strategy_settings, &instruments, event_sender);
        let mut data_handler = StubDataHandler::new(symbol);
        data_handler.set_current_index(1); // bar 0, close 95.0

        portfolio.update_fill(&fill(symbol, 1.0, "BUY", 95.0, 0.0, "LONG"), &data_handler);
        let entry_capital = portfolio.get_current_positions()[symbol].entry_capital;
        assert_eq!(
            portfolio.get_current_positions()[symbol].entry_price,
            Some(95.0)
        );
        assert!(entry_capital > 0.0, "an entry records its capital basis");

        // Half of the position is closed: the remaining 0.5 keeps the original basis.
        portfolio.update_fill(&fill(symbol, 0.5, "SELL", 96.0, 0.0, "EXIT"), &data_handler);
        let position_state = &portfolio.get_current_positions()[symbol];
        assert!((position_state.position - 0.5).abs() < 1e-12);
        assert_eq!(
            position_state.entry_price,
            Some(95.0),
            "a partial exit must keep the entry price of the residual position"
        );
        assert_eq!(
            position_state.entry_capital, entry_capital,
            "a partial exit must keep the entry capital of the residual position"
        );

        // Closing the last half leaves no position, hence no basis to keep.
        portfolio.update_fill(&fill(symbol, 0.5, "SELL", 96.0, 0.0, "EXIT"), &data_handler);
        let position_state = &portfolio.get_current_positions()[symbol];
        assert_eq!(position_state.position, 0.0);
        assert_eq!(position_state.entry_price, None);
        assert_eq!(position_state.entry_capital, 0.0);
    }

    /// Two fills for one symbol between two snapshots must both reach the capital. A regular
    /// snapshot used to book only the latest fill (`HoldingsState::pnl` is a single scalar), which
    /// dropped the earlier fill's PnL and commission; the regular loop now accumulates per symbol
    /// exactly like the final settle phase.
    #[test]
    fn two_fills_between_snapshots_are_both_booked() {
        let symbol = "Si-12.23";
        let (strategy_settings, instruments) = fractional_test_setup();
        let (event_sender, _event_receiver) = std::sync::mpsc::channel();
        let mut portfolio =
            portfolio_with_seeded_holdings(&strategy_settings, &instruments, event_sender);
        let mut data_handler = StubDataHandler::new(symbol);
        data_handler.set_current_index(1); // bar 0, close 95.0
        portfolio.update_timeindex(&data_handler);
        assert!((portfolio.get_latest_holdings().unwrap().capital - 10_000.0).abs() < 1e-9);

        // Both entries are marked to this bar's close 95.0:
        //   BUY 1.0 at 96.0 -> (95.0 - 96.0) * 1.0 - 1.00 = -2.00
        //   BUY 0.5 at 94.0 -> (95.0 - 94.0) * 0.5 - 0.25 = +0.25
        portfolio.update_fill(&fill(symbol, 1.0, "BUY", 96.0, 1.0, "LONG"), &data_handler);
        portfolio.update_fill(&fill(symbol, 0.5, "BUY", 94.0, 0.25, "LONG"), &data_handler);
        assert!(
            (portfolio.pending_fill_pnl[symbol] - (-1.75)).abs() < 1e-9,
            "both fills must accumulate, got {}",
            portfolio.pending_fill_pnl[symbol]
        );

        // No market move is left to book: an entry marks its position at this bar's close.
        portfolio.update_timeindex(&data_handler);

        assert!((portfolio.get_current_positions()[symbol].position - 1.5).abs() < 1e-12);
        assert!(
            (portfolio.get_latest_holdings().unwrap().capital - (10_000.0 - 1.75)).abs() < 1e-9,
            "both fills must be booked, capital {}",
            portfolio.get_latest_holdings().unwrap().capital
        );
        assert!(
            portfolio.pending_fill_pnl.is_empty(),
            "a booked fill must not be booked twice"
        );
    }

    /// After a partial exit the residual position must still be marked to market for that bar: the
    /// exit books the closed half, the mark-to-market books the remaining half's close-to-close
    /// move. The fill used to suppress the whole symbol, so the residual never got that move.
    #[test]
    fn partial_exit_still_marks_the_residual_position_to_market() {
        let symbol = "Si-12.23";
        let (strategy_settings, instruments) = fractional_test_setup();
        let (event_sender, _event_receiver) = std::sync::mpsc::channel();
        let mut portfolio =
            portfolio_with_seeded_holdings(&strategy_settings, &instruments, event_sender);
        let mut data_handler = StubDataHandler::new(symbol);
        data_handler.set_current_index(1); // bar 0, close 95.0
        portfolio.update_timeindex(&data_handler);

        // Entry: BUY 1.0 at 100.0 -> (95.0 - 100.0) * 1.0 = -5.00 (no commission).
        portfolio.update_fill(&fill(symbol, 1.0, "BUY", 100.0, 0.0, "LONG"), &data_handler);
        portfolio.update_timeindex(&data_handler);
        assert!((portfolio.get_latest_holdings().unwrap().capital - 9_995.0).abs() < 1e-9);

        // Next bar: close 96.0, previous close 95.0.
        data_handler.set_current_index(2);
        // Half of the position is closed at 96.0 -> (96.0 - 95.0) * 0.5 = +0.50.
        portfolio.update_fill(&fill(symbol, 0.5, "SELL", 96.0, 0.0, "EXIT"), &data_handler);
        portfolio.update_timeindex(&data_handler);

        // The bar books the exit (+0.50) and the remaining 0.5 position's close-to-close move
        // ((96.0 - 95.0) * 0.5 = +0.50). Booking only the exit would drop the residual's move.
        assert!(
            (portfolio.get_latest_holdings().unwrap().capital - 9_996.0).abs() < 1e-9,
            "the residual position must be marked to market, capital {}",
            portfolio.get_latest_holdings().unwrap().capital
        );
        assert!((portfolio.get_current_positions()[symbol].position - 0.5).abs() < 1e-12);
    }

    /// An entry and an exit in one bar must book only the realized delta: both fills are measured
    /// from the close the capital is booked through, so the bar's market move is counted once.
    #[test]
    fn entry_and_exit_in_one_bar_book_only_the_realized_delta() {
        let symbol = "Si-12.23";
        let (strategy_settings, instruments) = fractional_test_setup();
        let (event_sender, _event_receiver) = std::sync::mpsc::channel();
        let mut portfolio =
            portfolio_with_seeded_holdings(&strategy_settings, &instruments, event_sender);
        let mut data_handler = StubDataHandler::new(symbol);
        data_handler.set_current_index(1); // bar 0, close 95.0
        portfolio.update_timeindex(&data_handler);

        // BUY 1.0 at 94.0 -> (95.0 - 94.0) * 1.0 = +1.00, and the position is marked at 95.0.
        portfolio.update_fill(&fill(symbol, 1.0, "BUY", 94.0, 0.0, "LONG"), &data_handler);
        // SELL 1.0 (EXIT) at 97.0 -> (97.0 - 95.0) * 1.0 = +2.00, from the same marked close.
        portfolio.update_fill(&fill(symbol, 1.0, "SELL", 97.0, 0.0, "EXIT"), &data_handler);
        portfolio.update_timeindex(&data_handler);

        // Bought at 94.0 and sold at 97.0 with a flat position: +3.00 in total.
        assert_eq!(portfolio.get_current_positions()[symbol].position, 0.0);
        assert!(
            (portfolio.get_latest_holdings().unwrap().capital - 10_003.0).abs() < 1e-9,
            "only the realized delta may be booked, capital {}",
            portfolio.get_latest_holdings().unwrap().capital
        );
    }

    /// Drives two bars of one symbol: an entry on the flat position at bar 0 (close 95.0) and an
    /// add-on entry at bar 1 (close 96.0), each followed by that bar's snapshot. Returns the
    /// portfolio as of the add-on bar's snapshot, together with the data handler still pointing at
    /// bar 1.
    fn portfolio_after_add_on_entry() -> (Portfolio, StubDataHandler) {
        let symbol = "Si-12.23";
        let (strategy_settings, instruments) = fractional_test_setup();
        let (event_sender, _event_receiver) = std::sync::mpsc::channel();
        let mut portfolio =
            portfolio_with_seeded_holdings(&strategy_settings, &instruments, event_sender);
        let mut data_handler = StubDataHandler::new(symbol);

        data_handler.set_current_index(1); // bar 0, close 95.0
        portfolio.update_timeindex(&data_handler);
        // Entry of 1 contract at the close: nothing to book, the position is marked at 95.0.
        portfolio.update_fill(&fill(symbol, 1.0, "BUY", 95.0, 0.0, "LONG"), &data_handler);
        portfolio.update_timeindex(&data_handler);

        data_handler.set_current_index(2); // bar 1, close 96.0
        // Add-on entry: 1 more contract at 95.5 with a 0.25 commission.
        portfolio.update_fill(&fill(symbol, 1.0, "BUY", 95.5, 0.25, "LONG"), &data_handler);
        portfolio.update_timeindex(&data_handler);

        (portfolio, data_handler)
    }

    /// An add-on entry must not erase the contracts that were already open from the bar: advancing
    /// `last_marked_close` to the add-on's close suppressed their previous-close-to-current-close
    /// move, so the aggregated position silently lost it.
    #[test]
    fn add_on_entry_keeps_the_existing_contracts_close_to_close_move() {
        let symbol = "Si-12.23";
        let (portfolio, _data_handler) = portfolio_after_add_on_entry();

        // The bar books
        //   the pre-existing contract: (96.0 - 95.0) * 1.0 = +1.00
        //   the added contract:        (96.0 - 95.5) * 1.0 - 0.25 = +0.25
        assert!(
            (portfolio.get_latest_holdings().unwrap().capital - 10_001.25).abs() < 1e-9,
            "the add-on must keep the existing contract's close-to-close move, capital {}",
            portfolio.get_latest_holdings().unwrap().capital
        );
        assert_eq!(portfolio.get_current_positions()[symbol].position, 2.0);
        assert!(
            (portfolio.get_current_holdings()[symbol].pnl - 1.25).abs() < 1e-9,
            "the bar's booked PnL must be both contracts, got {}",
            portfolio.get_current_holdings()[symbol].pnl
        );
        assert!(
            portfolio.pending_fill_pnl.is_empty(),
            "a booked fill must not be booked twice"
        );
    }

    /// Parity: after an add-on entry, the final settlement must book exactly the per-bar sum — the
    /// old contract's close-to-close move and the new contract's entry-to-exit move, each once.
    #[test]
    fn final_settlement_after_an_add_on_entry_equals_the_per_bar_sum() {
        let symbol = "Si-12.23";
        let (mut portfolio, data_handler) = portfolio_after_add_on_entry();

        portfolio.begin_final_settle();
        // Selling both contracts at 97.0 is measured from the marked close 96.0:
        //   (97.0 - 96.0) * 2.0 = +2.00.
        portfolio.update_fill(&fill(symbol, 2.0, "SELL", 97.0, 0.0, "EXIT"), &data_handler);
        portfolio.update_final_timeindex(&data_handler);

        // Per bar: 0.00 (entry) + 1.25 (add-on bar) + 2.00 (exit) = 3.25.
        assert_eq!(portfolio.get_current_positions()[symbol].position, 0.0);
        assert!(
            (portfolio.get_latest_holdings().unwrap().capital - 10_003.25).abs() < 1e-9,
            "final settlement must equal the per-bar sum, capital {}",
            portfolio.get_latest_holdings().unwrap().capital
        );
        let equity_delta: f64 = portfolio
            .equity_series
            .windows(2)
            .map(|pair| pair[1].1 - pair[0].1)
            .sum();
        assert!(
            (equity_delta - (10_003.25 - 10_000.0)).abs() < 1e-9,
            "the per-bar snapshots must add up to the final capital, got {}",
            equity_delta
        );
        assert!(portfolio.pending_fill_pnl.is_empty());
    }

    /// An add-on entry followed by an exit in the same bar must book each contract exactly once:
    /// the old contract's move up to the marked close, then both contracts' realized delta.
    #[test]
    fn add_on_entry_and_exit_in_one_bar_book_every_contract_once() {
        let symbol = "Si-12.23";
        let (strategy_settings, instruments) = fractional_test_setup();
        let (event_sender, _event_receiver) = std::sync::mpsc::channel();
        let mut portfolio =
            portfolio_with_seeded_holdings(&strategy_settings, &instruments, event_sender);
        let mut data_handler = StubDataHandler::new(symbol);
        data_handler.set_current_index(1); // bar 0, close 95.0
        portfolio.update_timeindex(&data_handler);

        // Entry of 1 contract at 95.0, marked at the bar's close: nothing to book.
        portfolio.update_fill(&fill(symbol, 1.0, "BUY", 95.0, 0.0, "LONG"), &data_handler);
        portfolio.update_timeindex(&data_handler);

        data_handler.set_current_index(2); // bar 1, close 96.0
        // Add-on of 1 contract at 96.0 -> the existing contract earns (96.0 - 95.0) * 1.0 = +1.00.
        portfolio.update_fill(&fill(symbol, 1.0, "BUY", 96.0, 0.0, "LONG"), &data_handler);
        // Both contracts are sold at 97.0 from the marked close 96.0 -> +2.00.
        portfolio.update_fill(&fill(symbol, 2.0, "SELL", 97.0, 0.0, "EXIT"), &data_handler);
        portfolio.update_timeindex(&data_handler);

        // Bought at 95.0 and 96.0, both sold at 97.0: +1.00 + 2.00 = +3.00.
        assert_eq!(portfolio.get_current_positions()[symbol].position, 0.0);
        assert!(
            (portfolio.get_latest_holdings().unwrap().capital - 10_003.0).abs() < 1e-9,
            "each contract must be booked once, capital {}",
            portfolio.get_latest_holdings().unwrap().capital
        );
    }
}

// Farukon_2_0/src/execution.rs

/// Structure responsible for simulating order execution.
/// It mimics broker behavior by applying slippage and commission,
/// and generates `FILL` events which are sent to the event channel.
pub struct SimulatedExecutionHandler {
    /// Channel for sending events (in this case, `FillEvent`).
    pub event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
}

impl SimulatedExecutionHandler {
    /// Creates a new instance of `SimulatedExecutionHandler`.
    /// # Arguments
    /// * `event_sender` - The sender for the event channel, which will be used to send `FILL` events.
    /// # Returns
    /// * `anyhow::Result<Self>` - A new instance of `SimulatedExecutionHandler` or an error.
    pub fn new(
        event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    ) -> anyhow::Result<Self> {
        // Always returns Ok with a new instance, as creating the struct itself cannot fail.
        anyhow::Ok(
            Self {
                event_sender,
            }
        )
    }
}

/// Implementation of the `ExecutionHandler` trait from `farukon_core` for `SimulatedExecutionHandler`.
/// This allows the execution simulator to be used within the main backtesting engine.
impl farukon_core::execution::ExecutionHandler for SimulatedExecutionHandler {
    /// The main method that simulates order execution.
    /// It takes an `ORDER` event, determines the execution price based on the order type (market/limit),
    /// applies slippage and commission, and sends the result as a `FILL` event.
    ///
    /// # Arguments
    /// * `event` - The `OrderEvent` containing order details (symbol, type, direction, quantity, etc.).
    /// * `strategy_instruments_info` - A hash-map containing information about instruments traded by the strategy.
    /// * `strategy_settings` - Strategy settings, including slippage and commission plans.
    /// * `data_handler` - The data handler used to get the latest bar's price and time.
    ///
    /// # Returns
    /// * `anyhow::Result<()>` - `Ok(())` on success, or an `Err` in case of an error (e.g., missing data).
    fn execute_order(
            &self, // Immutable reference to `self` (the struct).
            event: &farukon_core::event::OrderEvent, // Reference to the order event.
            strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>, // Reference to instrument info map.
            strategy_settings: &farukon_core::settings::StrategySettings, // Reference to strategy settings.
            data_handler: &dyn farukon_core::data_handler::DataHandler, // Reference to the data handler (dyn trait object).
        ) -> anyhow::Result<()> {
            // Simulates market or limit order execution with slippage and commission.
            // Uses current bar's high/low for market orders.
            // For limit orders: checks if the price was hit during the bar.
            
            // Get the symbol (e.g., "Si-12.23") from the order event.
            let symbol = &event.symbol;
            // Get the instrument information (e.g., exchange, margin, step_price) from the map.
            let instruments_info = strategy_instruments_info.get(symbol)
                .ok_or_else(|| anyhow::anyhow!("No instrument info for {}", symbol))?; // Return an error if the information is missing.
            
            // Get the date/time of the latest bar for the symbol.
            let timeindex = data_handler.get_latest_bar_datetime(&event.symbol)
                .ok_or_else(|| anyhow::anyhow!("Failed to get latest bar datetime for symbol '{}' during order execution", event.symbol))?; // Return an error if date/time is unavailable.

            // Get the data of the latest bar (open, high, low, close, volume).
            let current_bar = data_handler.get_latest_bar(symbol)
                .ok_or_else(|| anyhow::anyhow!("No bar for {}", symbol))?; // Return an error if the bar is unavailable.

            // Determine the execution price based on the order type.
            let execution_price = match event.order_type.as_str() {
                "MKT" => {
                    // Market order: slippage is applied.
                    // Buy at High + slippage, Sell at Low - slippage.
                    if strategy_settings.slippage.len() == 1 {
                        // Check the order direction (BUY/SELL).
                        match event.direction.as_deref() {
                            // For a buy order, use the bar's High and add slippage.
                            Some("BUY") => (1.0 + strategy_settings.slippage[0]) * current_bar.high,
                            // For a sell order, use the bar's Low and subtract slippage.
                            Some("SELL") => (1.0 - strategy_settings.slippage[0]) * current_bar.low,
                            // If the direction is not specified or unknown, return a price of 0.0.
                            _ => 0.0,
                        }
                    } else {
                        // If the length of the slippage vector in the strategy settings is not 1, this is an error.
                        // The current implementation expects only a single slippage value for a market order.
                        anyhow::bail!("Wrong len of slippage vector!!");
                    }

                },
                "LMT" => {
                    // Limit order: execution occurs only if the price was reached during the bar.
                    // Use the specified limit price, or the bar's close price if the limit price is not specified.
                    let limit_price = event.limit_price.unwrap_or(current_bar.close);
                    // Check the direction of the limit order.
                    match event.direction.as_deref() {
                        // For a buy order, check if the bar's Low was less than or equal to the limit price.
                        Some("BUY") => {
                            if current_bar.low <= limit_price {
                                // If yes, the order is executed at the limit price.
                                limit_price
                            } else {
                                // If no, the order is not executed. Return Ok(()) without sending a FillEvent.
                                return anyhow::Ok(());
                            }
                        }
                        // For a sell order, check if the bar's High was greater than or equal to the limit price.
                        Some("SELL") => {
                            if current_bar.high >= limit_price {
                                // If yes, the order is executed at the limit price.
                                limit_price
                            } else {
                                // If no, the order is not executed. Return Ok(()) without sending a FillEvent.
                                return anyhow::Ok(());
                            }
                        }
                        // If the direction is not specified or unknown, the order is not executed.
                        _ => return anyhow::Ok(()),
                    }
                }
                // If the order type is neither "MKT" nor "LMT", the order is not executed.
                _ => return anyhow::Ok(()),
            };

            // Get the exchange name from the instrument information.
            let exchange = &instruments_info.exchange;
            // Calculate the commission for the trade using the execution price, instrument info, and strategy settings.
            let commission = farukon_core::commission_plans::calculate_forts_comission(
                Some(execution_price), // Pass the execution price.
                instruments_info,       // Pass the instrument information.
                strategy_settings       // Pass the strategy settings.
            );
            // Calculate the total commission by multiplying the commission per unit by the quantity of contracts.
            let total_commission = Some(commission.unwrap() * event.quantity);
            
            // Create a FillEvent with the details of the executed order.
            let fill_event = farukon_core::event::FillEvent::new(
                timeindex,              // Execution time (taken from the current bar).
                symbol.clone(),         // The instrument symbol.
                exchange.clone(),       // The exchange name.
                event.quantity,         // The number of contracts.
                event.direction.clone(), // The direction of the trade (BUY/SELL).
                Some(execution_price),  // The price at which the order was executed.
                total_commission,       // The total commission for the trade.
                event.signal_name.clone(), // The name of the signal that generated the order.
            );

            // Send the FillEvent to the event channel.
            self.event_sender.send(Box::new(fill_event))
                .map_err(|e| anyhow::anyhow!("Failed to send FillEvent: {}", e))?; // Wrap the send error in an anyhow::Error.
    
        // Return Ok(()) upon successful order execution and FillEvent sending.
        anyhow::Ok(())
    }
    
}

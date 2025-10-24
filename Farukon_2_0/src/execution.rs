pub struct SimulatedExecutionHandler {
    pub event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
}

impl SimulatedExecutionHandler {
    pub fn new(
        event_sender: std::sync::mpsc::Sender<Box<dyn farukon_core::event::Event>>,
    ) -> anyhow::Result<Self> {
        anyhow::Ok(
            Self {
                event_sender,
            }
        )
    }
}

impl farukon_core::execution::ExecutionHandler for SimulatedExecutionHandler {
    fn execute_order(
            &self,
            event: &farukon_core::event::OrderEvent,
            strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
            strategy_settings: &farukon_core::settings::StrategySettings,
            data_handler: &dyn farukon_core::data_handler::DataHandler,
        ) -> anyhow::Result<()> {
            let symbol = &event.symbol;
            let instruments_info = strategy_instruments_info.get(symbol)
                .ok_or_else(|| anyhow::anyhow!("No instrument info for {}", symbol))?;
            
            let timeindex = data_handler.get_latest_bar_datetime(&event.symbol)
                .ok_or_else(|| anyhow::anyhow!("Failed to get latest bar datetime for symbol '{}' during order execution", event.symbol))?;

            let current_bar = data_handler.get_latest_bar(symbol)
                .ok_or_else(|| anyhow::anyhow!("No bar for {}", symbol))?;

            let execution_price = match event.order_type.as_str() {
                "MKT" => {
                    if strategy_settings.slippage.len() == 1 {
                        match event.direction.as_deref() {
                            Some("BUY") => (1.0 + strategy_settings.slippage[0]) * current_bar.high,
                            Some("SELL") => (1.0 - strategy_settings.slippage[0]) * current_bar.low,
                            _ => 0.0,
                        }
                    } else {
                        anyhow::bail!("Wrong len of slippage vector!!");
                    }

                },
                "LMT" => {
                    let limit_price = event.limit_price.unwrap_or(current_bar.close);
                    match event.direction.as_deref() {
                        Some("BUY") => {
                            if current_bar.low <= limit_price {
                                limit_price
                            } else {
                                return anyhow::Ok(());
                            }
                        }
                        Some("SELL") => {
                            if current_bar.high >= limit_price {
                                limit_price
                            } else {
                                return anyhow::Ok(());
                            }
                        }
                        _ => return anyhow::Ok(()),
                    }
                }
                _ => return anyhow::Ok(()),
            };

            let exchange = &instruments_info.exchange;
            let commission = farukon_core::commission_plans::calculate_forts_comission(
                Some(execution_price),
                instruments_info,
                strategy_settings
            );
            let total_commission = Some(commission.unwrap() * event.quantity);
            
            let fill_event = farukon_core::event::FillEvent::new(
                timeindex,
                symbol.clone(),
                exchange.clone(),
                event.quantity,
                event.direction.clone(),
                Some(execution_price),
                total_commission,
                event.signal_name.clone(),
            );

            self.event_sender.send(Box::new(fill_event))
                .map_err(|e| anyhow::anyhow!("Failed to send FillEvent: {}", e))?;
    
        anyhow::Ok(())
    }
    
}
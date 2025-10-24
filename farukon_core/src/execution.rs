use crate::event;
use crate::settings;
use crate::data_handler;
use crate::instruments_info;

pub trait ExecutionHandler {
    fn execute_order(
        &self,
        event: &event::OrderEvent,
        strategy_instruments_info: &std::collections::HashMap<String, instruments_info::InstrumentInfo>,
        strategy_settings: &settings::StrategySettings,
        data_handler: &dyn data_handler::DataHandler,
    ) -> anyhow::Result<()>;
        
}

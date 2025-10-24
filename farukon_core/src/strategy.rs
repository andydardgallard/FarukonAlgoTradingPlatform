// farukon_core/src/strategy.rs

use crate::event;
use crate::portfolio;
use crate::data_handler;

pub trait Strategy {
    fn calculate_signals(
        &mut self,
        data_handler: &dyn data_handler::DataHandler,
        current_positions: &std::collections::HashMap<String, portfolio::PositionState>,
        latest_equity_point: &portfolio::EquitySnapshot,
        symbol_list: &[String],
    ) -> anyhow::Result<()>;

    fn open_by_limit(
        &self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    ) -> anyhow::Result<()> {
        event_sender.send(Box::new(event::SignalEvent::new(
            current_bar_datetime,
            symbol.clone(),
            signal_name.to_string(),
            "LMT".to_string(),
            quantity,
            limit_price,
        )))?;

        anyhow::Ok(())
    }

    fn close_by_limit(
        &self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    ) -> anyhow::Result<()> {
        event_sender.send(Box::new(event::SignalEvent::new(
            current_bar_datetime,
            symbol.clone(),
            signal_name.to_string(),
            "LMT".to_string(),
            quantity,
            limit_price,
        )))?;

        anyhow::Ok(())
    }

    fn open_by_market(
        &self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
    ) -> anyhow::Result<()> {
        event_sender.send(Box::new(event::SignalEvent::new(
            current_bar_datetime,
            symbol.clone(),
            signal_name.to_string(),
            "MKT".to_string(),
            quantity,
            None,
        )))?;

        anyhow::Ok(())
    }

    fn close_by_market(
        &self,
        event_sender: &std::sync::mpsc::Sender<Box<dyn event::Event>>,
        current_bar_datetime: chrono::DateTime<chrono::Utc>,
        symbol: &String,
        signal_name: &str,
        quantity: Option<f64>,
    ) -> anyhow::Result<()> {
        event_sender.send(Box::new(event::SignalEvent::new(
            current_bar_datetime,
            symbol.clone(),
            signal_name.to_string(),
            "MKT".to_string(),
            quantity,
            None,
        )))?;

        anyhow::Ok(())
    }

}

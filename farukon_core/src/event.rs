// farukon_core/src/event.rs

pub trait Event: std::fmt::Debug + Send + Sync {
    fn event_type(&self) -> &'static str;
    fn get_signal_event_params(&self) -> Option<&SignalEvent>;
    fn get_order_event_params(&self) -> Option<&OrderEvent>;
    fn get_fill_event_params(&self) -> Option<&FillEvent>;
}

// MARKET EVENT
#[derive(Debug)]
pub struct MarketEvent;

impl MarketEvent {
    pub fn new() -> Self {
        Self
    }
    
}

impl Event for MarketEvent {
    fn event_type (&self) -> &'static str {
        "MARKET"
    }

    fn get_signal_event_params(&self) -> Option<&SignalEvent> {
        None
    }

    fn get_order_event_params(&self) -> Option<&OrderEvent> {
        None
    }

    fn get_fill_event_params(&self) -> Option<&FillEvent> {
        None
    }

}

// SIGNAL EVENT
#[derive(Debug)]
pub struct SignalEvent {
    pub timeindex: chrono::DateTime<chrono::Utc>,
    pub symbol: String,
    pub signal_name: String,
    pub order_type: String,
    pub quantity: Option<f64>,
    pub limit_price: Option<f64>,
}

impl SignalEvent {
    pub fn new(
        timeindex: chrono::DateTime<chrono::Utc>,
        symbol: String,
        signal_name: String,
        order_type: String,
        quantity: Option<f64>,
        limit_price: Option<f64>,
    ) -> Self {
        Self {
            timeindex,
            symbol,
            signal_name,
            order_type,
            quantity,
            limit_price,
        }
    }

}

impl Event for SignalEvent {
    fn event_type (&self) -> &'static str {
        "SIGNAL"
    }

    fn get_signal_event_params(&self) -> Option<&SignalEvent> {
        Some(self)
    }

    fn get_order_event_params(&self) -> Option<&OrderEvent> {
        None
    }

    fn get_fill_event_params(&self) -> Option<&FillEvent> {
        None
    }

}

// ORDER EVENT
#[derive(Debug)]
pub struct OrderEvent {
    pub timeindex: chrono::DateTime<chrono::Utc>,
    pub symbol: String,
    pub signal_name: String,
    pub order_type: String,
    pub quantity: f64,          
    pub direction: Option<String>,
    pub limit_price: Option<f64>,
}

impl OrderEvent {
    pub fn new(
        timeindex: chrono::DateTime<chrono::Utc>,
        symbol: String,
        order_type: String,
        quantity: f64,
        direction: Option<String>,
        signal_name: String,
        limit_price: Option<f64>,
    ) -> Self {
        Self {
            timeindex,
            symbol,
            signal_name,
            order_type,
            quantity,
            direction,
            limit_price,
        }
    }

}

impl Event for OrderEvent {
    fn event_type (&self) -> &'static str {
        "ORDER"
    }

    fn get_signal_event_params(&self) -> Option<&SignalEvent> {
        None
    }

    fn get_order_event_params(&self) -> Option<&OrderEvent> {
        Some(self)
    }

    fn get_fill_event_params(&self) -> Option<&FillEvent> {
        None
    }

}

//FillEvent
#[derive(Debug)]
pub struct FillEvent {
    pub timeindex: chrono::DateTime<chrono::offset::Utc>,
    pub symbol: String,
    pub exchange: String,
    pub quantity: f64,      
    pub direction: Option<String>,
    pub execution_price: Option<f64>,
    pub commission: Option<f64>,
    pub signal_name: String,
}

impl FillEvent {
    pub fn new(
        timeindex: chrono::DateTime<chrono::offset::Utc>,
        symbol: String,
        exchange: String,
        quantity: f64,
        direction: Option<String>,
        execution_price: Option<f64>,
        commission: Option<f64>,
        signal_name: String,
    ) -> Self {
        Self {
            timeindex,
            symbol,
            exchange,
            quantity,
            direction,
            execution_price,
            commission,
            signal_name,
        }
    }
    
}

impl Event for FillEvent {  
    fn event_type (&self) -> &'static str {
        "FILL"
    }

    fn get_signal_event_params(&self) -> Option<&SignalEvent> {
        None
    }

    fn get_order_event_params(&self) -> Option<&OrderEvent> {
        None
    }

    fn get_fill_event_params(&self) -> Option<&FillEvent> {
        Some(self)
    }

}

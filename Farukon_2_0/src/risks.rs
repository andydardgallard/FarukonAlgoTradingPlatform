// Farukon_2_0/src/risks.rs

use farukon_core;

pub fn margin_call_control_for_signal(
    quantity: f64,
    latest_equity_point: &farukon_core::portfolio::EquitySnapshot,
    signal_event: &farukon_core::event::SignalEvent,
    instrument_info: &farukon_core::instruments_info::InstrumentInfo,
) -> anyhow::Result<bool> {
    // Checks if sufficient capital exists to open a new position.
    // Used during SIGNAL → ORDER conversion.

    let symbol = &signal_event.symbol;
    let signal_name = &signal_event.signal_name;

    if signal_name != "EXIT" {
        let instrument_type = &instrument_info.instrument_type;
        if instrument_type == "futures" {
            let margin = instrument_info.margin;
            let initial_margin = quantity * margin;
            let capital = latest_equity_point.equity_point.capital;

            if capital > initial_margin {
                return anyhow::Ok(true);
            } else {
                println!("Not enough initial margin {initial_margin} to entry {symbol} #{quantity} = {capital}! Order will not send!");
                return anyhow::Ok(false);
            }
        }
    } else {
        return anyhow::Ok(true);    // EXIT always allowed
    }

    anyhow::Ok(true)
}

pub fn margin_call_control_for_market(
    latest_equity_point: &farukon_core::portfolio::EquitySnapshot,
    current_positions: &std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
    strategy_settings: &farukon_core::settings::StrategySettings,
    strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
) -> anyhow::Result<bool> {
    // Checks if current portfolio has sufficient equity to maintain open positions.
    // Triggers margin call if capital < min_margin * total_position_value.

    let cash = latest_equity_point.equity_point.cash;
    if cash < 0.0 {
        let mut capital = 0.0;
        
        for symbol in &strategy_settings.symbols {
            let instrument_info = strategy_instruments_info.get(symbol).unwrap();
            if instrument_info.instrument_type == "futures" {
                let current_position_for_symbol = current_positions.get(symbol).unwrap().position;
                if current_position_for_symbol != 0.0 {
                    let entry_capital_for_symbol = current_positions.get(symbol).unwrap().entry_capital;
                    capital += entry_capital_for_symbol;
                }
            }
        }
        let min_margin_param = strategy_settings.margin_params.min_margin;
        let min_margin_for_strategy = capital * min_margin_param;
        let strategy_current_capital = latest_equity_point.equity_point.capital;

        if strategy_current_capital < min_margin_for_strategy {
            println!("Not enough minimal margin {} with {} of cash!", min_margin_for_strategy, strategy_current_capital);
            return anyhow::Ok(false);
        } else {
            return anyhow::Ok(true);
        }
    }

    anyhow::Ok(true)
}

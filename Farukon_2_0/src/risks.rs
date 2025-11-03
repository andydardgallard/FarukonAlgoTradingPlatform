// Farukon_2_0/src/risks.rs

use farukon_core;

/// Checks if sufficient capital exists to execute a signal.
/// Used during SIGNAL → ORDER conversion.
/// # Arguments
/// * `quantity` - The quantity to trade.
/// * `latest_equity_point` - The latest equity point.
/// * `signal_event` - The signal event.
/// * `instrument_info` - The instrument metadata.
/// # Returns
/// * `anyhow::Result<bool>` indicating whether the signal can be executed.
pub fn margin_call_control_for_signal(
    quantity: f64,
    latest_holdings: &farukon_core::portfolio::HoldingSnapshot,
    signal_event: &farukon_core::event::SignalEvent,
    instrument_info: &farukon_core::instruments_info::InstrumentInfo,
) -> anyhow::Result<bool> {
    // Checks if sufficient capital exists to open a new position.
    // Used during SIGNAL → ORDER conversion.

    let signal_name = &signal_event.signal_name;
    
    if signal_name != "EXIT" {
        let symbol = &signal_event.symbol;
        let instrument_type = &instrument_info.instrument_type;
        
        if instrument_type == "futures" {
            let margin = instrument_info.margin;
            let initial_margin = quantity * margin;
            let capital = latest_holdings.capital;

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

/// Checks if current portfolio has sufficient equity to maintain open positions.
/// Triggers margin call if capital < min_margin * total_position_value.
/// # Arguments
/// * `latest_equity_point` - The latest equity point.
/// * `current_positions` - The current positions.
/// * `strategy_settings` - The strategy settings.
/// * `strategy_instruments_info` - The instrument metadata.
/// # Returns
/// * `anyhow::Result<bool>` indicating whether a margin call has occurred.
pub fn margin_call_control_for_market(
    latest_holdings: &farukon_core::portfolio::HoldingSnapshot,
    current_positions: &std::collections::HashMap<String, farukon_core::portfolio::PositionState>,
    strategy_settings: &farukon_core::settings::StrategySettings,
    strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
) -> anyhow::Result<bool> {
    // Checks if current portfolio has sufficient equity to maintain open positions.
    // Triggers margin call if capital < min_margin * total_position_value.

    let cash = latest_holdings.cash;
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
        let strategy_current_capital = latest_holdings.capital;

        if strategy_current_capital < min_margin_for_strategy {
            println!("Not enough minimal margin {} with {} of cash!", min_margin_for_strategy, strategy_current_capital);
            return anyhow::Ok(false);
        } else {
            return anyhow::Ok(true);
        }
    }

    anyhow::Ok(true)
}

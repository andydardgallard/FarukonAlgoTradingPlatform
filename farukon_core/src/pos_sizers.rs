//farukon_core/src/pos_sizers.rs

use crate::settings;
use crate::commission_plans;
use crate::instruments_info;

fn mpr(
    mode: &String,
    capital: f64,
    entry_price: f64,
    exit_price: f64,
    strategy_settings: &settings::StrategySettings,
    strategy_instruments_info_for_symbol: &instruments_info::InstrumentInfo,
) -> Option<f64> {
    let mpr = strategy_settings.pos_sizer_params.pos_sizer_value[0];
    let full_commission = match strategy_instruments_info_for_symbol.exchange.as_str() {
            "FORTS" => {
                let entry_commission = commission_plans::calculate_forts_comission(
                    Some(entry_price),
                    strategy_instruments_info_for_symbol,
                    strategy_settings,
                ).unwrap();
                let exit_commission = commission_plans::calculate_forts_comission(
                    Some(exit_price),
                    strategy_instruments_info_for_symbol,
                    strategy_settings
                ).unwrap();

                Some(entry_commission + exit_commission)
            } 
            _ => None,
    };

    let risk_per_deal_in_points = (exit_price - entry_price).abs();
    let point_value = ((strategy_instruments_info_for_symbol.step_price / strategy_instruments_info_for_symbol.step) * 100_000.0).round() / 100_000.0;
    let risk_per_deal_in_value_net = risk_per_deal_in_points * point_value;
    let risk_per_deal_in_value_gross = risk_per_deal_in_value_net + full_commission?;
    let max_percent_risk = capital * (mpr / 100.0);
    let points_from_zero = strategy_instruments_info_for_symbol.contract_precision as i32;

    if mode == "Debug" {
        println!("risk_per_deal_in_points: {}, point_value: {}, risk_per_deal_in_value_net: {}, risk_per_deal_in_value_gross: {}, max_percent_risk: {}, capital: {}", risk_per_deal_in_points, point_value, risk_per_deal_in_value_net, risk_per_deal_in_value_gross, max_percent_risk, capital);
    }
                    
    Some(((max_percent_risk / risk_per_deal_in_value_gross) * 10.0_f64.powi(points_from_zero)).floor() / 10.0_f64.powi(points_from_zero))
}

pub fn get_pos_sizer_from_settings(
    mode: &String,
    capital: Option<f64>,
    entry_price: Option<f64>,
    exit_price: Option<f64>,
    strategy_settings: &settings::StrategySettings,
    strategy_instruments_info_for_symbol: &instruments_info::InstrumentInfo,
) -> Option<f64> {
    if strategy_settings.pos_sizer_params.pos_sizer_value.len() == 1 {
        let quantity = match strategy_settings.pos_sizer_params.pos_sizer_name.as_str() {
            "1" => Some(1.0),
            "mpr" => mpr(
                mode,
                capital?,
                entry_price?,
                exit_price?,
                strategy_settings,
                strategy_instruments_info_for_symbol,
            ),
            "poe" => None, // TODO
            _ => None,
        };
        
        quantity
    } else { None }
}

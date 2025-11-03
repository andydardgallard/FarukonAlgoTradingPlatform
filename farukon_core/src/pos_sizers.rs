// farukon_core/src/pos_sizers.rs

//! Position sizing module.
//!
//! This module implements risk-aware position sizing strategies that determine trade quantity
//! based on available capital, instrument metadata, and user-defined risk parameters.
//! It supports multiple sizing methods, including fixed-lot and risk-based approaches,
//! and incorporates transaction costs (commissions) where applicable.
//!
//! Currently supported position sizers:
//! - `"1"`: Fixed-size lot (default: 1 contract).
//! - `"mpr"`: Maximum Percent Risk — sizes positions so that the potential loss
//!            (including commissions) does not exceed a user-defined percentage of capital.
//! - `"poe"`: (Planned) Percent of Equity — sizes positions as a percentage of total equity.
//!
//! Commission handling is exchange-specific. As of now, only FORTS exchange commissions
//! are modeled, using the `calculate_forts_comission` function from the `commission_plans` module.
//! Commissions for both entry and exit are included in risk calculations for the MPR method.

use crate::settings;
use crate::commission_plans;
use crate::instruments_info;

/// Calculates the position size using the "MPR" (Maximum Possible Risk) method.
/// # Arguments
/// * `mode` - Operational mode (Debug, Optimize, etc.).
/// * `capital` - Available capital.
/// * `entry_price` - Entry price for the trade.
/// * `exit_price` - Exit price for the trade.
/// * `strategy_settings` - Strategy settings.
/// * `instrument_info` - Instrument metadata.
/// # Returns
/// * An optional `f64` representing the quantity to trade.
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
        println!(
            "risk_per_deal_in_points: {}, point_value: {}, risk_per_deal_in_value_net: {}, risk_per_deal_in_value_gross: {}, max_percent_risk: {}, capital: {}",
            risk_per_deal_in_points, point_value, risk_per_deal_in_value_net, risk_per_deal_in_value_gross, max_percent_risk, capital
        );
    }
                    
    Some(((max_percent_risk / risk_per_deal_in_value_gross) * 10.0_f64.powi(points_from_zero)).floor() / 10.0_f64.powi(points_from_zero))
}

/// Returns a fixed position size of 1.0 contract.
///
/// Primarily used for baseline testing or strategies that do not require dynamic sizing.
/// In `"Debug"` mode, logs point value and entry commission for diagnostics.
fn plain_pos_sizer(
    mode: &String,
    entry_price: f64,
    strategy_settings: &settings::StrategySettings,
    strategy_instruments_info_for_symbol: &instruments_info::InstrumentInfo,
) -> Option<f64> {
    if mode == "Debug" {
        let point_value = ((strategy_instruments_info_for_symbol.step_price / strategy_instruments_info_for_symbol.step) * 100_000.0).round() / 100_000.0;
        let commission = match strategy_instruments_info_for_symbol.exchange.as_str() {
            "FORTS" => {
                let entry_commission = commission_plans::calculate_forts_comission(
                    Some(entry_price),
                    strategy_instruments_info_for_symbol,
                    strategy_settings,
                ).unwrap();

                Some(entry_commission)
            } 
            _ => None,
        };

        println!("point_value: {}, commision: {:?}", point_value, commission);
    }
    Some(1.0)
}

/// Calculates the position size based on the strategy settings.
/// # Arguments
/// * `mode` - Operational mode.
/// * `capital` - Available capital.
/// * `price` - Current price.
/// * `long_sma` - Long-term SMA (optional).
/// * `strategy_settings` - Strategy settings.
/// * `instrument_info` - Instrument metadata.
/// # Returns
/// * An optional `f64` representing the quantity to trade.
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
            "1" => plain_pos_sizer(
                mode,
                entry_price?,
                strategy_settings,
                strategy_instruments_info_for_symbol
            ),
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

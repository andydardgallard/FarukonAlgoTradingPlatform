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

use crate::commission_plans;
use crate::instruments_info;
use crate::settings;
use crate::portfolio;

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
    holdings: &Vec<portfolio::HoldingSnapshot>,
    entry_price: f64,
    exit_price: f64,
    strategy_settings: &settings::StrategySettings,
    strategy_instruments_info_for_symbol: &instruments_info::InstrumentInfo,
) -> Option<f64> {
    let mpr = {
        match &strategy_settings.pos_sizer_params.pos_sizer_value {
            settings::ParamSpec::Discrete(values) => {
                if let Some(value) = values.first() {
                    value.as_f64()?
                } else {
                    return None;
                }
            }
            settings::ParamSpec::Range { .. } => {
                return None;
            }
        }
    };

    let full_commission = match strategy_instruments_info_for_symbol.exchange.as_str() {
        "FORTS" => {
            let entry_commission = commission_plans::calculate_forts_comission(
                Some(entry_price),
                strategy_instruments_info_for_symbol,
                strategy_settings,
            )
            .unwrap();
            let exit_commission = commission_plans::calculate_forts_comission(
                Some(exit_price),
                strategy_instruments_info_for_symbol,
                strategy_settings,
            )
            .unwrap();

            Some(entry_commission + exit_commission)
        }
        _ => None,
    };

    let current_capital = holdings.last().expect("No data in latest HoldingSnapshot").capital;
    let risk_per_deal_in_points = (exit_price - entry_price).abs();
    let point_value = ((strategy_instruments_info_for_symbol.step_price
        / strategy_instruments_info_for_symbol.step)
        * 100_000.0)
        .round()
        / 100_000.0;
    let risk_per_deal_in_value_net = risk_per_deal_in_points * point_value;
    let risk_per_deal_in_value_gross = risk_per_deal_in_value_net + full_commission?;
    let max_percent_risk = current_capital * (mpr / 100.0);
    let points_from_zero = strategy_instruments_info_for_symbol.contract_precision as i32;

    if mode == "Debug" {
        println!(
            "risk_per_deal_in_points: {}, point_value: {}, risk_per_deal_in_value_net: {}, risk_per_deal_in_value_gross: {}, max_percent_risk: {}, capital: {}",
            risk_per_deal_in_points,
            point_value,
            risk_per_deal_in_value_net,
            risk_per_deal_in_value_gross,
            max_percent_risk,
            current_capital
        );
    }

    Some(
        ((max_percent_risk / risk_per_deal_in_value_gross) * 10.0_f64.powi(points_from_zero))
            .floor()
            / 10.0_f64.powi(points_from_zero)
    )
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
        let point_value = ((strategy_instruments_info_for_symbol.step_price
            / strategy_instruments_info_for_symbol.step)
            * 100_000.0)
            .round()
            / 100_000.0;
        let commission = match strategy_instruments_info_for_symbol.exchange.as_str() {
            "FORTS" => {
                let entry_commission = commission_plans::calculate_forts_comission(
                    Some(entry_price),
                    strategy_instruments_info_for_symbol,
                    strategy_settings,
                )
                .unwrap();

                Some(entry_commission)
            }
            _ => None,
        };

        println!("point_value: {}, commision: {:?}", point_value, commission);
    }
    Some(1.0)
}

/// Calculates the position size using the "fixed_ratio" (by Ryan Jhones) method.
/// # Arguments
/// * `mode` - Operational mode (Debug, Optimize, etc.).
/// * `pnl` - Current pnl of strategy.
/// 
/// * `entry_price` - Entry price for the trade.
/// * `exit_price` - Exit price for the trade.
/// * `strategy_settings` - Strategy settings.
/// * `instrument_info` - Instrument metadata.
/// # Returns
/// * An optional `f64` representing the quantity to trade.
fn fixed_ratio(
    mode: &String,
    holdings: &Vec<portfolio::HoldingSnapshot>,
    strategy_settings: &settings::StrategySettings,
    strategy_instruments_info_for_symbol: &instruments_info::InstrumentInfo,
) -> Option<f64> {
    let fixed_ratio = {
        match &strategy_settings.pos_sizer_params.pos_sizer_value {
            settings::ParamSpec::Discrete(values) => {
                if let Some(value) = values.first() {
                    value.as_f64()?
                } else {
                    return None;
                }
            }
            settings::ParamSpec::Range { .. } => {
                return None;
            }
        }
    };

    let initial_capital = holdings.first().expect("No data in Holdings").capital;
    let current_capital = holdings.last().expect("No data in latest HoldingSnapshot").capital;
    let pnl = current_capital - initial_capital;

    let points_from_zero = strategy_instruments_info_for_symbol.contract_precision as i32;
    let discriminant = (fixed_ratio * fixed_ratio / 4.0) + (2.0 * fixed_ratio * pnl);
    let ratio = (fixed_ratio / 2.0 + discriminant.sqrt()) / fixed_ratio;

    if mode == "Debug" {
        println!(
            "initial_capital: {}, current_capital: {}, common_pnl: {}, fixed_ratio: {}",
            initial_capital,
            current_capital,
            pnl,
            fixed_ratio
        );
    }

    Some(
        if pnl <= 0.0 {
            1.0
        } else {
            (ratio * 10.0_f64.powi(points_from_zero)).floor() / 10.0_f64.powi(points_from_zero)
        }
    )
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
    holdings: &Vec<portfolio::HoldingSnapshot>,
    entry_price: Option<f64>,
    exit_price: Option<f64>,
    strategy_settings: &settings::StrategySettings,
    strategy_instruments_info_for_symbol: &instruments_info::InstrumentInfo,
) -> Option<f64> {
    let is_single_value = match &strategy_settings.pos_sizer_params.pos_sizer_value {
        settings::ParamSpec::Discrete(values) => values.len() == 1,
        settings::ParamSpec::Range { .. } => false,
    };

    if is_single_value {
        let quantity = match strategy_settings.pos_sizer_params.pos_sizer_name.as_str() {
            "1" => plain_pos_sizer(
                mode,
                entry_price?,
                strategy_settings,
                strategy_instruments_info_for_symbol,
            ),
            "mpr" => mpr(
                mode,
                holdings,
                entry_price?,
                exit_price?,
                strategy_settings,
                strategy_instruments_info_for_symbol,
            ),
            "fixed_ratio" => fixed_ratio(
                mode,
                holdings,
                strategy_settings,
                strategy_instruments_info_for_symbol
            ),
            "poe" => None, // TODO
            _ => None,
        };

        quantity
    } else {
        None
    }
}

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
//!   (including commissions) does not exceed a user-defined percentage of capital.
//! - `"poe"`: Percent of Equity — sizes positions so that the entry margin does not exceed
//!   a user-defined percentage of the current equity.
//! - `"fixed_ratio"`: The fixed-ratio (Ryan Jones) method, based on the strategy PnL.
//!
//! Every sizer returns `Option<f64>`. `None` means "this configuration cannot be sized"
//! (invalid parameter, missing commission plan, non-positive equity, ...). Callers must treat
//! it as an error — never emit an order and never panic, because these functions run inside the
//! `extern "C"` strategy call.
//!
//! Commission handling is exchange-specific. As of now, only FORTS exchange commissions
//! are modeled, using the `calculate_forts_comission` function from the `commission_plans` module.
//! Commissions for both entry and exit are included in risk calculations for the MPR method.

use crate::commission_plans;
use crate::instruments_info;
use crate::portfolio;
use crate::settings;

/// Calculates the position size using the "MPR" (Maximum Possible Risk) method.
///
/// Returns `None` when the risk cannot be computed — invalid `pos_sizer_value`, a non-FORTS
/// exchange, or a missing/zero commission plan. `None` is propagated to the order generation
/// step as a normal error instead of aborting inside the `extern "C"` strategy call.
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
    holdings: &[portfolio::HoldingSnapshot],
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
            )?;
            let exit_commission = commission_plans::calculate_forts_comission(
                Some(exit_price),
                strategy_instruments_info_for_symbol,
                strategy_settings,
            )?;

            Some(entry_commission + exit_commission)
        }
        _ => None,
    };

    let current_capital = holdings
        .last()
        .expect("No data in latest HoldingSnapshot")
        .capital;
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
            / 10.0_f64.powi(points_from_zero),
    )
}

/// Calculates the position size using the "poe" (Percent Of Equity) method.
///
/// `pos_sizer_value` is the percentage of the current capital that may be blocked as entry
/// margin. The resulting size is floored to whole contracts at the instrument's contract
/// precision, so a small equity (or a large margin) yields `0.0` — the caller must skip the
/// order instead of forcing one contract.
///
/// Returns `None` when the configuration cannot be sized (non-positive percentage or margin,
/// empty holdings).
///
/// # Arguments
/// * `mode` - Operational mode (Debug, Optimize, etc.).
/// * `holdings` - Holdings snapshots (the latest one carries the current capital).
/// * `strategy_settings` - Strategy settings.
/// * `strategy_instruments_info_for_symbol` - Instrument metadata.
/// # Returns
/// * An optional `f64` representing the quantity to trade.
fn poe(
    mode: &String,
    holdings: &[portfolio::HoldingSnapshot],
    strategy_settings: &settings::StrategySettings,
    strategy_instruments_info_for_symbol: &instruments_info::InstrumentInfo,
) -> Option<f64> {
    let percent = match &strategy_settings.pos_sizer_params.pos_sizer_value {
        settings::ParamSpec::Discrete(values) => values.first()?.as_f64()?,
        settings::ParamSpec::Range { .. } => return None,
    };

    let current_capital = holdings.last()?.capital;
    let margin = strategy_instruments_info_for_symbol.margin;

    if percent <= 0.0 || percent.is_nan() || margin <= 0.0 || margin.is_nan() {
        return None;
    }

    let points_from_zero = strategy_instruments_info_for_symbol.contract_precision as i32;
    let quantity = ((current_capital * percent / 100.0) / margin * 10.0_f64.powi(points_from_zero))
        .floor()
        / 10.0_f64.powi(points_from_zero);

    if mode == "Debug" {
        println!(
            "percent_of_equity: {}, capital: {}, margin: {}, quantity: {}",
            percent, current_capital, margin, quantity
        );
    }

    Some(quantity)
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
            // A missing commission plan is a normal `None` for the caller: printing the diagnostic
            // without one must not panic inside the `extern "C"` strategy call (this check runs in
            // the Optimize flow too, where `commission_plans` may be absent).
            "FORTS" => commission_plans::calculate_forts_comission(
                Some(entry_price),
                strategy_instruments_info_for_symbol,
                strategy_settings,
            ),
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
    holdings: &[portfolio::HoldingSnapshot],
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
    let current_capital = holdings
        .last()
        .expect("No data in latest HoldingSnapshot")
        .capital;
    let pnl = current_capital - initial_capital;

    let points_from_zero = strategy_instruments_info_for_symbol.contract_precision as i32;
    let discriminant = (fixed_ratio * fixed_ratio / 4.0) + (2.0 * fixed_ratio * pnl);
    let ratio = (fixed_ratio / 2.0 + discriminant.sqrt()) / fixed_ratio;

    if mode == "Debug" {
        println!(
            "initial_capital: {}, current_capital: {}, common_pnl: {}, fixed_ratio: {}",
            initial_capital, current_capital, pnl, fixed_ratio
        );
    }

    Some(if pnl <= 0.0 {
        1.0
    } else {
        (ratio * 10.0_f64.powi(points_from_zero)).floor() / 10.0_f64.powi(points_from_zero)
    })
}

/// Calculates the position size based on the strategy settings.
///
/// # Returns
/// * `Some(quantity)` for a sizer that could size the position, `None` when the configuration
///   cannot be sized (unknown/unsupported sizer, invalid parameters, missing commission plan).
///   The caller must treat `None` as an error before emitting an order.
///
/// # Arguments
/// * `mode` - Operational mode.
/// * `holdings` - Holdings snapshots (capital/cash history for risk-based sizing).
/// * `entry_price` - Current (entry) price, if known.
/// * `exit_price` - Planned exit price used for the risk estimate, if known.
/// * `strategy_settings` - Strategy settings.
/// * `strategy_instruments_info_for_symbol` - Instrument metadata.
pub fn get_pos_sizer_from_settings(
    mode: &String,
    holdings: &[portfolio::HoldingSnapshot],
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
        match strategy_settings.pos_sizer_params.pos_sizer_name.as_str() {
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
                strategy_instruments_info_for_symbol,
            ),
            "poe" => poe(
                mode,
                holdings,
                strategy_settings,
                strategy_instruments_info_for_symbol,
            ),
            _ => None,
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holdings(capital: f64) -> Vec<portfolio::HoldingSnapshot> {
        vec![portfolio::HoldingSnapshot::new(
            chrono::DateTime::from_timestamp(0, 0).unwrap(),
            capital,
            capital,
            0.0,
            std::collections::HashMap::new(),
        )]
    }

    fn instrument(margin: f64, precision: usize) -> instruments_info::InstrumentInfo {
        instruments_info::InstrumentInfo {
            exchange: "FORTS".to_string(),
            instrument_type: "futures".to_string(),
            contract_precision: precision,
            margin,
            commission_type: "currency".to_string(),
            trade_from_date: "2025-01-01 00:00:00".to_string(),
            expiration_date: "2025-12-31 00:00:00".to_string(),
            marginal_costs: 0.0,
            step: 1.0,
            step_price: 1.0,
        }
    }

    fn settings_with_sizer(name: &str, value: f64) -> settings::StrategySettings {
        let mut s = settings::StrategySettings {
            threads: None,
            strategy_name: "test".to_string(),
            strategy_path: String::new(),
            exit_results_path: String::new(),
            strategy_weight: 1.0,
            slippage: settings::ParamSpec::Discrete(vec![]),
            data: settings::DataSettings {
                data_path: String::new(),
                timeframe: String::new(),
            },
            symbols: vec!["Si-12.23".to_string()],
            strategy_params: std::collections::HashMap::new(),
            pos_sizer_params: settings::PosSizer {
                pos_sizer_name: name.to_string(),
                pos_sizer_params: std::collections::HashMap::new(),
                pos_sizer_value: settings::ParamSpec::Discrete(vec![serde_json::Value::from(
                    value,
                )]),
            },
            margin_params: settings::MarginParams {
                min_margin: 0.0,
                margin_call_type: String::new(),
            },
            portfolio_settings_for_strategy: settings::PortfolioSettingsForStrategy {
                metrics_calculation_mode: settings::MetricsMode::Offline,
            },
            optimizer_type: settings::OptimizerType::GridSearch,
            commission_plans: None,
        };
        s.commission_plans = Some(commission_plans::CommissionPlans {
            exchanges: std::collections::HashMap::from([(
                "FORTS".to_string(),
                std::collections::HashMap::from([(
                    "default".to_string(),
                    serde_json::json!({ "currency": 0.5 }),
                )]),
            )]),
        });
        s
    }

    /// `poe` used to return `None`, which crashed order generation. It must size the position
    /// from the requested share of equity.
    #[test]
    fn poe_sizes_a_position_from_the_requested_percent_of_equity() {
        let strategy_settings = settings_with_sizer("poe", 10.0);
        let quantity = get_pos_sizer_from_settings(
            &"Optimize".to_string(),
            &holdings(10_000.0),
            Some(100.0),
            Some(90.0),
            &strategy_settings,
            &instrument(100.0, 0),
        );

        // 10% of 10_000 = 1_000 of margin, 100 per contract -> 10 contracts.
        assert_eq!(quantity, Some(10.0));
    }

    /// An unallocatable share of equity is a real answer (`0.0`), not a silent crash, and a
    /// non-positive margin is reported as an error (`None`).
    #[test]
    fn poe_reports_zero_quantity_and_rejects_invalid_margin() {
        let strategy_settings = settings_with_sizer("poe", 0.5);
        assert_eq!(
            get_pos_sizer_from_settings(
                &"Optimize".to_string(),
                &holdings(100.0),
                Some(1.0),
                Some(1.0),
                &strategy_settings,
                &instrument(1_000.0, 0),
            ),
            Some(0.0)
        );

        assert_eq!(
            get_pos_sizer_from_settings(
                &"Optimize".to_string(),
                &holdings(100.0),
                Some(1.0),
                Some(1.0),
                &strategy_settings,
                &instrument(0.0, 0),
            ),
            None
        );
    }

    /// MPR without a matching commission plan must return `None` (a normal error for the caller)
    /// instead of unwrapping inside the `extern "C"` strategy call.
    #[test]
    fn mpr_without_commission_plan_returns_none() {
        let mut strategy_settings = settings_with_sizer("mpr", 1.0);
        strategy_settings.commission_plans = None;

        assert_eq!(
            get_pos_sizer_from_settings(
                &"Optimize".to_string(),
                &holdings(10_000.0),
                Some(100.0),
                Some(90.0),
                &strategy_settings,
                &instrument(100.0, 0),
            ),
            None
        );
    }

    /// The `Debug` diagnostic of the fixed-lot sizer used to unwrap the commission calculation,
    /// which is `None` whenever no commission plan is configured — a panic on the `extern "C"`
    /// strategy path. The diagnostic must degrade to "no commission" and still size the order.
    #[test]
    fn debug_fixed_lot_without_commission_plan_still_sizes_the_order() {
        let mut strategy_settings = settings_with_sizer("1", 1.0);
        strategy_settings.commission_plans = None;

        assert_eq!(
            get_pos_sizer_from_settings(
                &"Debug".to_string(),
                &holdings(10_000.0),
                Some(100.0),
                Some(90.0),
                &strategy_settings,
                &instrument(100.0, 0),
            ),
            Some(1.0)
        );
    }

    #[test]
    fn debug_fixed_lot_with_a_commission_plan_still_sizes_the_order() {
        let strategy_settings = settings_with_sizer("1", 1.0);

        assert_eq!(
            get_pos_sizer_from_settings(
                &"Debug".to_string(),
                &holdings(10_000.0),
                Some(100.0),
                Some(90.0),
                &strategy_settings,
                &instrument(100.0, 0),
            ),
            Some(1.0)
        );
    }
}

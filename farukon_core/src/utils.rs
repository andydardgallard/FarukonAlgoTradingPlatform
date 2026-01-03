// farukon_core/src/utils.rs

//! Utility functions for the Farukon platform.
//! Includes helper functions for parsing settings, calculating quantities, and validating data.

use std::io::Write;
use anyhow::Context;

use crate::settings;
use crate::optimization;
use crate::instruments_info;

/// Converts a string representation of a date and time into a `chrono::DateTime<chrono::Utc>`.
/// This function uses `chrono::NaiveDateTime::parse_from_str` to parse the input string according to the provided format,
/// then converts the resulting `NaiveDateTime` to a `DateTime<Utc>`.
///
/// # Arguments
/// * `string` - The date-time string to parse (e.g., "2025-07-08 15:30:00").
/// * `format` - The expected format of the input string (e.g., "%Y-%m-%d %H:%M:%S").
///
/// # Returns
/// * `anyhow::Result<chrono::DateTime<chrono::Utc>>` - The parsed UTC date-time on success, or an error if parsing fails.
pub fn string_to_date_time(string: &String, format: &str) -> anyhow::Result<chrono::DateTime<chrono::Utc>> {
    // Format "%Y-%m-%d %H:%M:%S"
    let dt = chrono::NaiveDateTime::parse_from_str(
        string,
        format,
    ).with_context(|| format!(
        "Invalid format '{}'",
        format
    ))?;

    let dt_utc = chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
        dt,
        chrono::Utc
    );

    anyhow::Ok(dt_utc)
}

/// Calculates the maximum available quantity to trade based on available capital.
/// # Arguments
/// * `cash` - Available cash.
/// * `quantity` - Desired quantity.
/// * `instrument_info` - Instrument metadata.
/// # Returns
/// * `f64` representing the maximum available quantity.
pub fn calculate_max_available_quantity(
    cash: f64,
    mut current_quantity: f64,
    strategy_instruments_info: &instruments_info::InstrumentInfo,
) -> f64 {
    let margin = strategy_instruments_info.margin;
    let precision = strategy_instruments_info.contract_precision as i32;
    let max_quantity = ((cash / margin) * 10.0_f64.powi(precision)).floor() / 10.0_f64.powi(precision);

    current_quantity = current_quantity.min(max_quantity.abs());

    if current_quantity == 0.0 {
        current_quantity += 1.0 / 10.0_f64.powi(precision);
    }

    current_quantity
}

/// Parses optimization configuration from strategy settings.
/// # Arguments
/// * `strategy_settings` - The strategy settings.
/// # Returns
/// * `OptimizationConfig` containing the parsed configuration.
pub fn parse_optimization_config(
    strategy_settings: &settings::StrategySettings,
) -> optimization::OptimizationConfig {
    let strategy_params_ranges: std::collections::HashMap<String, settings::ParamSpec> = strategy_settings
        .strategy_params
        .iter()
        .map(|(name, spec)| (name.clone(), spec.clone()))
        .collect();
    
    let pos_sizer_value_range = strategy_settings.pos_sizer_params.pos_sizer_value.clone();
    let slippage_range = strategy_settings.slippage.clone();

    let pos_sizer_additional_params: std::collections::HashMap<String, settings::ParamSpec> = strategy_settings
        .pos_sizer_params
        .pos_sizer_params
        .iter()
        .map(|(name, spec)| (name.clone(), spec.clone()))
        .collect();

    optimization::OptimizationConfig::new()
        .with_strategy_params_ranges(strategy_params_ranges)
        .with_pos_sizer_name(strategy_settings.pos_sizer_params.pos_sizer_name.clone())
        .with_pos_sizer_value_ranges(pos_sizer_value_range)
        .with_slippage_range(slippage_range)
        .with_pos_sizer_additional_params(pos_sizer_additional_params)
}

/// Creates a new strategy settings object from a parameter set.
/// # Arguments
/// * `original_settings` - The original strategy settings.
/// * `parameters` - The parameter set to use.
/// # Returns
/// * `StrategySettings` with updated parameters.
pub fn create_stratagy_settings_from_params(
    strategy_settings: &settings::StrategySettings,
    params: &optimization::ParameterSet,
) -> settings::StrategySettings {
    let mut new_strategy_settings = strategy_settings.clone();

    new_strategy_settings.pos_sizer_params.pos_sizer_value = settings::ParamSpec::Discrete(vec![serde_json::Value::Number(serde_json::Number::from_f64(*params.get_pos_sizer_value()).unwrap())]);
    new_strategy_settings.slippage = settings::ParamSpec::Discrete(vec![serde_json::Value::Number(serde_json::Number::from_f64(*params.get_slippage()).unwrap())]);

    let mut map = strategy_settings.strategy_params.clone();
    for (key, selected_value) in params.get_strategy_params() {
        map.insert(key.clone(), settings::ParamSpec::Discrete(vec![selected_value.clone()]));
    }
    new_strategy_settings.strategy_params = map;

    let mut ps_params_map = strategy_settings.pos_sizer_params.pos_sizer_params.clone();
    for (key, selected_value) in params.get_pos_sizer_additional_params() {
        ps_params_map.insert(key.clone(), settings::ParamSpec::Discrete(vec![selected_value.clone()]));
    }
    new_strategy_settings.pos_sizer_params.pos_sizer_params = ps_params_map;

    new_strategy_settings
}

/// Exports the equity curve to a CSV file.
/// # Arguments
/// * `filename` - The name of the output file.
/// # Returns
/// * `anyhow::Result<()>` indicating success or failure.
pub fn export_equity_to_csv(
    equity_series: &Vec<(chrono::DateTime<chrono::Utc>, f64)>,
    strategy_settings: &settings::StrategySettings,
) -> anyhow::Result<()> {
    let path = format!("{}/equity_series.csv", strategy_settings.exit_results_path);

    let mut file = std::fs::File::create(path)?;
    writeln!(file, "datetime;capital")?;
    for point in equity_series {
        writeln!(file, "{};{}", point.0.format("%Y-%m-%d %H:%M:%S"), point.1)?;
    }

    anyhow::Ok(())
}

/// Exports equity series and drawdown data to a CSV file for analysis and plotting.
/// 
/// This function creates a comprehensive CSV file containing the equity curve data
/// along with drawdown information for performance analysis. The output file includes
/// timestamps, capital values, absolute drawdown amounts, and percentage drawdowns.
/// 
/// The file is saved to `{exit_results_path}/equity_series.csv` where `exit_results_path`
/// is taken from the provided `strategy_settings`.
/// 
/// # Arguments
/// let equity_series = vec![(datetime1, 10000.0), (datetime2, 9900.0), (datetime3, 9950.0)];
/// export_equity_drawdowns_to_csv(&drawdowns, &drawdowns_pct, &equity_series, &strategy_settings)?;
/// // Creates: {exit_results_path}/equity_series.csv
/// ```
pub fn export_equity_drawdowns_to_csv(
    drawdowns: &Vec<f64>,
    drawdowns_pct: &Vec<f64>,
    equity_series: &[(chrono::DateTime<chrono::Utc>, f64)],
    strategy_settings: &settings::StrategySettings,
) -> anyhow::Result<()> {
    let path = format!("{}/equity_series.csv", strategy_settings.exit_results_path);

    let mut file = std::fs::File::create(path)?;
    writeln!(file, "datetime;capital;drawdown;drawdown_pct")?;
    for ((datetime_capital, drawdown), drawdown_pct) in 
        equity_series.iter().zip(drawdowns.iter()).zip(drawdowns_pct.iter())
    {
        let (datetime, capital) = datetime_capital;
        writeln!(
            file,
            "{};{};{};{}",
            datetime.format("%Y-%m-%d %H:%M:%S"),
            capital,
            drawdown,
            drawdown_pct
        )?;
    }

    anyhow::Ok(())
}

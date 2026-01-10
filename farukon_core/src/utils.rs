// farukon_core/src/utils.rs

//! Utility functions for the Farukon platform.
//! Includes helper functions for parsing settings, calculating quantities, and validating data.

use std::io::Write;
use anyhow::Context;
use rand::prelude::*;
use rand_distr::Distribution;

use crate::optimization::ParameterSet;
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
/// Creates: {exit_results_path}/equity_series.csv
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

/// Generates a vector of `size` `f64` values using Latin Hypercube Sampling (LHS) within a specified range.
///
/// This function divides the range `[min, max]` into `size` equal intervals and samples one value
/// uniformly from each interval. If a `step` is provided, the sampled value is snapped to the
/// nearest multiple of `step` within the `[min, max]` bounds. This ensures good coverage of the
/// parameter space while allowing for discrete steps.
///
/// # Arguments
/// * `min` - The lower bound of the range.
/// * `max` - The upper bound of the range.
/// * `step` - Optional step size to discretize the sampled values.
/// * `size` - The number of values to generate.
/// * `rng` - A mutable reference to a random number generator implementing `rand::Rng`.
///
/// # Returns
/// A `Vec<f64>` containing the generated values, shuffled to randomize order.
fn generate_lhs_values_f64(
    min: f64,
    max: f64,
    step: Option<f64>,
    size: usize,
    mut rng: &mut impl rand::Rng
) -> Vec<f64> {
    let mut values = Vec::with_capacity(size);
    // Calculate the width of each interval for LHS
    let interval_size = (max - min) / size as f64;

    // Sample one value from each interval
    for i in 0..size {
        let interval_start = min + (i as f64) * interval_size;
        let interval_end = min + ((i + 1) as f64) * interval_size;

        // Sample uniformly within the specific interval
        let value = rand_distr::Uniform::new(interval_start, interval_end).sample(&mut rng);

        // If a step is defined, snap the value to the nearest step increment within bounds
        let processed_value = if let Some(step_val) = step {
            let clamped_value = value.clamp(min, max);
            let rounded_index = ((clamped_value - min) / step_val).round();
            rounded_index * step_val + min
        } else {
            // Otherwise, use the sampled value directly
            value
        };

        values.push(processed_value);
    }

    // Shuffle the resulting vector to remove any systematic ordering from interval sampling
    values.shuffle(&mut rng);
    values
}

/// Generates a vector of `size` `f64` values based on a `ParamSpec`.
///
/// This function handles both `ParamSpec::Range` and `ParamSpec::Discrete`.
/// For `Range`, it delegates to `generate_lhs_values_f64` for Latin Hypercube Sampling.
/// For `Discrete`, it cycles through the provided discrete values and shuffles the result.
///
/// # Arguments
/// * `spec` - A reference to the `settings::ParamSpec` defining the parameter space.
/// * `size` - The number of values to generate.
/// * `rng` - A mutable reference to a random number generator implementing `rand::Rng`.
///
/// # Returns
/// A `Vec<f64>` containing the generated values.
fn generate_param_values(
    spec: &settings::ParamSpec,
    size: usize,
    rng: &mut impl rand::Rng
) -> Vec<f64> {
    match spec {
        // If the spec defines a continuous range, use LHS
        settings::ParamSpec::Range { start, end, step } => {
            generate_lhs_values_f64(*start, *end, Some(*step), size, rng)
        }
        // If the spec defines a discrete list of values
        settings::ParamSpec::Discrete(values) => {
            let mut result = Vec::with_capacity(size);
            let num_discrete_vals = values.len();

            // If the discrete list is empty, return an empty vector
            if num_discrete_vals == 0 {
                return vec![0.0; size];
           }

           // Create a list of indices, cycling through the discrete values to fill 'size' slots
            let indices: Vec<usize> = (0..size).map(|i| i % num_discrete_vals).collect();
            let mut shuffled_indices = indices;
            // Shuffle the indices to randomize which discrete value goes where
            shuffled_indices.shuffle(rng);

            // Map the shuffled indices back to the actual discrete values
            for idx in shuffled_indices {
                if let Some(json_val) = values.get(idx) {
                    // Attempt to extract an f64 value from the serde_json::Value
                    if let Some(f64_val) = json_val.as_f64() {
                        result.push(f64_val);
                    } else {
                        // Log a warning if the value isn't a number and push a default (0.0)
                        eprintln!("Warning: Discrete value {:?} is not a number, skipping.", json_val);
                        result.push(0.0);   // Consider returning Result or handling error differently
                    }
                }
            }

            result
        }
    }
}

/// Generates a vector of `ParameterSet` objects using Latin Hypercube Sampling (LHS) for specified parameter ranges.
///
/// This function creates a diverse initial population or fresh blood by sampling parameter values
/// according to their specifications (either continuous `Range` or discrete `Discrete` lists).
/// It ensures that the generated sets cover the defined parameter space effectively.
///
/// # Arguments
/// * `strategy_params_ranges` - A map of strategy parameter names to their `ParamSpec`.
/// * `pos_sizer_value_range` - The `ParamSpec` for the position sizer value.
/// * `slippage_range` - The `ParamSpec` for the slippage value.
/// * `pos_sizer_additional_params` - A map of additional position sizer parameter names to their `ParamSpec`.
/// * `pos_sizer_name` - The name of the position sizing method.
/// * `size` - The number of `ParameterSet` objects to generate.
/// * `rng` - A mutable reference to a random number generator implementing `rand::Rng`.
///
/// # Returns
/// A `Vec<optimization::ParameterSet>` containing the generated parameter sets.
pub fn generate_lhs_parameter_sets(
    strategy_params_ranges: &std::collections::HashMap<String, settings::ParamSpec>,
    pos_sizer_value_range: &settings::ParamSpec,
    slippage_range: &settings::ParamSpec,
    pos_sizer_additional_params: &std::collections::HashMap<String, settings::ParamSpec>,
    pos_sizer_name: &String,
    size: usize,
    rng: &mut impl rand::Rng
) -> Vec<optimization::ParameterSet> {
    let mut result = Vec::with_capacity(size);

    // Generate LHS/dischcrete values for each strategy parameter across 'size' sets
    let strategy_param_values: std::collections::HashMap<String, Vec<f64>> = strategy_params_ranges
        .iter()
        .map(|(name, spec)| {
            let values = generate_param_values(spec, size, rng);
            (name.clone(), values)
        })
        .collect();

    // Generate LHS/discrete values for pos_sizer_value across 'size' sets
    let pos_sizer_values = if pos_sizer_name == "1" {
        vec![0.0; size]
    } else {
        generate_param_values(pos_sizer_value_range, size, rng)
    };

    // Generate LHS/discrete values for slippage across 'size' sets
    let slippage_values = generate_param_values(slippage_range, size, rng);
    // Generate LHS/discrete values for each additional pos sizer parameter across 'size' sets
    let pos_sizer_additional_values: std::collections::HashMap<String, Vec<f64>> = pos_sizer_additional_params
        .iter()
        .map(|(name, spec)| {
            let values = generate_param_values(spec, size, rng);
            (name.clone(), values)
        })
        .collect();

    // Construct 'size' ParameterSet objects by combining values from each parameter map
    for i in 0..size {
        // Build the strategy_params vector for the i-th ParameterSet
        let strategy_params: Vec<(String, serde_json::Value)> = strategy_param_values
            .iter()
            .map(|(name, values)| {
                // Get the i-th value for this parameter, defaulting to 0.0 if index is somehow out of bounds
                let value = values.get(i).copied().unwrap_or(0.0);
                (name.clone(), serde_json::Value::Number(serde_json::Number::from_f64(value).unwrap()))
            })
            .collect();

        // Build the pos_sizer_additional_params vector for the i-th ParameterSet
        let pos_sizer_additional_params: Vec<(String, serde_json::Value)> = pos_sizer_additional_values
        .iter()
        .map(|(name, values)| {
            // Get the i-th value for this parameter, defaulting to 0.0 if index is out of bounds
            let value = values.get(i).copied().unwrap_or(0.0);
            (name.clone(), serde_json::Value::Number(serde_json::Number::from_f64(value).unwrap()))
        })
        .collect();

        // Create the ParameterSet for index i
        let param_set = ParameterSet::new()
            .with_strategy_params(strategy_params)
            .with_pos_sizer_name(pos_sizer_name.clone())
            .with_pos_sizer_additional_params(pos_sizer_additional_params)
            .with_pos_sizer_value(pos_sizer_values[i])
            .with_slippage(slippage_values[i]);

        result.push(param_set);
    }

    result
}

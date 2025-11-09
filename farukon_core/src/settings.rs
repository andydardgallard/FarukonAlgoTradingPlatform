// farukon_core/src/settings.rs

//! Configuration structures for the Farukon platform.
//! Loads settings from JSON files and validates them.

use std::fs;

use serde::Deserialize;

use crate::commission_plans;
use crate::instruments_info;

/// Type of optimizer to use.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OptimizerType {
    #[serde(rename = "Grid_Search")]
    GridSearch,
    #[serde(rename = "Genetic")]
    Genetic { ga_params: GAParams },
}

/// Type of fitness metric to optimize.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub enum FitnessValue {
    #[serde(rename = "Total_Return")]
    TotalReturn,
    #[serde(rename = "Total_Return_%")]
    TotalReturnPercent,
    #[serde(rename = "APR")]
    APR,
    #[serde(rename = "Max_Drawdown")]
    MaxDD,
    #[serde(rename = "APR/DD_factor")]
    AprDDFactor,
    #[serde(rename = "Recovery_Factor")]
    RecoveryFactor,
    #[serde(rename = "Deals_Count")]
    DealsCount,
    #[serde(rename = "Composite")]
    Composite { metrics: Vec<String> },
}

impl Default for FitnessValue {
    fn default() -> Self {
        FitnessValue::AprDDFactor
    }
}

/// Parameters for the Genetic Algorithm optimizer.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct GAParams {
    pub population_size: usize,
    pub p_crossover: f64,
    pub p_mutation: f64,
    pub max_generations: usize,
    pub fitness_params: FitnessParams,
}

/// Fitness function parameters for the Genetic Algorithm.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct FitnessParams {
    pub fitness_direction: String,
    pub fitness_value: FitnessValue,
}

#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct MarginParams {
    pub min_margin: f64,
    pub margin_call_type: String,
}

/// Position sizing parameters.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct PosSizer {
    pub pos_sizer_name: String,

    #[serde(deserialize_with = "deserialize_strategy_params")]
    pub pos_sizer_params: std::collections::HashMap<String, Vec<serde_json::Value>>,

    #[serde(deserialize_with = "deserialize_float_range")]
    pub pos_sizer_value: Vec<f64>,
}

/// Data settings for a strategy.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DataSettings {
    pub data_path: String,
    pub timeframe: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum KellyMode {
    #[serde(rename = "on")]
    On,
    #[serde(rename = "off")]
    Off,
}

/// Mode for calculating performance metrics.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MetricsMode {
    #[serde(rename = "offline")]
    Offline,
    #[serde(rename = "realtime")]
    RealTime { modified_kelly_creterion: KellyMode },
}

/// Portfolio settings for a strategy.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortfolioSettingsForStrategy {
    pub metrics_calculation_mode: MetricsMode,
}

/// Settings for a single strategy.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct StrategySettings {
    pub threads: Option<usize>,
    pub strategy_name: String,
    pub strategy_path: String,
    pub exit_results_path: String,
    pub strategy_weight: f64,
    
    #[serde(deserialize_with = "deserialize_float_range")]
    pub slippage: Vec<f64>,

    pub data: DataSettings,
    pub symbol_base_name: String,
    pub symbols: Vec<String>,

    #[serde(deserialize_with = "deserialize_strategy_params")]
    pub strategy_params: std::collections::HashMap<String, Vec<serde_json::Value>>,

    pub pos_sizer_params: PosSizer,
    pub margin_params: MarginParams,
    pub portfolio_settings_for_strategy: PortfolioSettingsForStrategy,
    pub optimizer_type: OptimizerType,
    #[serde(skip_deserializing)]
    pub commission_plans: Option<commission_plans::CommissionPlans>
}

/// Common settings applicable to the entire platform.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CommonSettings {
    pub mode: String,
    pub initial_capital: f64,
}

/// Top-level settings structure.
#[derive(serde::Deserialize, serde::Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub common: CommonSettings,
    pub portfolio: std::collections::HashMap<String, StrategySettings>,
}

impl Settings {
    /// Loads settings from a JSON file.
    /// # Arguments
    /// * `config_path` - Path to the JSON configuration file.
    /// * `instruments_info` - Instrument metadata registry.
    /// # Returns
    /// * `anyhow::Result<Settings>` containing the loaded settings.
    pub fn load<P: AsRef<std::path::Path>>(
        settings_file_path: P,
        instruments_info: &instruments_info::InstrumentsInfoRegistry,
    ) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(settings_file_path)?;
        let mut settings: Settings = serde_json::from_str(&contents)
            .map_err(|e| anyhow::anyhow!("Failed to parse settings JSON: {}", e))?;

        let commission_plans = commission_plans::CommissionPlans::load()?;

        check_args(&mut settings, &commission_plans, &instruments_info)
            .map_err(|e| anyhow::anyhow!("Settings validation failed:\n{}", e))?;

        anyhow::Ok(settings)
    }

}

fn check_args(
    settings: &mut Settings,
    commission_plans: &commission_plans::CommissionPlans,
    instruments_info: &instruments_info::InstrumentsInfoRegistry,
) -> anyhow::Result<()> {
    // check threads
    {
        let strategies_ids: Vec<String> = settings.portfolio.keys().cloned().collect();
        for strategy_id in strategies_ids {
            let strategy_settings: &mut StrategySettings = settings.portfolio.get_mut(&strategy_id).unwrap();
            match strategy_settings.threads {
                Some(threads) => {
                    if threads == 0 {
                        anyhow::bail!("Settings validation error: 'threads' cannot be zero.");
                    } else if threads > 0 {
                        let availiable_threads = num_cpus::get();
                        if threads > availiable_threads {
                            strategy_settings.threads = Some(availiable_threads);
                        }
                    }
                },
                None => {}
            }
        }
    }

    // check mode
    {
        const VALID_MODES: &[&str] = &["Debug", "Optimize", "Visual"];
        if !VALID_MODES.contains(&settings.common.mode.as_str()) {
            anyhow::bail!("Wrong mode setting! Use one of {:?}", VALID_MODES);
        }
    }

    // check portfolio
    {
        let strategies = settings.portfolio.keys();
        for strategy in strategies {
            let strategy_settings = settings.portfolio.get(strategy).unwrap();

            // check optimizer type
            {
                match &strategy_settings.optimizer_type {
                    OptimizerType::Genetic { ga_params }=> {
                        if ga_params.population_size == 0 {
                            anyhow::bail!("GA population_size must be greater than 0");
                        }
                        if ga_params.p_crossover < 0.0 || ga_params.p_crossover > 1.0 {
                            anyhow::bail!("GA p_crossover must be between 0.0 and 1.0");
                        }
                        if ga_params.p_mutation < 0.0 || ga_params.p_mutation > 1.0 {
                            anyhow::bail!("GA p_mutation must be between 0.0 and 1.0");
                        }
                        if ga_params.max_generations == 0 {
                            anyhow::bail!("GA max_generations must be greater than 0");
                        }

                        // check fitness_direction
                        let dir_str = &ga_params.fitness_params.fitness_direction;
                        if dir_str != "max" && dir_str != "min" {
                            anyhow::bail!("fitness_direction must be 'max' or 'min'");
                        }

                        // check fitness_value
                        match &ga_params.fitness_params.fitness_value {
                            FitnessValue::Composite { metrics } => {
                                const VALID_COMPOSITE_METRICS: &[&str] = &[
                                    "Total_Return",
                                    "Total_Return_%",
                                    "APR",
                                    "max_DD", 
                                    "max_DD_%",
                                    "APR/DD_factor",
                                    "Recovery_Factor",
                                    "Recovery_Factor_%",
                                    "Deals_Count",
                                ];

                                if metrics.is_empty() {
                                    anyhow::bail!(
                                        "Composite fitness must have at least one metric. One of {:?}",
                                        VALID_COMPOSITE_METRICS
                                    );
                                }

                                for metric in metrics {
                                    if !VALID_COMPOSITE_METRICS.contains(&metric.as_str()) {
                                        anyhow::bail!(
                                            "Invalid composite metric '{}'. Must be one of: {:?}",
                                            metric,
                                            VALID_COMPOSITE_METRICS
                                        );
                                    }
                                }
                            },
                            _ => {}
                        }
                    },
                    OptimizerType::GridSearch => {}
                }
            }

            // check timeframe
            {
                const VALID_TIMEFRAMES: &[&str] = &["1min", "2min", "3min", "4min", "5min", "1d"];
                if !VALID_TIMEFRAMES.contains(&strategy_settings.data.timeframe.as_str()) {
                    anyhow::bail!("Wrong mode setting! Use one of {:?}", VALID_TIMEFRAMES);
                }
            }

            // check symbols
            {
                if strategy_settings.symbols.is_empty() {
                    anyhow::bail!("Provide symbol list!")
                }
            }

            // check pos sizers
            {
                const VALID_POS_SIZERS: &[&str] = &["mpr", "poe", "fixed_ratio", "1"];
                if !VALID_POS_SIZERS.contains(&strategy_settings.pos_sizer_params.pos_sizer_name.as_str()) {
                    anyhow::bail!("Wrong pos sizer. Use one of: {:#?}", VALID_POS_SIZERS);
                }

                if &strategy_settings.pos_sizer_params.pos_sizer_name == "1" {
                    if !strategy_settings.pos_sizer_params.pos_sizer_value.is_empty() {
                        anyhow::bail!("Pos sizer value vector of plain '1' param must be empty!");
                    }
                } else {
                    if strategy_settings.pos_sizer_params.pos_sizer_value.is_empty() {
                        anyhow::bail!("Pos sizer value vector cannot be empty!");
                    }
                }
                
                for &pos_val in &strategy_settings.pos_sizer_params.pos_sizer_value {
                    if pos_val <= 0.0 {
                        anyhow::bail!("Pos sizer values must be positive!");
                    }
                }
            }

            // check slippage vector
            {
                if strategy_settings.slippage.is_empty() {
                    anyhow::bail!("Slippage vector cannot be empty!");
                }

                for &pos_val in &strategy_settings.pos_sizer_params.pos_sizer_value {
                    if pos_val < 0.0 {
                        anyhow::bail!("Pos sizer values must be positive!")
                    }
                }
            }

            // check strategy params range
            {
                for (param_name, values) in &strategy_settings.strategy_params {
                    if values.is_empty() {
                        anyhow::bail!("Strategy parameter '{}' range cannot be empty!", param_name);
                    }

                    for value in values {
                        if let Some(num) = value.as_f64() {
                            if num <= 0.0 {
                                anyhow::bail!("Strategy parameter '{}' must have positive values!", param_name);
                            }
                        } else if let Some(num) = value.as_i64() {
                            if num <= 0 {
                                anyhow::bail!("Strategy parameter '{}' must have positive values!", param_name);
                            }
                        }
                    }
                }
            }

            // check exit path of results
            {
                let normalized_path = strategy_settings.exit_results_path
                    .trim_end_matches('/');

                if normalized_path.is_empty() {
                    anyhow::bail!("Exit path cannot be empty!")
                }

                let path = std::path::Path::new(normalized_path);
                if !path.exists() {
                    fs::create_dir_all(path)?;
                } else if !path.is_dir() {
                    anyhow::bail!("Exit path is not a directory!")
                }
            }
        }
    }

    // add commission plans
    {
        for strategy_settings in settings.portfolio.values_mut() {
            let mut required_combinations = std::collections::HashSet::new();

            for symbol in &strategy_settings.symbols {
                if let Some(instruments_info) = instruments_info.get_instrument_info(symbol) {
                    required_combinations.insert((instruments_info.exchange.clone(), instruments_info.commission_type.clone()));
                } else {
                    anyhow::bail!("Instrument info not found for symbol '{}'", symbol);
                }
            }

            let mut filtered_exchanges: std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>> = std::collections::HashMap::new();

            for (exchange, commission_type) in required_combinations {
                if let Some(exchange_plans) = commission_plans.exchanges.get(&exchange) {
                    let filtered_plan_map = filtered_exchanges
                        .entry(exchange.clone())
                        .or_insert_with(|| std::collections::HashMap::new());
                    
                    for (plan_name, plan_value) in exchange_plans {
                        if let Some(obj) = plan_value.as_object() {
                            if let Some(amount) = obj.get(&commission_type) {
                                if let Some(_) = amount.as_f64() {
                                    let plan_entry = filtered_plan_map
                                        .entry(plan_name.clone())
                                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));

                                    if let serde_json::Value::Object(plan_obj) = plan_entry {
                                        plan_obj.insert(commission_type.clone(), amount.clone());
                                    }
                                }
                            }
                        }
                        else if let Some(_amount) = plan_value.as_f64() {
                            // TO DO
                        }
                    }
                }
            }

            let filtered_commission_plans = commission_plans::CommissionPlans {
                exchanges: filtered_exchanges,
            };

            strategy_settings.commission_plans = Some(filtered_commission_plans);
        }
    }

    anyhow::Ok(())
}

// --- Deserialization Helpers ---

/// Deserializes strategy parameters from JSON.
fn deserialize_strategy_params<'de, D>(deserializer: D) -> Result<std::collections::HashMap<String, Vec<serde_json::Value>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum ParamValue {
        Descrete(Vec<serde_json::Value>),
        Range { start: f64, end: f64, step: f64 },
    }

    let raw_map: std::collections::HashMap<String, ParamValue> = std::collections::HashMap::deserialize(deserializer)?;
    let mut result = std::collections::HashMap::new();

    for (key, value) in raw_map {
        let values = match value {
            ParamValue::Descrete(vec) => vec,
            ParamValue::Range { start, end, step } => {
                let mut values = Vec::new();
                let mut current = start;

                while current <= end + std::f64::EPSILON {
                    values.push(serde_json::Value::from(current));
                    current += step;
                }
                values
            }
        };
        result.insert(key, values);
    }

    Ok(result)
}

/// Deserializes a `serde_json::Value` that can represent either a discrete array of floats or a range object into a vector of floats.
/// The range object is expected to have fields `start`, `end`, and `step`.
/// This function is typically used for deserializing parameter ranges in optimization settings.
///
/// # Arguments
/// * `deserializer` - The serde deserializer.
///
/// # Returns
/// * `Result<Vec<f64>, D::Error>` - A vector of floats representing either the discrete values or the generated range.
fn deserialize_float_range<'de, D>(deserializer: D) -> Result<Vec<f64>, D::Error>
where 
    D: serde::Deserializer<'de>,
{
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum FloatRange {
        Discrete(Vec<f64>),
        Range { start: f64, end: f64, step: f64 },
    }

    let value = FloatRange::deserialize(deserializer)?;

    match value {
        FloatRange::Discrete(vec) => Ok(vec),
        FloatRange::Range { start, end, step } => {
            let mut values = Vec::new();
            let mut current = start;

            while current <= end + std::f64::EPSILON {
                values.push(current);
                current += step;
            }

            Ok(values)
        }
    }
}

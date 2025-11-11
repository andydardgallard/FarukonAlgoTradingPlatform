// farukon_core/src/commision_plans.rs

//! Manages commission fee structure per exchange and instrument type.
//! Loads commission_plans.json and calculates commissions for FORTS futures.
//!
//! The commission structure is defined in a JSON file (`commission_plans.json`) with the following format:
//! {
//!   "FORTS": {
//!     "currency": 0.5,
//!     "index": 1.0,
//!     "percent": 0.01
//!   }
//! }
//!
//! This module provides functions to load this data and calculate the commission amount for a given trade.

use crate::settings;
use crate::instruments_info;

/// Represents the commission fee structure for all exchanges.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommissionPlans {
    /// A map of exchange names to their commission plans.
    pub exchanges: std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
}

impl CommissionPlans {
    /// Loads the commission plan from the `commission_plans.json` file.
    /// # Returns
    /// * `anyhow::Result<CommissionPlans>` containing the loaded commission structure.
    pub fn load(
        settings: &mut settings::Settings,
        instruments_info: &instruments_info::InstrumentsInfoRegistry,
    ) -> anyhow::Result<Self> {
        // Loads commission structure from commission_plans.json.
        let file_path = &settings.common.commission_plans_path;
        let contents = std::fs::read_to_string(file_path)?;
        // Deserialize the JSON content into a CommissionPlans struct.
        let plans: Self = serde_json::from_str(&contents)?;

        Self::add_commission_plans_to_settings(&plans, settings, instruments_info)?;

        anyhow::Ok(plans)
    }

    /// Filters the global commission plans based on the exchanges and instrument types required by each strategy,
    /// and attaches the filtered plans to the respective strategy settings.
    /// This ensures that each strategy only carries the commission information relevant to its traded instruments,
    /// optimizing memory usage and access speed during backtesting.
    ///
    /// # Arguments
    /// * `self` - A reference to the global `CommissionPlans` loaded from `commission_plans.json`.
    /// * `settings` - A mutable reference to the main `Settings` object, which contains the map of all strategy configurations.
    ///                The function modifies the `commission_plans` field within each strategy's settings.
    /// * `instruments_info` - A reference to the `InstrumentsInfoRegistry` containing metadata for all known instruments.
    ///                       Used to determine the exchange and commission type for each symbol traded by a strategy.
    ///
    /// # Returns
    /// * `anyhow::Result<()>` - `Ok(())` if the filtering and attachment process completes successfully.
    ///                          Returns an `Err` if an instrument's info is missing for a symbol listed in a strategy's settings.
    fn add_commission_plans_to_settings(
        &self, // Reference to the loaded global commission plans
        settings: &mut settings::Settings, // Mutable reference to the main settings, to be updated
        instruments_info: &instruments_info::InstrumentsInfoRegistry, // Reference to the instrument metadata registry
    ) -> anyhow::Result<()> {
        // Iterate over each strategy configuration within the portfolio settings map.
        for strategy_settings in settings.portfolio.values_mut() {
            // --- Phase 1: Determine Required Commission Combinations ---
            // Identify the unique (Exchange, CommissionType) pairs needed for this specific strategy.
            // This is based on the symbols the strategy intends to trade.
            let mut required_combinations = std::collections::HashSet::new();

            for symbol in &strategy_settings.symbols {
                // Retrieve the instrument info for the current symbol.
                if let Some(instruments_info) = instruments_info.get_instrument_info(symbol) {
                    // Insert the (exchange, commission_type) pair into the set to ensure uniqueness.
                    required_combinations.insert((
                        instruments_info.exchange.clone(), // e.g., "FORTS"
                        instruments_info.commission_type.clone() // e.g., "currency", "index"
                    ));
                } else {
                    // If instrument info is missing, it's a critical error for the backtest configuration.
                    anyhow::bail!("Instrument info not found for symbol '{}'", symbol);
                }
            }

            // --- Phase 2: Filter Global Plans Based on Requirements ---
            // Construct a new, smaller `CommissionPlans` object containing only the necessary data for this strategy.
            let mut filtered_exchanges: std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>> = std::collections::HashMap::new();

            // Iterate through the unique (Exchange, CommissionType) pairs identified for this strategy.
            for (exchange, commission_type) in required_combinations {
                // Attempt to find the commission plans for the required exchange in the global plans.
                if let Some(exchange_plans) = self.exchanges.get(&exchange) {
                    // Get or create a map for this specific exchange in the filtered plans.
                    let filtered_plan_map = filtered_exchanges
                        .entry(exchange.clone()) // Use the exchange name as the key
                        .or_insert_with(|| std::collections::HashMap::new()); // Initialize an empty map if the exchange wasn't present

                    // Iterate through all available commission plans for this exchange in the global plans.
                    for (plan_name, plan_value) in exchange_plans {
                        // Check if the plan value is an object (like {"currency": 0.5, "index": 1.0})
                        if let Some(obj) = plan_value.as_object() {
                            // Check if this plan object contains the specific commission type required by the strategy.
                            if let Some(amount) = obj.get(&commission_type) {
                                // Verify that the commission amount is a floating-point number.
                                if let Some(_) = amount.as_f64() {
                                    // Get or create an entry for this specific plan name within the exchange's map in the filtered plans.
                                    let plan_entry = filtered_plan_map
                                        .entry(plan_name.clone()) // Use the plan name (e.g., "default") as the key
                                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new())); // Initialize an empty object if the plan wasn't present

                                    // If the entry is indeed an object (which it should be, due to `or_insert_with` above),
                                    // insert the required commission type and its value into this plan's object.
                                    if let serde_json::Value::Object(plan_obj) = plan_entry {
                                        plan_obj.insert(commission_type.clone(), amount.clone()); // Add "currency": 0.5 or similar
                                    }
                                }
                            }
                        }
                        // Check if the plan value is a single floating-point number (alternative format).
                        // This branch is currently marked with a "TO DO", suggesting incomplete logic or handling.
                        else if let Some(_amount) = plan_value.as_f64() {
                            // TO DO: Potentially handle this plan format if applicable.
                            // Example: If a plan is just {"default": 0.75}, meaning 0.75 for all types under this plan.
                            // This would require a different insertion logic into filtered_plan_map.
                        }
                    }
                }
                // If the required exchange is not found in the global plans, the filtered map for this exchange will remain empty.
                // The strategy will later handle this gracefully during commission calculation.
            }

            // --- Phase 3: Attach Filtered Plans to Strategy Settings ---
            // Create the final `CommissionPlans` struct containing only the data relevant to this strategy.
            let filtered_commission_plans = Self {
                exchanges: filtered_exchanges, // The map populated in Phase 2
            };

            // Assign the filtered commission plans to the current strategy's settings.
            // This replaces the potentially large global commission plan structure with a much smaller, strategy-specific one.
            strategy_settings.commission_plans = Some(filtered_commission_plans);
        }

        // Indicate successful completion of the filtering and attachment process.
        anyhow::Ok(())
    }


    /// Retrieves the commission rate for a specific exchange, instrument type, and plan name.
    /// # Arguments
    /// * `exchange` - The name of the exchange (e.g., "FORTS").
    /// * `instrument_type` - The type of instrument (e.g., "futures", "equity").
    /// * `plan_name` - The name of the commission plan (e.g., "default", "premium").
    /// # Returns
    /// * An optional `f64` representing the commission rate, or `None` if not found.
    pub fn get_commission(
        &self,
        exchange: &str,
        instrument_type: &str,
        plan_name: &str,
    ) -> Option<f64> {
        // Retrieves commission rate from plan.
        let exchange_map = self.exchanges.get(exchange)?;
        // Get the commission value for the specified plan and instrument type.
        let plan_value = exchange_map.get(plan_name)?;

        // If the plan value is an object, look up the commission rate by instrument type.
        if let Some(obj) = plan_value.as_object() {
            if let Some(amount) = obj.get(instrument_type) {
                if let Some(value) = amount.as_f64() {
                    return Some(value);
                }
            }
        } else if let Some(value) = plan_value.as_f64() {
            // If the plan value is a single number, use it as the commission rate.
            return Some(value);
        }

        None
    }

    /// Generic getter for any plan field.
    /// # Arguments
    /// * `exchange` - The name of the exchange.
    /// * `plan_name` - The name of the commission plan.
    /// * `key` - The key of the field to retrieve.
    /// # Returns
    /// * An optional `serde_json::Value` representing the field value, or `None` if not found.
    pub fn get_plan_value (
        &self,
        exchange: &str,
        plan_name: &str,
        key: &str,
    ) -> Option<serde_json::Value> {
        // Generic getter for any plan field.

        // Get the map of commission plans for the specified exchange.
        let exchange_map = self.exchanges.get(exchange)?;
        // Get the commission value for the specified plan.
        let plan_value = exchange_map.get(plan_name)?;
        // Return the value for the specified key.
        plan_value.get(key).cloned()
    }

}

/// Calculates the commission for a FORTS futures trade based on the step price, step, and commission plan.
/// # Arguments
/// * `price` - The execution price of the trade.
/// * `strategy_instruments_info_for_symbol` - The instrument metadata for the traded symbol.
/// * `strategy_settings` - The strategy settings, which include the commission plan.
/// # Returns
/// * An optional `f64` representing the commission amount, or `None` if no commission can be calculated.
pub fn calculate_forts_comission(
    price: Option<f64>,
    strategy_instruments_info_for_symbol: &instruments_info::InstrumentInfo,
    strategy_settings: &settings::StrategySettings,
) -> Option<f64> {
    // Calculates commission for FORTS futures based on step_price and step.
    // Uses commission_plans.json to determine rate per instrument type.
    
    // Get the exchange name from the instrument metadata.
    let exchange = &strategy_instruments_info_for_symbol.exchange;
    // Get the step price and step from the instrument metadata.
    let step_price = strategy_instruments_info_for_symbol.step_price;
    let step = strategy_instruments_info_for_symbol.step;
    // Get the commission type from the instrument metadata.
    let commission_type = strategy_instruments_info_for_symbol.commission_type.clone();
    
    // Get the commission plans from the strategy settings.
    let commission_plans_map = strategy_settings.commission_plans
        .as_ref()
        .and_then(|cp| cp.exchanges.get(exchange));

    if let Some(plans) = commission_plans_map {
        // Calculate the total commission rate by summing up all commission values for the specified commission type.
        let mut total_commission_rate = 0.0;
        for (_plan_name, plan_value) in plans {
            if let Some(obj) = plan_value.as_object() {
                if let Some(currency_val) = obj.get(&commission_type) {
                    if let Some(amount) = currency_val.as_f64() {
                        total_commission_rate += amount / 100.0;
                    }
                }
            }
        }

        // If a valid commission rate was found, calculate the commission amount.
        if total_commission_rate > 0.0 {
            // Calculate the cost of one step price in base currency.
            let cost_of_step_price = ((step_price / step) * 100_000.0).round() / 100_000.0;
            // Calculate the commission base (price * cost_of_step_price).
            let commission_base = (price.unwrap().abs() * cost_of_step_price * 100.0).round() / 100.0;
            // Calculate the final commission amount.
            let commission = (commission_base * total_commission_rate * 100.0).round() / 100.0;

            return Some(commission);
        }
    }
    None

}

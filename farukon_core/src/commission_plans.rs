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
    pub fn load() -> anyhow::Result<Self> {
        // Loads commission structure from commission_plans.json.
        let file_path = "commission_plans.json";
        let contents = std::fs::read_to_string(file_path)?;
        // Deserialize the JSON content into a CommissionPlans struct.
        let plans: Self = serde_json::from_str(&contents)?;

        anyhow::Ok(plans)
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

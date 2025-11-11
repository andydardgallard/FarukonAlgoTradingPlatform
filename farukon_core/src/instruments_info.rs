// farukon_core/src/instruments_info.rs

//! Instrument metadata registry.
//! Loads instruments_info.json and validates all contracts.
//!
//! The instrument metadata includes information such as exchange, type, margin, commission type, and step price.
//! This information is used by the portfolio and execution modules to calculate positions, commissions, and risk.

use anyhow::Context;

use crate::settings;

/// Represents the metadata for a single instrument.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InstrumentInfo {
    /// The exchange where the instrument is traded (e.g., "FORTS").
    pub exchange: String,
    /// The type of instrument (e.g., "futures", "equity", "reversal_futures").
    #[serde(rename = "type")]
    pub instrument_type: String,
    /// The precision of the contract (number of decimal places).
    pub contract_precision: usize,
    /// The initial margin required to trade one contract.
    pub margin: f64,
    /// The type of commission charged ("currency", "index", "percent").
    pub commission_type: String,
    /// The date and time when trading for this contract begins.
    pub trade_from_date: String,
    /// The date and time when trading for this contract ends.
    pub expiration_date: String,
    /// The marginal costs associated with trading this contract.
    pub marginal_costs: f64,
    /// The step size for the contract (e.g., 1 for Si, 10 for RTS).
    pub step: f64,
    /// The price of one step (e.g., 1.0 for Si, 14.73394 for RTS).
    pub step_price: f64,
}

impl InstrumentInfo {
    /// Validates the instrument metadata.
    /// # Arguments
    /// * `contract_name` - The name of the contract (e.g., "Si-12.23").
    /// # Returns
    /// * `anyhow::Result<()>` indicating success or failure.
    pub fn validate(&self, contract_name: &str) -> anyhow::Result<()> {
        // Validates all fields for correctness and consistency.
        // Prevents invalid configurations from crashing backtest.
        // Called during InstrumentsInfoRegistry::load()

        // Validate instrument_type
        {
            const VALID_INSTRUMENT_TYPES: &[&str] = &["futures", "equity", "reversal_futures"];
            if !VALID_INSTRUMENT_TYPES.contains(&self.instrument_type.as_str()) {
                anyhow::bail!(
                    "Validation error for contract '{}': Invalid 'type' '{}'. Valid types are: {:?}",
                    contract_name,
                    self.instrument_type,
                    VALID_INSTRUMENT_TYPES,
                );
            }
        }

        // Validate margin
        {
            if self.margin <= 0.0 {
                anyhow::bail!(
                    "Validation error for contract '{}': 'margin' must be positive, got {}",
                    contract_name,
                    self.margin,
                )
            }
        }

        // Validate commission_type
        {
            const VALID_COMMISSION_TYPES: &[&str] = &["currency", "index", "percent"];
            if !VALID_COMMISSION_TYPES.contains(&self.commission_type.as_str()) {
                anyhow::bail!(
                    "Validation error for contract '{}': Invalid 'commission_type' '{}'. Valid types are: {:?}",
                    contract_name,
                    self.commission_type,
                    VALID_COMMISSION_TYPES,
                );
            }
        }

        // Validate trade_from_date
        {
            chrono::NaiveDateTime::parse_from_str(
                &self.trade_from_date,
                "%Y-%m-%d %H:%M:%S",
            ).with_context(|| format!(
                "Invalid 'trade_from_date' format '{}' for contract '{}'",
                self.trade_from_date,
                contract_name,
            ))?;
        }

        // Validate expiration_date
        {
            chrono::NaiveDateTime::parse_from_str(
                &self.expiration_date,
                "%Y-%m-%d %H:%M:%S",
            ).with_context(|| format!(
                "Invalid 'expiration_date' format '{}' for contract '{}'",
                self.expiration_date,
                contract_name,
            ))?;
        }

        // Validate step
        {
            if self.step <= 0.0 {
                anyhow::bail!(
                    "Validation error for contract '{}': 'step' must be positive, got {}",
                    contract_name,
                    self.step,
                );
            }
        }

        // Validate step_price
        {
            if self.step_price <= 0.0 {
                anyhow::bail!(
                    "Validation error for contract '{}': 'step_price' must be positive, got {}",
                    contract_name,
                    self.step_price
                );
            }
        }

        // Validate marginal_costs
        {
            if self.marginal_costs < 0.0 {
                anyhow::bail!(
                    "Validation error for contract '{}': 'marginal_costs' cannot be negative, got {}",
                    contract_name,
                    self.marginal_costs,
                )
            }
        }

        anyhow::Ok(())
    }

}

/// Type alias for a map of instrument names to their metadata.
pub type InstrumentBaseInfo = std::collections::HashMap<String, InstrumentInfo>;

/// Registry for all instrument metadata.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InstrumentsInfoRegistry(
    pub std::collections::HashMap<String, InstrumentBaseInfo>
);

impl InstrumentsInfoRegistry {
    /// Loads the instrument metadata from the `instruments_info.json` file.
    /// # Returns
    /// * `anyhow::Result<InstrumentsInfoRegistry>` containing the loaded metadata.
    pub fn load(settings: &settings::Settings,) -> anyhow::Result<Self> {
        // Validate all instruments
        let file_path = settings.common.instrument_info_path.clone();
        let contents = std::fs::read_to_string(file_path)?;
        let registry: InstrumentsInfoRegistry = serde_json::from_str(&contents)?;

        // Validate registry
        for (_instrument_name, base_info) in registry.0.iter() {
            for (contract_name, instrument_info) in base_info.iter() {
                instrument_info.validate(contract_name)?;
            }
        }

        anyhow::Ok(registry)
    }

    /// Returns the instrument metadata for a specific symbol.
    /// # Arguments
    /// * `symbol` - The symbol to retrieve metadata for (e.g., "Si-12.23").
    /// # Returns
    /// * An optional reference to the `InstrumentInfo`, or `None` if not found.
    pub fn get_instrument_info(
        &self,
        symbol: &str,
    ) -> Option<&InstrumentInfo> {
        // Finds instrument info by symbol (e.g., "Si-12.23").

        for base_name in self.0.keys() {
            // Get the instrument base info for the base name.
            if let Some(instrument_base_info) = self.0.get(base_name) {
                // Get the instrument info for the symbol.
                if let Some(instrument_info) = instrument_base_info.get(symbol) {
                    return Some(instrument_info);
                }
            }
        }
        None
    }

    /// Returns a map of InstrumentInfo for all symbols in a strategy.
    /// # Arguments
    /// * `symbol_list` - The list of symbols to retrieve metadata for.
    /// # Returns
    /// * `anyhow::Result<std::collections::HashMap<String, InstrumentInfo>>` containing the metadata for all symbols.
    pub fn get_instrument_info_for_strategy(&self, symbol_list: &[String]) -> anyhow::Result<std::collections::HashMap<String, InstrumentInfo>> {
        // Returns a map of InstrumentInfo for all symbols in a strategy.

        let mut result: std::collections::HashMap<String, InstrumentInfo> = std::collections::HashMap::new();
        for symbol in symbol_list {
            let instrument_info = self.get_instrument_info(symbol).unwrap();
            result.insert(symbol.clone(), instrument_info.clone());
        }

        anyhow::Ok(result)
    }

}

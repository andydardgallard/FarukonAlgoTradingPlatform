// farukon_core/src/instruments_info.rs

use anyhow::Context;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InstrumentInfo {
    pub exchange: String,
    #[serde(rename = "type")]
    pub instrument_type: String,
    pub contract_precision: usize,
    pub margin: f64,
    pub commission_type: String,
    pub trade_from_date: String,
    pub expiration_date: String,
    pub marginal_costs: f64,
    pub step: f64,
    pub step_price: f64,
}

impl InstrumentInfo {
    pub fn validate(&self, contract_name: &str) -> anyhow::Result<()> {
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

pub type InstrumentBaseInfo = std::collections::HashMap<String, InstrumentInfo>;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct InstrumentsInfoRegistry(
    pub std::collections::HashMap<String, InstrumentBaseInfo>
);

impl InstrumentsInfoRegistry {
    pub fn load() -> anyhow::Result<Self> {
        let file_path = "instruments_info.json";
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

    pub fn get_instrument_info(
        &self,
        symbol: &str,
    ) -> Option<&InstrumentInfo> {
        for base_name in self.0.keys() {
            if let Some(instrument_base_info) = self.0.get(base_name) {
                if let Some(instrument_info) = instrument_base_info.get(symbol) {
                    return Some(instrument_info);
                }
            }
        }
        None
    }

    pub fn get_instrument_info_for_strategy(&self, symbol_list: &[String]) -> anyhow::Result<std::collections::HashMap<String, InstrumentInfo>> {
        let mut result: std::collections::HashMap<String, InstrumentInfo> = std::collections::HashMap::new();
        for symbol in symbol_list {
            let instrument_info = self.get_instrument_info(symbol).unwrap();
            result.insert(symbol.clone(), instrument_info.clone());
        }

        anyhow::Ok(result)
    }

}

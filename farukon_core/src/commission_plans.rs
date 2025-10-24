
use crate::settings;
use crate::instruments_info;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CommissionPlans {
    pub exchanges: std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
}

impl CommissionPlans {
    pub fn load() -> anyhow::Result<Self> {
        let file_path = "commission_plans.json";
        let contents = std::fs::read_to_string(file_path)?;
        let plans: Self = serde_json::from_str(&contents)?;

        anyhow::Ok(plans)
    }

    pub fn get_commission(
        &self,
        exchange: &str,
        instrument_type: &str,
        plan_name: &str,
    ) -> Option<f64> {
        let exchange_map = self.exchanges.get(exchange)?;
        let plan_value = exchange_map.get(plan_name)?;

        if let Some(obj) = plan_value.as_object() {
            if let Some(amount) = obj.get(instrument_type) {
                if let Some(value) = amount.as_f64() {
                    return Some(value);
                }
            }
        } else if let Some(value) = plan_value.as_f64() {
            return Some(value);
        }

        None
    }

    pub fn get_plan_value (
        &self,
        exchange: &str,
        plan_name: &str,
        key: &str,
    ) -> Option<serde_json::Value> {
        let exchange_map = self.exchanges.get(exchange)?;
        let plan_value = exchange_map.get(plan_name)?;
        plan_value.get(key).cloned()
    }

}

pub fn calculate_forts_comission(
    price: Option<f64>,
    strategy_instruments_info_for_symbol: &instruments_info::InstrumentInfo,
    strategy_settings: &settings::StrategySettings,
) -> Option<f64> {
    let exchange = &strategy_instruments_info_for_symbol.exchange;
    let step_price = strategy_instruments_info_for_symbol.step_price;
    let step = strategy_instruments_info_for_symbol.step;
    let commission_type = strategy_instruments_info_for_symbol.commission_type.clone();
    
    let commission_plans_map = strategy_settings.commission_plans
        .as_ref()
        .and_then(|cp| cp.exchanges.get(exchange));

    if let Some(plans) = commission_plans_map {
        let mut total_commission_rate = 0.0;

        for (_plan_name, plan_value) in plans {
            if let Some(obj) = plan_value.as_object() {
                if let Some(currency_val) = obj.get(&commission_type) {
                    if let Some(amount) = currency_val.as_f64() {
                        total_commission_rate += amount;
                    }
                }
            }
        }

        if total_commission_rate > 0.0 {
            let cost_of_step_price = ((step_price / step) * 100_000.0).round() / 100_000.0;
            let commission_base = (price.unwrap().abs() * cost_of_step_price * 100.0).round() / 100.0;
            let commission = (commission_base * total_commission_rate * 100.0).round() / 100.0;

            return Some(commission);
        }
    }
    None

}

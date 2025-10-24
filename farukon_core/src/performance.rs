// farukon_core/src/performance.rs

use crate:: settings;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PerformanceMetrics {
    total_return: f64,
    total_return_percent: f64,
    apr_percent: f64,
    max_drawdown: f64,
    max_drawdown_percent: f64,
    apr_to_drawdown_ratio: f64,
    recovery_factor: f64,
    recovery_factor_percent: f64,
    deals_count: usize,
}

impl PerformanceMetrics {
    pub fn default() -> Self {
        Self {
            total_return: 0.0,
            total_return_percent: 0.0,
            apr_percent: 0.0,
            max_drawdown: 0.0,
            max_drawdown_percent: 0.0,
            apr_to_drawdown_ratio: 0.0,
            recovery_factor: 0.0,
            recovery_factor_percent: 0.0,
            deals_count: 0,
        }
    }

    pub fn to_stats_list(&self) -> Vec<(String, String)> {
        let mut stats = Vec::new();

        stats.push(("Total Return".to_string(), format!("{:.2}", self.total_return)));
        stats.push(("Total Return %".to_string(), format!("{:.5}", self.total_return_percent)));
        stats.push(("APR %".to_string(), format!("{:.5}", self.apr_percent)));
        stats.push(("Max Drawdown".to_string(), format!("{:.2}", self.max_drawdown)));
        stats.push(("Max Drawdown %".to_string(), format!("{:.5}", self.max_drawdown_percent)));
        stats.push(("APR/Drawdown Factor".to_string(), format!("{:.2}", self.apr_to_drawdown_ratio)));
        stats.push(("Recovery Factor".to_string(), format!("{:.2}", self.recovery_factor)));
        stats.push(("Recovery Factor %".to_string(), format!("{:.2}", self.recovery_factor_percent)));
        stats.push(("Deals Count".to_string(), self.deals_count.to_string()));

        stats
    }

    pub fn get_apr_to_drawdown_ratio(&self) -> &f64 {
        &self.apr_to_drawdown_ratio
    }

    pub fn get_total_return(&self) -> &f64 {
        &self.total_return
    }

    pub fn get_total_return_percent(&self) -> &f64 {
        &self.total_return_percent
    }

    pub fn get_apr_percent(&self) -> &f64 {
        &self.apr_percent
    }

    pub fn get_max_drawdown(&self) -> &f64 {
        &self.max_drawdown
    }

    pub fn get_max_drawdown_percent(&self) -> &f64 {
        &self.max_drawdown_percent
    }

    pub fn get_recovery_factor(&self) -> &f64 {
        &self.recovery_factor
    }

    pub fn get_recovery_factor_percent(&self) -> &f64 {
        &self.recovery_factor_percent
    }

    pub fn get_deals_count(&self) -> &usize {
        &self.deals_count
    }

}

pub struct PerformanceManager {
    initial_capital_for_strategy: f64,
    #[allow(dead_code)]
    metrics_calculation_mode: settings::MetricsMode, // TODO online calculation
    metrics: PerformanceMetrics,
    returns: Vec<f64>,
    equity_curve: Vec<f64>,
    drawdowns: Vec<f64>,
    drawdowns_percent: Vec<f64>,
    peak: f64,
    max_drawdown: f64,
    max_drawdown_percent: f64,
}

impl PerformanceManager {
    pub fn new(
        initial_capital_for_strategy: f64,
        strategy_settings: &settings::StrategySettings
    ) -> Self {
        let mode = strategy_settings.portfolio_settings_for_strategy.metrics_calculation_mode.clone();
        Self {
            initial_capital_for_strategy,
            metrics_calculation_mode: mode,
            metrics: PerformanceMetrics::default(),
            returns: vec![],
            equity_curve: vec![initial_capital_for_strategy],
            drawdowns: vec![],
            drawdowns_percent: vec![],
            peak: initial_capital_for_strategy,
            max_drawdown: 0.0,
            max_drawdown_percent: 0.0,
        }
    }

    pub fn update_incremental(
        &mut self,
        current_total: f64,
        start_date: chrono::DateTime<chrono::Utc>,
        end_date: chrono::DateTime<chrono::Utc>,
        deals_count: usize,
    ) {
        self.equity_curve.push(current_total);
        self.peak = self.peak.max(current_total);

        let dd = if self.peak > 0.0 { current_total - self.peak } else { 0.0 };
        self.drawdowns.push(dd);
        self.max_drawdown = self.max_drawdown.min(dd);

        let dd_percent = if self.peak > 0.0 { (current_total / self.peak) - 1.0 } else { 0.0 };
        self.drawdowns_percent.push(dd_percent);
        self.max_drawdown_percent = self.max_drawdown_percent.min(dd_percent);

        self.update_metrics(start_date, end_date, deals_count);
    }

    fn update_metrics(
        &mut self,
        start_date: chrono::DateTime<chrono::Utc>,
        end_date: chrono::DateTime<chrono::Utc>,
        deals_count: usize,
    ) {
        let days = (end_date - start_date).num_days() as f64;
        let years = days / 365.0;

        let current_equity = *self.equity_curve.last().unwrap_or(&1.0);
        let current_return = current_equity - self.initial_capital_for_strategy;
        let current_return_percent = (current_equity / self.initial_capital_for_strategy) - 1.0;

        let apr = (1.0 + current_return_percent).powf(1.0 / years.max(1e-8)) - 1.0;

        self.metrics = PerformanceMetrics {
            total_return: current_return,
            total_return_percent: current_return_percent,
            apr_percent: apr,
            max_drawdown: self.max_drawdown,
            max_drawdown_percent: self.max_drawdown_percent,
            apr_to_drawdown_ratio: if self.max_drawdown_percent.abs() > 1e-8 { apr / self.max_drawdown_percent.abs() } else { 0.0 },
            recovery_factor: current_return / self.max_drawdown.abs().max(1e-8),
            recovery_factor_percent: current_return_percent / self.max_drawdown_percent.abs().max(1e-8),
            deals_count,
        }
    } 

    pub fn calculate_final(
        &mut self,
        equity_series: &[f64],
        start_date: chrono::DateTime<chrono::Utc>,
        end_date: chrono::DateTime<chrono::Utc>,
        deals_count: usize,
    ) {
        let series = Vec::from(equity_series);
        let n = series.len();

        if n < 2 { return; }

        // SIMD: returns
        self.returns = calculate_returns_simd(&series);

        // Cumulative equity curve
        self.equity_curve.clear();
        self.equity_curve.push(self.initial_capital_for_strategy);
        for i in 1..n {
            let r = self.returns[i];
            let last_eq = self.equity_curve.last().copied().unwrap_or(self.initial_capital_for_strategy);
            self.equity_curve.push(last_eq * (1.0 + r));
        }

        // Max drawdown
        let (dd_percent, max_dd_percent) = calculate_drawdowns_simd(&series, "percent");
        self.drawdowns_percent = dd_percent;
        self.max_drawdown_percent = max_dd_percent;

        let (dd, max_dd) = calculate_drawdowns_simd(&series, "currency");
        self.drawdowns = dd;
        self.max_drawdown = max_dd;

        self.update_metrics(start_date, end_date, deals_count);

    }

    pub fn get_current_performance_metrics(&self) -> &PerformanceMetrics {
        &self.metrics
    }
    
}

fn calculate_returns_simd(equity: &[f64]) -> Vec<f64> {
    let n = equity.len();
    if n < 2 { return vec![0.0; n]; }

    let mut returns = vec![0.0; n];

    let chunks = (n - 1) / 4;
    for i in 0..chunks {
        let start = i * 4 + 1;
        if start + 3 >= n { break; }

        let prev_values = wide::f64x4::from([
            equity[start - 1],
            equity[start],
            equity[start + 1],
            equity[start + 2]
        ]);

        let curr_values = wide::f64x4::from([
            equity[start],
            equity[start + 1],
            equity[start + 2],
            equity[start + 3]
        ]);

        let ret_values = (curr_values / prev_values) - wide::f64x4::splat(1.0);

        let result_array: [f64; 4] = ret_values.into();
        returns[start..start+4].copy_from_slice(&result_array);
    }

    let processed_elements = chunks * 4 + 1;
    for i in processed_elements..n {
        let prev = equity[i - 1];
        let curr = equity[i];
        returns[i] = if prev != 0.0 { (curr / prev) - 1.0 } else { 0.0 };
    }

    returns
}

fn calculate_drawdowns_simd(equity: &[f64], calc_type: &str) -> (Vec<f64>, f64) {
    let n = equity.len();
    if n == 0 {
        return (Vec::new(), 0.0);
    }

    let mut drawdowns = vec![0.0; n];
    let mut peak = equity[0];
    let mut max_dd = 0.0;
    
    drawdowns[0] = 0.0;

    let chunks = (n - 1) / 4;
    for i in 0..chunks {
        let start = i * 4 + 1;
        if start + 3 >= n { break; }

        let values = wide::f64x4::from([
            equity[start],
            equity[start + 1],
            equity[start + 2],
            equity[start + 3]
        ]);

        for j in 0..4 {
            let value = equity[start + j];
            if value > peak {
                peak = value;
            }
        }

        let peak_vec = wide::f64x4::splat(peak);
        let mut dd_vec = wide::f64x4::splat(0.0);

        match calc_type {
            "percent" => {
                dd_vec = (values / peak_vec) - wide::f64x4::splat(1.0);
            },
            "currency" => {
                dd_vec = values - peak_vec;
            },
            _ => {
                eprintln!("Wrong type of calculation drawdown!");
            }
        }
        
        let dd_array: [f64; 4] = dd_vec.into();
        drawdowns[start..start+4].copy_from_slice(&dd_array);

        for j in 0..4 {
            let dd = dd_array[j];
            if dd < max_dd {
                max_dd = dd
            }
        }
    }

    let processed_elements = chunks * 4 + 1;
    for i in processed_elements..n {
        let value = equity[i];
        if value > peak {
            peak = value;
        }

        let mut dd = 0.0;

        match calc_type {
            "percent" => {
                dd = (value / peak) - 1.0;
            },
            "currency" => {
                dd = value - peak;
            },
            _ => {
                eprintln!("Wrong type of calculation drawdown!");
            }
        }

        drawdowns[i] = dd;

        if dd < max_dd {
            max_dd = dd;
        }
    }

    (drawdowns, max_dd)
}

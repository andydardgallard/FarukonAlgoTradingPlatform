// farukon_core/src/performance.rs

//! Performance metrics calculation engine.
//! Uses SIMD for ultra-fast return and drawdown calculations.

use crate:: settings;

/// Structure holding all calculated performance metrics for a strategy.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PerformanceMetrics {
    /// Total profit or loss in base currency (e.g., USD).
    total_return: f64,
    /// Total return as a percentage of initial capital.
    total_return_percent: f64,
    /// Annualized Percentage Return (APR) as a percentage.
    apr: f64,
    /// Maximum drawdown as a percentage of peak equity.
    max_drawdown: f64,
    /// Ratio of APR to Max Drawdown (higher is better).
    apr_to_drawdown_ratio: f64,
    /// Recovery Factor as a percentage.
    recovery_factor: f64,
    /// Total number of trades executed.
    deals_count: usize,
}

impl PerformanceMetrics {
    /// Creates a new, empty PerformanceMetrics object with default values.
    pub fn default() -> Self {
        Self {
            total_return: 0.0,
            total_return_percent: 0.0,
            apr: 0.0,
            max_drawdown: 0.0,
            apr_to_drawdown_ratio: 0.0,
            recovery_factor: 0.0,
            deals_count: 0,
        }
    }

    /// Converts the performance metrics into a list of key-value pairs for display.
    pub fn to_stats_list(&self) -> Vec<(String, String)> {
        let mut stats = Vec::new();

        stats.push(("Total_Return".to_string(), format!("{:.2}", self.total_return)));
        stats.push(("Total_Return_%".to_string(), format!("{:.5}", self.total_return_percent)));
        stats.push(("APR".to_string(), format!("{:.5}", self.apr)));
        stats.push(("Max_Drawdown".to_string(), format!("{:.5}", self.max_drawdown)));
        stats.push(("APR/Drawdown_factor".to_string(), format!("{:.2}", self.apr_to_drawdown_ratio)));
        stats.push(("Recovery_Factor".to_string(), format!("{:.2}", self.recovery_factor)));
        stats.push(("Deals_Count".to_string(), self.deals_count.to_string()));

        stats
    }

    // Getters

    /// Returns a reference to the APR to Drawdown ratio.
    pub fn get_apr_to_drawdown_ratio(&self) -> &f64 {
        &self.apr_to_drawdown_ratio
    }

    /// Returns a reference to the total return.
    pub fn get_total_return(&self) -> &f64 {
        &self.total_return
    }

    /// Returns a reference to the total return as a percentage.
    pub fn get_total_return_percent(&self) -> &f64 {
        &self.total_return_percent
    }

    /// Returns a reference to the APR as a percentage.
    pub fn get_apr(&self) -> &f64 {
        &self.apr
    }

    /// Returns a reference to the maximum drawdown as a percentage.
    pub fn get_max_drawdown(&self) -> &f64 {
        &self.max_drawdown
    }

    /// Returns a reference to the recovery factor as a percentage.
    pub fn get_recovery_factor(&self) -> &f64 {
        &self.recovery_factor
    }

    /// Returns a reference to the deal count.
    pub fn get_deals_count(&self) -> &usize {
        &self.deals_count
    }

}

/// Manager for calculating performance metrics.
/// Can calculate metrics incrementally during backtest or offline at the end.
pub struct PerformanceManager {
    /// Initial capital allocated to this strategy.
    initial_capital_for_strategy: f64,
    /// Mode for calculating metrics (offline or realtime).
    #[allow(dead_code)]
    metrics_calculation_mode: settings::MetricsMode, // TODO online calculation
    /// Current performance metrics.
    metrics: PerformanceMetrics,
    /// List of daily returns.
    returns: Vec<f64>,
    /// Equity curve (capital over time).
    equity_curve: Vec<f64>,
    // /// Drawdowns as percentages over time.
    // drawdowns: Vec<f64>,
    /// Highest equity reached so far.
    peak: f64,
    /// Maximum drawdown as a percentage.
    max_drawdown: f64,
}

impl PerformanceManager {
    /// Creates a new PerformanceManager.
    /// # Arguments
    /// * `initial_capital_for_strategy` - The starting capital for the strategy.
    /// * `strategy_settings` - The strategy settings, which include the metrics calculation mode.
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
            // drawdowns: vec![],
            peak: initial_capital_for_strategy,
            max_drawdown: 0.0,
        }
    }

    /// Updates performance metrics incrementally during the backtest.
    /// This is used when `metrics_calculation_mode` is `RealTime`.
    /// # Arguments
    /// * `current_total` - The current total capital.
    /// * `start_date` - The start date of the backtest.
    /// * `end_date` - The end date of the backtest.
    /// * `deals_count` - The number of deals executed so far.
    pub fn update_incremental(
        &mut self,
        current_total: f64,
        start_date: chrono::DateTime<chrono::Utc>,
        end_date: chrono::DateTime<chrono::Utc>,
        deals_count: usize,
    ) {
        self.equity_curve.push(current_total);
        self.peak = self.peak.max(current_total);

        let dd_percent = if self.peak > 0.0 { (current_total / self.peak) - 1.0 } else { 0.0 };
        // self.drawdowns.push(dd_percent);
        self.max_drawdown = self.max_drawdown.min(dd_percent);

        self.update_metrics(start_date, end_date, deals_count);
    }

    /// Calculate metrics
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
            apr,
            max_drawdown: self.max_drawdown,
            apr_to_drawdown_ratio: if self.max_drawdown.abs() > 1e-8 { apr.abs() / self.max_drawdown.abs() } else { 0.0 },
            recovery_factor: current_return_percent.abs() / self.max_drawdown.abs().max(1e-8),
            deals_count,
        }
    } 

    /// Calculates final performance metrics after the backtest is complete.
    /// This is used when `metrics_calculation_mode` is `Offline`.
    /// # Arguments
    /// * `equity_series` - The full equity curve (capital over time).
    /// * `start_date` - The start date of the backtest.
    /// * `end_date` - The end date of the backtest.
    /// * `deals_count` - The total number of deals executed.
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
        let max_dd_percent = calculate_drawdowns_simd(&series);
        self.max_drawdown = max_dd_percent;

        self.update_metrics(start_date, end_date, deals_count);

    }

    /// Returns a reference to the current performance metrics.
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

fn calculate_drawdowns_simd(equity: &[f64]) -> f64 {
    let n = equity.len();
    if n == 0 {
        return 0.0;
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
        let dd_vec = (values / peak_vec) - wide::f64x4::splat(1.0);
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

        let dd = (value / peak) - 1.0;
        drawdowns[i] = dd;

        if dd < max_dd {
            max_dd = dd;
        }
    }

    max_dd
}

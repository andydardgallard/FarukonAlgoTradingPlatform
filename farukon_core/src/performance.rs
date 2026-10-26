// farukon_core/src/performance.rs

//! Performance metrics calculation engine.
//! Uses SIMD for ultra-fast return and drawdown calculations.

use crate::settings;

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
    /// Maximum drawdown as an absolute amount in base currency (equity minus peak).
    max_drawdown_pct: f64,
    /// Ratio of APR to Max Drawdown (higher is better).
    apr_to_drawdown_ratio: f64,
    /// Recovery Factor: signed net return divided by the absolute max drawdown amount, so an
    /// equal-magnitude loss yields a negative value.
    recovery_factor: f64,
    /// Total number of trades executed.
    deals_count: usize,
    /// Timestamp of the relative maximum drawdown. `None` until `calculate_final` runs: the
    /// incremental `RealTime` update has no timestamps, so the value is set for every metrics mode
    /// only in the final calculation.
    #[serde(default)]
    max_drawdown_datetime: Option<chrono::DateTime<chrono::Utc>>,
}

impl Default for PerformanceMetrics {
    /// Creates a new, empty PerformanceMetrics object with default values.
    fn default() -> Self {
        Self {
            total_return: 0.0,
            total_return_percent: 0.0,
            apr: 0.0,
            max_drawdown: 0.0,
            max_drawdown_pct: 0.0,
            apr_to_drawdown_ratio: 0.0,
            recovery_factor: 0.0,
            deals_count: 0,
            max_drawdown_datetime: None,
        }
    }
}

impl PerformanceMetrics {
    /// Converts the performance metrics into a list of key-value pairs for display.
    pub fn to_stats_list(&self) -> Vec<(String, String)> {
        let mut stats = Vec::new();

        stats.push((
            "Total_Return".to_string(),
            format!("{:.2}", self.total_return),
        ));
        stats.push((
            "Total_Return_%".to_string(),
            format!("{:.5}", self.total_return_percent),
        ));
        stats.push(("APR".to_string(), format!("{:.5}", self.apr)));
        stats.push((
            "Max_Drawdown".to_string(),
            format!("{:.5}", self.max_drawdown),
        ));
        // Timestamp of the relative `Max_Drawdown` trough, not of `Max_Drawdown_pct`.
        stats.push((
            "Max_Drawdown_DateTime".to_string(),
            self.max_drawdown_datetime
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_default(),
        ));
        stats.push((
            "Max_Drawdown_pct".to_string(),
            format!("{:.5}", self.max_drawdown_pct),
        ));
        stats.push((
            "APR/Drawdown_factor".to_string(),
            format!("{:.2}", self.apr_to_drawdown_ratio),
        ));
        stats.push((
            "Recovery_Factor".to_string(),
            format!("{:.2}", self.recovery_factor),
        ));
        stats.push((
            "Composite".to_string(),
            format!("{:.5}", self.get_composite_score()),
        ));
        stats.push(("Deals_Count".to_string(), self.deals_count.to_string()));

        stats
    }

    // Getters

    /// Calculates the Composite score from the APR/DD ratio, the signed Recovery Factor and the
    /// relative Max Drawdown: each sub-metric below its floor subtracts a 1000-point penalty, and
    /// the log term is clamped so a Recovery Factor below -1 stays finite.
    pub fn get_composite_score(&self) -> f64 {
        let apr_dd_metric = self.apr_to_drawdown_ratio;
        let rf_metric = self.recovery_factor;
        let dd_metric = self.max_drawdown;

        let mut penalty = 0.0;
        if apr_dd_metric < 3.0 {
            penalty += 1000.0;
        }
        if rf_metric < 5.0 {
            penalty += 1000.0;
        }
        if dd_metric < -0.3 {
            penalty += 1000.0;
        }

        // A signed Recovery Factor can fall below -1, where `ln` is undefined: clamp the argument so
        // a losing strategy scores finite (and heavily negative) instead of NaN.
        200.0 * apr_dd_metric + (1.0 + rf_metric).max(1e-12).ln() / 10.0 - penalty
    }

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

    /// Returns a reference to the maximum absolute drawdown (equity minus peak) in base currency.
    pub fn get_max_drawdown_pct(&self) -> &f64 {
        &self.max_drawdown_pct
    }

    /// Returns a reference to the timestamp of the maximum drawdown, if computed.
    pub fn get_max_drawdown_datetime(&self) -> &Option<chrono::DateTime<chrono::Utc>> {
        &self.max_drawdown_datetime
    }

    /// Returns a reference to the recovery factor (signed net return over the absolute max
    /// drawdown; negative for a net loss).
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
    /// Highest equity reached so far.
    peak: f64,
    /// Maximum drawdown as a percentage.
    max_drawdown: f64,
    max_drawdown_pct: f64,
    drawdown_curve: Vec<f64>,
    drawdown_pct_curve: Vec<f64>,
}

impl PerformanceManager {
    /// Creates a new PerformanceManager.
    /// # Arguments
    /// * `initial_capital_for_strategy` - The starting capital for the strategy.
    /// * `strategy_settings` - The strategy settings, which include the metrics calculation mode.
    pub fn new(
        initial_capital_for_strategy: f64,
        strategy_settings: &settings::StrategySettings,
    ) -> Self {
        let mode = strategy_settings
            .portfolio_settings_for_strategy
            .metrics_calculation_mode
            .clone();
        Self {
            initial_capital_for_strategy,
            metrics_calculation_mode: mode,
            metrics: PerformanceMetrics::default(),
            returns: vec![],
            equity_curve: vec![initial_capital_for_strategy],
            peak: initial_capital_for_strategy,
            max_drawdown: 0.0,
            max_drawdown_pct: 0.0,
            drawdown_curve: vec![],
            drawdown_pct_curve: vec![],
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

        let dd_percent = if self.peak > 0.0 {
            (current_total / self.peak) - 1.0
        } else {
            0.0
        };
        // self.drawdowns.push(dd_percent);
        self.max_drawdown = self.max_drawdown.min(dd_percent);

        let dd = if self.peak > 0.0 {
            current_total - self.peak
        } else {
            0.0
        };
        self.max_drawdown_pct = self.max_drawdown_pct.min(dd);

        self.update_metrics(start_date, end_date, deals_count, None);
    }

    /// Calculate metrics
    fn update_metrics(
        &mut self,
        start_date: chrono::DateTime<chrono::Utc>,
        end_date: chrono::DateTime<chrono::Utc>,
        deals_count: usize,
        max_drawdown_datetime: Option<chrono::DateTime<chrono::Utc>>,
    ) {
        let days = (end_date - start_date).num_days() as f64;
        let years = days / 365.0;

        let current_equity = *self.equity_curve.last().unwrap_or(&1.0);
        let current_return = current_equity - self.initial_capital_for_strategy;
        let current_return_percent = (current_equity / self.initial_capital_for_strategy) - 1.0;

        let apr = if current_equity <= 0.0 || years <= 0.0 {
            if years <= 0.0 { 0.0 } else { -1.0 }
        } else {
            (1.0 + current_return_percent).powf(1.0 / years.max(1e-8)) - 1.0
        };

        let max_dd_abs = self.max_drawdown.abs();
        let apr_to_drawdown_ratio = if max_dd_abs > 1e-8 {
            apr / max_dd_abs
        } else {
            apr * 100.0
        };

        // Signed net return over the absolute drawdown: an equal-magnitude loss must produce a
        // negative Recovery Factor, otherwise maximizing it can prefer a losing strategy.
        let max_dd_pct_abs = self.max_drawdown_pct.abs();
        let recovery_factor = if max_dd_pct_abs > 1e-8 {
            current_return / max_dd_pct_abs
        } else {
            current_return_percent * 100.0
        };

        self.metrics = PerformanceMetrics {
            total_return: current_return,
            total_return_percent: current_return_percent,
            apr,
            max_drawdown: self.max_drawdown,
            max_drawdown_pct: self.max_drawdown_pct,
            apr_to_drawdown_ratio,
            recovery_factor,
            deals_count,
            max_drawdown_datetime,
        }
    }

    /// Calculates final performance metrics after the backtest is complete.
    /// Called in every metrics mode (`Offline` and `RealTime`): the full equity curve is the only
    /// source of the drawdown timestamp, while `RealTime` additionally keeps its incremental
    /// per-bar updates during the run.
    /// # Arguments
    /// * `equity_series` - The full equity curve (capital over time).
    /// * `equity_datetimes` - Timestamps parallel to `equity_series`.
    /// * `start_date` - The start date of the backtest.
    /// * `end_date` - The end date of the backtest.
    /// * `deals_count` - The total number of deals executed.
    pub fn calculate_final(
        &mut self,
        equity_series: &[f64],
        equity_datetimes: &[chrono::DateTime<chrono::Utc>],
        start_date: chrono::DateTime<chrono::Utc>,
        end_date: chrono::DateTime<chrono::Utc>,
        deals_count: usize,
    ) {
        let n = equity_series.len();

        // Only an empty curve carries no information. A one-point curve is a valid (degenerate)
        // result: it has zero returns and zero drawdowns, but the final return metrics, the deal
        // count and the drawdown timestamp must still be computed rather than left at their
        // defaults.
        if n == 0 {
            return;
        }

        // SIMD: returns
        self.returns = calculate_returns_simd(equity_series);

        // Cumulative equity curve
        self.equity_curve.clear();
        self.equity_curve.push(self.initial_capital_for_strategy);
        for i in 1..n {
            let r = self.returns[i];
            let last_eq = self
                .equity_curve
                .last()
                .copied()
                .unwrap_or(self.initial_capital_for_strategy);
            self.equity_curve.push(last_eq * (1.0 + r));
        }

        // Max drawdown
        let drawdow_calculation_results = calculate_drawdowns_simd(equity_series);

        let max_dd_percent = drawdow_calculation_results.0;
        self.max_drawdown = max_dd_percent;
        self.drawdown_curve = drawdow_calculation_results.2;

        let max_dd_pnct = drawdow_calculation_results.1;
        self.max_drawdown_pct = max_dd_pnct;
        self.drawdown_pct_curve = drawdow_calculation_results.3;

        let max_drawdown_datetime = equity_datetimes.get(drawdow_calculation_results.4).copied();

        self.update_metrics(start_date, end_date, deals_count, max_drawdown_datetime);
    }

    /// Returns a reference to the current performance metrics.
    pub fn get_current_performance_metrics(&self) -> &PerformanceMetrics {
        &self.metrics
    }

    pub fn get_drawdons(&self) -> &Vec<f64> {
        &self.drawdown_curve
    }

    pub fn get_drawdowns_pct(&self) -> &Vec<f64> {
        &self.drawdown_pct_curve
    }
}

fn calculate_returns_simd(equity: &[f64]) -> Vec<f64> {
    let n = equity.len();
    if n < 2 {
        return vec![0.0; n];
    }

    let mut returns = vec![0.0; n];

    let chunks = (n - 1) / 4;
    for i in 0..chunks {
        let start = i * 4 + 1;
        if start + 3 >= n {
            break;
        }

        let prev_lanes = [
            equity[start - 1],
            equity[start],
            equity[start + 1],
            equity[start + 2],
        ];
        // The scalar tail treats a zero previous equity as a zero return; the SIMD lane must do the
        // same. A raw division by the zero lane produced Inf/NaN and poisoned every headline metric
        // of an account that ever hit zero equity, so zero lanes divide by 1.0 and are overwritten
        // with 0.0 below.
        let mut safe_prev_lanes = prev_lanes;
        let mut zero_prev_lanes = [false; 4];
        for (lane, zero) in safe_prev_lanes.iter_mut().zip(zero_prev_lanes.iter_mut()) {
            if *lane == 0.0 {
                *lane = 1.0;
                *zero = true;
            }
        }

        let prev_values = wide::f64x4::from(safe_prev_lanes);

        let curr_values = wide::f64x4::from([
            equity[start],
            equity[start + 1],
            equity[start + 2],
            equity[start + 3],
        ]);

        let ret_values = (curr_values / prev_values) - wide::f64x4::splat(1.0);

        let mut result_array: [f64; 4] = ret_values.into();
        for (ret, zero) in result_array.iter_mut().zip(zero_prev_lanes) {
            if zero {
                *ret = 0.0;
            }
        }
        returns[start..start + 4].copy_from_slice(&result_array);
    }

    let processed_elements = chunks * 4 + 1;
    for i in processed_elements..n {
        let prev = equity[i - 1];
        let curr = equity[i];
        returns[i] = if prev != 0.0 {
            (curr / prev) - 1.0
        } else {
            0.0
        };
    }

    returns
}

/// Calculates the drawdown curves with SIMD.
///
/// Returns `(max_drawdown, max_drawdown_pct, drawdowns, drawdowns_pct, max_drawdown_index)`,
/// where the index is the position of the maximum (most negative) relative drawdown in the
/// equity series and is used to look up the corresponding timestamp.
fn calculate_drawdowns_simd(equity: &[f64]) -> (f64, f64, Vec<f64>, Vec<f64>, usize) {
    let n = equity.len();
    if n == 0 {
        return (0.0, 0.0, vec![], vec![], 0);
    }

    let mut drawdowns = vec![0.0; n];
    let mut drawdowns_pct = vec![0.0; n];
    let mut peak = equity[0];
    let mut max_dd = 0.0;
    let mut max_dd_pct = 0.0;
    let mut max_dd_index = 0;

    if peak.is_nan() || peak.is_infinite() {
        let mut found_valid_peak = false;
        for &val in equity.iter() {
            if val.is_finite() {
                peak = val;
                found_valid_peak = true;
                break;
            }
        }
        if !found_valid_peak {
            return (0.0, 0.0, vec![0.0; n], vec![0.0; n], 0);
        }
    }

    drawdowns[0] = 0.0;
    drawdowns_pct[0] = 0.0;

    let chunks = (n - 1) / 4;
    for i in 0..chunks {
        let start = i * 4 + 1;
        if start + 3 >= n {
            break;
        }

        let values = wide::f64x4::from([
            equity[start],
            equity[start + 1],
            equity[start + 2],
            equity[start + 3],
        ]);

        // Running-peak semantics of the scalar tail loop below: every value is measured against
        // the peak of the values up to and including itself. Advancing the peak across all four
        // lanes before the division measured a rising chunk against its own maximum and fabricated
        // a drawdown (e.g. [100, 200, 300, 400] read as -80%).
        let mut chunk_peaks = [0.0_f64; 4];
        for (j, chunk_peak) in chunk_peaks.iter_mut().enumerate() {
            let value = equity[start + j];
            if value.is_finite() && value > peak {
                peak = value;
            }
            *chunk_peak = peak;
        }

        let peak_vec = wide::f64x4::from(chunk_peaks);
        let dd_array: [f64; 4] = ((values / peak_vec) - wide::f64x4::splat(1.0)).into();
        let dd_pct_array: [f64; 4] = (values - peak_vec).into();

        drawdowns[start..start + 4].copy_from_slice(&dd_array);
        drawdowns_pct[start..start + 4].copy_from_slice(&dd_pct_array);

        for j in 0..4 {
            let dd: f64 = dd_array[j];
            if dd < max_dd {
                max_dd = dd;
                max_dd_index = start + j;
            }

            let dd_pct = dd_pct_array[j];
            if dd_pct < max_dd_pct {
                max_dd_pct = dd_pct
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

        let dd_pct = value - peak;
        drawdowns_pct[i] = dd_pct;

        if dd < max_dd {
            max_dd = dd;
            max_dd_index = i;
        }

        if dd_pct < max_dd_pct {
            max_dd_pct = dd_pct
        }
    }

    (max_dd, max_dd_pct, drawdowns, drawdowns_pct, max_dd_index)
}

#[cfg(test)]
mod tests {
    use super::{PerformanceManager, PerformanceMetrics};
    use crate::settings;

    fn test_strategy_settings() -> settings::StrategySettings {
        settings::StrategySettings {
            threads: None,
            strategy_name: "test".to_string(),
            strategy_path: String::new(),
            exit_results_path: String::new(),
            strategy_weight: 1.0,
            slippage: settings::ParamSpec::Discrete(vec![]),
            data: settings::DataSettings {
                data_path: String::new(),
                timeframe: String::new(),
            },
            symbols: vec![],
            strategy_params: std::collections::HashMap::new(),
            pos_sizer_params: settings::PosSizer {
                pos_sizer_name: "1".to_string(),
                pos_sizer_params: std::collections::HashMap::new(),
                pos_sizer_value: settings::ParamSpec::Discrete(vec![]),
            },
            margin_params: settings::MarginParams {
                min_margin: 0.0,
                margin_call_type: String::new(),
            },
            portfolio_settings_for_strategy: settings::PortfolioSettingsForStrategy {
                metrics_calculation_mode: settings::MetricsMode::Offline,
            },
            optimizer_type: settings::OptimizerType::GridSearch,
            commission_plans: None,
        }
    }

    #[test]
    fn composite_score_no_penalty() {
        let metrics = PerformanceMetrics {
            total_return: 100.0,
            total_return_percent: 10.0,
            apr: 10.0,
            max_drawdown: -0.2,
            max_drawdown_pct: -200.0,
            apr_to_drawdown_ratio: 4.0,
            recovery_factor: 6.0,
            deals_count: 10,
            max_drawdown_datetime: None,
        };

        let expected = 200.0 * 4.0 + (1.0_f64 + 6.0_f64).ln() / 10.0;
        assert!((metrics.get_composite_score() - expected).abs() < 1e-9);
    }

    #[test]
    fn composite_score_full_penalty() {
        let metrics = PerformanceMetrics {
            total_return: -50.0,
            total_return_percent: -5.0,
            apr: -10.0,
            max_drawdown: -0.5,
            max_drawdown_pct: -500.0,
            apr_to_drawdown_ratio: 2.0,
            recovery_factor: 4.0,
            deals_count: 10,
            max_drawdown_datetime: None,
        };

        let expected = 200.0 * 2.0 + (1.0_f64 + 4.0_f64).ln() / 10.0 - 3000.0;
        assert!((metrics.get_composite_score() - expected).abs() < 1e-9);
    }

    #[test]
    fn stats_list_contains_composite() {
        let metrics = PerformanceMetrics {
            total_return: 100.0,
            total_return_percent: 10.0,
            apr: 10.0,
            max_drawdown: -0.2,
            max_drawdown_pct: -200.0,
            apr_to_drawdown_ratio: 4.0,
            recovery_factor: 6.0,
            deals_count: 10,
            max_drawdown_datetime: None,
        };

        let stats = metrics.to_stats_list();
        assert!(stats.iter().any(|(key, _)| key == "Composite"));
    }

    #[test]
    fn stats_list_contains_formatted_max_drawdown_datetime() {
        let dt = chrono::DateTime::parse_from_rfc3339("2025-01-02T03:04:05Z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let metrics = PerformanceMetrics {
            max_drawdown_datetime: Some(dt),
            ..Default::default()
        };

        let value = metrics
            .to_stats_list()
            .into_iter()
            .find(|(key, _)| key == "Max_Drawdown_DateTime")
            .map(|(_, value)| value);
        assert_eq!(value.as_deref(), Some("2025-01-02 03:04:05"));

        let value = PerformanceMetrics::default()
            .to_stats_list()
            .into_iter()
            .find(|(key, _)| key == "Max_Drawdown_DateTime")
            .map(|(_, value)| value);
        assert_eq!(value.as_deref(), Some(""));
    }

    #[test]
    fn calculate_final_sets_max_drawdown_datetime() {
        let mut pm = PerformanceManager::new(100.0, &test_strategy_settings());

        let equity = [100.0, 80.0, 60.0, 90.0];
        let base = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let datetimes: Vec<chrono::DateTime<chrono::Utc>> = (0..equity.len())
            .map(|i| base + chrono::Duration::minutes(i as i64))
            .collect();

        pm.calculate_final(
            &equity,
            &datetimes,
            datetimes[0],
            *datetimes.last().unwrap(),
            1,
        );

        assert_eq!(
            pm.get_current_performance_metrics()
                .get_max_drawdown_datetime(),
            &Some(datetimes[2])
        );
    }

    #[test]
    fn calculate_final_sets_max_drawdown_datetime_from_simd_chunk() {
        let mut pm = PerformanceManager::new(100.0, &test_strategy_settings());

        // n >= 6 so the trough (index 3) is found by the SIMD chunk loop.
        let equity = [100.0, 200.0, 100.0, 40.0, 150.0, 180.0, 120.0, 175.0];
        let base = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let datetimes: Vec<chrono::DateTime<chrono::Utc>> = (0..equity.len())
            .map(|i| base + chrono::Duration::minutes(i as i64))
            .collect();

        pm.calculate_final(
            &equity,
            &datetimes,
            datetimes[0],
            *datetimes.last().unwrap(),
            1,
        );

        let metrics = pm.get_current_performance_metrics();
        assert!((metrics.get_max_drawdown() - -0.8).abs() < 1e-9);
        assert_eq!(metrics.get_max_drawdown_datetime(), &Some(datetimes[3]));
    }

    /// Scalar running-peak reference for the drawdown curves: every value is measured against the
    /// peak of the values up to and including itself.
    fn scalar_drawdowns(equity: &[f64]) -> (f64, f64, Vec<f64>, Vec<f64>, usize) {
        let n = equity.len();
        let mut drawdowns = vec![0.0; n];
        let mut drawdowns_pct = vec![0.0; n];
        let mut peak = equity[0];
        let mut max_dd = 0.0;
        let mut max_dd_pct = 0.0;
        let mut max_dd_index = 0;

        for i in 1..n {
            let value = equity[i];
            if value > peak {
                peak = value;
            }

            let dd = (value / peak) - 1.0;
            drawdowns[i] = dd;

            let dd_pct = value - peak;
            drawdowns_pct[i] = dd_pct;

            if dd < max_dd {
                max_dd = dd;
                max_dd_index = i;
            }

            if dd_pct < max_dd_pct {
                max_dd_pct = dd_pct;
            }
        }

        (max_dd, max_dd_pct, drawdowns, drawdowns_pct, max_dd_index)
    }

    fn datetimes_for(len: usize) -> Vec<chrono::DateTime<chrono::Utc>> {
        let base = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        (0..len)
            .map(|i| base + chrono::Duration::minutes(i as i64))
            .collect()
    }

    #[test]
    fn calculate_final_monotonic_equity_has_zero_drawdown() {
        let mut pm = PerformanceManager::new(100.0, &test_strategy_settings());

        // Strictly increasing equity: nine values so the SIMD chunk loop runs twice.
        let equity = [
            100.0, 110.0, 125.0, 130.0, 150.0, 180.0, 200.0, 240.0, 260.0,
        ];
        let datetimes = datetimes_for(equity.len());

        pm.calculate_final(
            &equity,
            &datetimes,
            datetimes[0],
            *datetimes.last().unwrap(),
            1,
        );

        let metrics = pm.get_current_performance_metrics();
        assert_eq!(metrics.get_max_drawdown(), &0.0);
        assert_eq!(metrics.get_max_drawdown_pct(), &0.0);
        assert!(pm.get_drawdons().iter().all(|dd| *dd == 0.0));
        assert!(pm.get_drawdowns_pct().iter().all(|dd| *dd == 0.0));
    }

    #[test]
    fn simd_drawdowns_match_scalar_reference() {
        // Nontrivial series long enough to exercise two SIMD chunks plus the scalar tail.
        let equity = [
            100.0, 120.0, 108.0, 140.0, 96.0, 150.0, 130.0, 160.0, 120.0, 175.0, 90.0, 200.0,
        ];

        let simd = super::calculate_drawdowns_simd(&equity);
        let scalar = scalar_drawdowns(&equity);

        assert!(
            (simd.0 - scalar.0).abs() < 1e-12,
            "max dd {} {}",
            simd.0,
            scalar.0
        );
        assert!(
            (simd.1 - scalar.1).abs() < 1e-12,
            "max dd pct {} {}",
            simd.1,
            scalar.1
        );
        assert_eq!(simd.4, scalar.4);
        for i in 0..equity.len() {
            assert!(
                (simd.2[i] - scalar.2[i]).abs() < 1e-12,
                "drawdown mismatch at {i}: {} vs {}",
                simd.2[i],
                scalar.2[i]
            );
            assert!(
                (simd.3[i] - scalar.3[i]).abs() < 1e-12,
                "drawdown pct mismatch at {i}: {} vs {}",
                simd.3[i],
                scalar.3[i]
            );
        }
    }

    #[test]
    fn recovery_factor_is_signed() {
        // Both series peak at 150 and dip through 90 (drawdown), so only the net return differs.
        let profitable = [100.0, 120.0, 90.0, 150.0, 140.0];
        let losing = [100.0, 120.0, 90.0, 150.0, 60.0];

        let mut pm_win = PerformanceManager::new(100.0, &test_strategy_settings());
        let datetimes = datetimes_for(profitable.len());
        pm_win.calculate_final(
            &profitable,
            &datetimes,
            datetimes[0],
            *datetimes.last().unwrap(),
            1,
        );
        let rf_win = *pm_win
            .get_current_performance_metrics()
            .get_recovery_factor();

        let mut pm_loss = PerformanceManager::new(100.0, &test_strategy_settings());
        pm_loss.calculate_final(
            &losing,
            &datetimes,
            datetimes[0],
            *datetimes.last().unwrap(),
            1,
        );
        let rf_loss = *pm_loss
            .get_current_performance_metrics()
            .get_recovery_factor();

        // +40 over a 30 absolute drawdown, -40 over a 90 absolute drawdown.
        assert!((rf_win - 40.0 / 30.0).abs() < 1e-6, "rf_win = {rf_win}");
        assert!((rf_loss + 40.0 / 90.0).abs() < 1e-6, "rf_loss = {rf_loss}");
        assert!(rf_win > 0.0);
        assert!(rf_loss < 0.0);
    }

    #[test]
    fn composite_score_stays_finite_for_negative_recovery_factor() {
        // A signed Recovery Factor can drop below -1, where `ln(1 + rf)` is NaN.
        let metrics = PerformanceMetrics {
            apr_to_drawdown_ratio: 1.0,
            recovery_factor: -3.0,
            max_drawdown: -0.5,
            ..Default::default()
        };

        assert!(metrics.get_composite_score().is_finite());
    }

    /// A one-point equity curve is a valid result of a very short run. It used to return early and
    /// keep `PerformanceMetrics::default()`, losing the deal count and the drawdown timestamp.
    #[test]
    fn calculate_final_handles_a_single_point_curve() {
        let mut pm = PerformanceManager::new(100.0, &test_strategy_settings());

        let equity = [100.0];
        let datetimes = datetimes_for(1);

        pm.calculate_final(&equity, &datetimes, datetimes[0], datetimes[0], 7);

        // No interval to compound over: one zero return and zero drawdowns.
        assert_eq!(pm.returns, vec![0.0]);
        assert_eq!(pm.get_drawdons(), &vec![0.0]);
        assert_eq!(pm.get_drawdowns_pct(), &vec![0.0]);

        // The final metrics are computed, not left at their defaults.
        let metrics = pm.get_current_performance_metrics();
        assert_eq!(metrics.get_deals_count(), &7);
        assert_eq!(metrics.get_max_drawdown_datetime(), &Some(datetimes[0]));
        assert_eq!(metrics.get_total_return(), &0.0);
        assert_eq!(metrics.get_total_return_percent(), &0.0);
        assert_eq!(metrics.get_max_drawdown(), &0.0);
    }

    /// An empty curve still carries no information and must leave the metrics untouched.
    #[test]
    fn calculate_final_ignores_an_empty_curve() {
        let mut pm = PerformanceManager::new(100.0, &test_strategy_settings());
        let datetimes = datetimes_for(1);

        pm.calculate_final(&[], &[], datetimes[0], datetimes[0], 7);

        let metrics = pm.get_current_performance_metrics();
        assert_eq!(metrics.get_deals_count(), &0);
        assert_eq!(metrics.get_max_drawdown_datetime(), &None);
    }

    /// A zero previous equity inside a SIMD chunk must yield a zero return, exactly like the scalar
    /// tail does, instead of `Inf`/`NaN`.
    #[test]
    fn simd_returns_treat_a_zero_denominator_as_zero_return() {
        // n = 6: the single SIMD chunk covers indices 1..=4, so the zero at index 1 is one of its
        // denominators and index 5 is handled by the scalar tail.
        let equity = [100.0, 0.0, 50.0, 60.0, 70.0, 80.0];

        let returns = super::calculate_returns_simd(&equity);

        assert_eq!(returns.len(), equity.len());
        assert_eq!(returns[0], 0.0);
        assert_eq!(returns[1], -1.0);
        assert_eq!(
            returns[2], 0.0,
            "zero previous equity must produce a zero return, got {}",
            returns[2]
        );
        assert!((returns[3] - 0.2).abs() < 1e-12);
        assert!((returns[4] - (70.0 / 60.0 - 1.0)).abs() < 1e-12);
        assert!((returns[5] - (80.0 / 70.0 - 1.0)).abs() < 1e-12);
        assert!(
            returns.iter().all(|r| r.is_finite()),
            "returns must stay finite, got {:?}",
            returns
        );
    }

    /// The same zero in a SIMD chunk must not poison the headline metrics of a finished run.
    #[test]
    fn calculate_final_keeps_metrics_finite_with_a_zero_equity_point() {
        let mut pm = PerformanceManager::new(100.0, &test_strategy_settings());

        let equity = [100.0, 0.0, 50.0, 60.0, 70.0, 80.0];
        let datetimes = datetimes_for(equity.len());

        pm.calculate_final(
            &equity,
            &datetimes,
            datetimes[0],
            *datetimes.last().unwrap(),
            1,
        );

        assert!(pm.returns.iter().all(|r| r.is_finite()));
        let metrics = pm.get_current_performance_metrics();
        assert!(metrics.get_total_return().is_finite());
        assert!(metrics.get_total_return_percent().is_finite());
        assert!(metrics.get_apr().is_finite());
        assert!(metrics.get_max_drawdown().is_finite());
    }
}

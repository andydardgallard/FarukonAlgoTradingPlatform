// Farukon_2_0/src/optimizers.rs

//! Optimization engine for hyperparameter tuning.
//! Supports Grid Search (exhaustive) and Genetic Algorithm (evolutionary).
//! Uses Rayon for parallel evaluation of thousands of parameter combinations.

use crate::backtest;
use crate::portfolio;
use crate::execution;
use crate::data_handler;
use crate::strategy_loader;

#[derive(Debug, Clone)]
pub struct OptimizationRunner {
    mode: String,
    initial_capital_for_strategy: f64,
    strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    strategy_settings: farukon_core::settings::StrategySettings,
    grid_search_optimizer: farukon_core::optimization::GridSearchOptimizer,
}

impl OptimizationRunner {
    pub fn new(
        mode: &str,
        initial_capital_for_strategy: &f64,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>
    ) -> Self {
        let config = farukon_core::utils::parse_optimization_config(strategy_settings);
        let grid_search_optimizer = farukon_core::optimization::GridSearchOptimizer::new()
            .with_optimization_config(config);
        
        Self {
            mode: mode.to_string(),
            initial_capital_for_strategy: *initial_capital_for_strategy,
            strategy_instruments_info: strategy_instruments_info.clone(),
            strategy_settings: strategy_settings.clone(),
            grid_search_optimizer,
        }
    }

    pub fn get_grid_search_optimizer(&self) -> &farukon_core::optimization::GridSearchOptimizer {
        &self.grid_search_optimizer
    }

    pub fn run_grid_search(self, total_combinations: usize, combinations_to_grid_search: Vec<farukon_core::optimization::ParameterSet>) -> Vec<farukon_core::optimization::OptimizationResult> {
        // Runs Grid Search in parallel across all CPU cores.
        // Each parameter set is evaluated by running a full backtest.
        // Uses Atomic counter to track progress.

        let threads = self.strategy_settings.threads.unwrap_or(num_cpus::get());
        let mode = self.mode;

        if mode == "Debug" {
            println!("Starting grid search optimization:");
            println!("Strategy: {}", self.strategy_settings.strategy_name);
            println!("Configured threads: {:?}", threads);
            println!("Total combinations: {}", total_combinations);
        }

        let initial_capital = self.initial_capital_for_strategy;
        let strategy_settings = self.strategy_settings.clone();
        let strategy_instruments_info = self.strategy_instruments_info.clone();
        let pos_sizer_name = self.strategy_settings.pos_sizer_params.pos_sizer_name.clone();
        let pos_sizer_additional_params = self.strategy_settings.pos_sizer_params.pos_sizer_params.clone();
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("Failed to create thread pool");

        let results = pool.install(|| {
            self.grid_search_optimizer.run_optimization(
                move |params| {
                    let start_time = std::time::Instant::now();

                    let full_parameter_set = farukon_core::optimization::ParameterSet::new()
                        .with_strategy_params(params.get_strategy_params().clone())
                        .with_pos_sizer_name(pos_sizer_name.clone())
                        .with_pos_sizer_value(params.get_pos_sizer_value().clone())
                        .with_pos_sizer_additional_params(pos_sizer_additional_params.clone())
                        .with_slippage(params.get_slippage().clone());

                    let current_count = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    println!(
                        "# {} from {} {}",
                        current_count,
                        total_combinations,
                        full_parameter_set.format_for_display()
                    );

                    let test_settings = farukon_core::utils::create_stratagy_settings_from_params(
                        &strategy_settings,
                        &full_parameter_set,
                    );

                    let results = Self::run_backtest_with_settings(
                        &mode,
                        &initial_capital,
                        &test_settings,
                        &strategy_instruments_info
                    );
                    
                    println!("# {} from {} is done in {:.3} seconds ", current_count, total_combinations, start_time.elapsed().as_secs_f64());

                    farukon_core::optimization::OptimizationResult::new()
                        .with_parameters(params.clone())
                        .with_results(results)
                },
                threads,
                combinations_to_grid_search
            )
        });

        results
    }

    fn run_backtest_with_settings(
        mode: &String, 
        initial_capital_for_strategy: &f64,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info:  &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    ) -> farukon_core::performance::PerformanceMetrics {
        // Creates a full backtest environment for a single parameter set.
        // Used by Grid Search and Genetic Algorithm.

        let (event_sender, event_receiver) = std::sync::mpsc::channel::<Box<dyn farukon_core::event::Event>>();
        let data_handler: Box<dyn farukon_core::data_handler::DataHandler> = Box::new(
            data_handler::HistoricFlatBuffersDataHandlerZC::new_with_sequential_load(
                mode,
                event_sender.clone(),
                strategy_settings,
            ).expect("Failed to create data handler")
        );

        let dynamic_strategy: Box<strategy_loader::DynamicStratagy> = Box::new(
            strategy_loader::DynamicStratagy::load_from_path(
                mode,
                strategy_settings,
                strategy_instruments_info,
                &event_sender,
            ).expect("Failed to load dynamic strategy")
        );

        let portfolio: Box<dyn farukon_core::portfolio::PortfolioHandler> = Box::new(
            portfolio::Portfolio::new(
                mode,
                event_sender.clone(),
                strategy_settings,
                strategy_instruments_info,
                initial_capital_for_strategy,
            ).expect("Failed to create portfolio")
        );

        let execution_handler: Box<dyn farukon_core::execution::ExecutionHandler> = Box::new(
            execution::SimulatedExecutionHandler::new(
                event_sender.clone(),
            ).expect("Failed to create execution handler")
        );

        let mut backtest = backtest::Backtest::new(
            mode,
            strategy_settings,
            strategy_instruments_info,
            data_handler,
            event_receiver,
            dynamic_strategy,
            portfolio,
            execution_handler
        );

        backtest.simulate_trading().expect("Backtest failed").clone()

    }

    pub fn run_genetic_search(self, ga_params: &farukon_core::settings::GAParams) -> anyhow::Result<Vec<farukon_core::optimization::GAStatsPerGeneration>> {
        // Runs Genetic Algorithm optimization.
        // Uses fitness function to evaluate chromosomes.
        
        let ga_config = farukon_core::optimization::GAConfig::from_settings(ga_params);
        let opt_config = farukon_core::utils::parse_optimization_config(&self.strategy_settings);
        let mut ga = farukon_core::optimization::GeneticAlgorythm::new()
            .with_ga_config(ga_config.clone())
            .with_optimization_config(opt_config);
        
        let stats = ga.run(&self.strategy_settings.clone(), move |params| {
            let test_settings = &farukon_core::utils::create_stratagy_settings_from_params(&self.strategy_settings, params);
                let backtest_result = Self::run_backtest_with_settings(
                    &self.mode,
                    &self.initial_capital_for_strategy,
                    test_settings, 
                    &self.strategy_instruments_info,
                );
                self.calculate_fitness_score(&backtest_result, &ga_config)  
        })?;

        anyhow::Ok(stats)
    }

    fn calculate_fitness_score(
        &self,
        metrics: &farukon_core::performance::PerformanceMetrics,
        ga_config: &farukon_core::optimization::GAConfig,
    ) -> f64 {
        // Converts performance metrics into a scalar fitness score.
        // Supports max/min direction and composite metrics.

        let raw_fitness = match ga_config.get_fitness_metric() {
            farukon_core::settings::FitnessValue::TotalReturn => metrics.get_total_return(),
            farukon_core::settings::FitnessValue::TotalReturnPercent => metrics.get_total_return_percent(),
            farukon_core::settings::FitnessValue::APR => metrics.get_apr_percent(),
            farukon_core::settings::FitnessValue::MaxDD => metrics.get_max_drawdown(),
            farukon_core::settings::FitnessValue::MaxDDPercent => metrics.get_max_drawdown_percent(),
            farukon_core::settings::FitnessValue::AprDDFactor => metrics.get_apr_to_drawdown_ratio(),
            farukon_core::settings::FitnessValue::RecoveryFactor => metrics.get_recovery_factor(),
            farukon_core::settings::FitnessValue::RecoveryFactorPercent => metrics.get_recovery_factor_percent(),
            farukon_core::settings::FitnessValue::DealsCount => &(metrics.get_deals_count().clone() as f64),
            farukon_core::settings::FitnessValue::Composite { metrics: composite_metrics } => {
                &self.calculate_composite_score(metrics, composite_metrics)
            }
        };

       let fitness = match ga_config.get_fitness_direction().as_str() {
            "max" => *raw_fitness,
            "min" => -raw_fitness,
            _ => *raw_fitness,
        };

        fitness
    }

    fn calculate_composite_score(
        &self,
        metrics: &farukon_core::performance::PerformanceMetrics,
        composite_metrics: &[String],
    ) -> f64 {
        // Combines multiple metrics into a single score with equal weights.
        // Example: APR/DD_factor + Recovery_Factor + Deals_Count
        
        let mut total_score = 0.0;
        let weight = 1.0 / composite_metrics.len() as f64;

        for metric_name in composite_metrics {
            let metric_value = match metric_name.as_str() {
                "Total_Return" => metrics.get_total_return(),
                "Total_Return_%" => metrics.get_total_return_percent(), 
                "APR" => metrics.get_apr_percent(),
                "max_DD" => &(-metrics.get_max_drawdown()),
                "max_DD_%" => &(-metrics.get_max_drawdown_percent()),
                "APR/DD_factor" => metrics.get_apr_to_drawdown_ratio(),
                "Recovery_Factor" => metrics.get_recovery_factor(),
                "Recovery_Factor_%" => metrics.get_recovery_factor_percent(),
                "Deals_Count" => &(-(*metrics.get_deals_count() as f64)),
                _ => &0.0,
            };
            total_score += metric_value * weight;
        }

        total_score
    }

}

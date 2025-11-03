// Farukon_2_0/src/optimizers.rs

//! Optimization engine for hyperparameter tuning.
//! Supports Grid Search (exhaustive) and Genetic Algorithm (evolutionary).
//! Uses Rayon for parallel evaluation of thousands of parameter combinations.

use crate::backtest;
use crate::portfolio;
use crate::execution;
use crate::data_handler;
use crate::strategy_loader; // Note: Typo in module name — should be "strategy_loader"

#[derive(Debug, Clone)]
/// Orchestrates the optimization process for a single strategy.
/// Manages configuration, runs Grid Search or Genetic Algorithm, and evaluates parameter sets.
pub struct OptimizationRunner {
    /// Operational mode (e.g., "Debug", "Optimize", "Visual").
    mode: String,
    /// Initial capital allocated to this specific strategy for optimization runs.
    initial_capital_for_strategy: f64,
    /// Instrument metadata (e.g., margin, step, exchange) for all symbols traded by the strategy.
    strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    /// Settings specific to the strategy being optimized (parameters, paths, etc.).
    strategy_settings: farukon_core::settings::StrategySettings,
    /// The Grid Search optimizer instance, configured based on strategy settings.
    grid_search_optimizer: farukon_core::optimization::GridSearchOptimizer,
}

impl OptimizationRunner {
    /// Creates a new `OptimizationRunner` instance.
    /// Initializes the Grid Search optimizer based on the provided strategy settings.
    /// # Arguments
    /// * `mode` - The operational mode (affects verbosity and behavior).
    /// * `initial_capital_for_strategy` - The starting capital for backtests within this optimization run.
    /// * `strategy_settings` - The configuration for the strategy being optimized.
    /// * `strategy_instruments_info` - Metadata for all instruments traded by the strategy.
    /// # Returns
    /// * `OptimizationRunner` - The newly created runner instance.
    pub fn new(
        mode: &str,
        initial_capital_for_strategy: &f64,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>
    ) -> Self {
        // Parse the strategy settings to extract the ranges for parameters to be optimized.
        let config = farukon_core::utils::parse_optimization_config(strategy_settings);
        // Create the Grid Search optimizer and configure it with the extracted ranges.
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

    /// Provides access to the internal Grid Search optimizer.
    /// # Returns
    /// * A reference to the `GridSearchOptimizer`.
    pub fn get_grid_search_optimizer(&self) -> &farukon_core::optimization::GridSearchOptimizer {
        &self.grid_search_optimizer
    }

    /// Executes a Grid Search optimization.
    /// Evaluates all parameter combinations in parallel using Rayon.
    /// Each combination triggers a full backtest run.
    /// # Arguments
    /// * `total_combinations` - The total number of parameter sets to evaluate.
    /// * `combinations_to_grid_search` - A vector of `ParameterSet` objects to test.
    /// # Returns
    /// * A vector of `OptimizationResult` objects, one for each evaluated parameter set.
    pub fn run_grid_search(self, total_combinations: usize, combinations_to_grid_search: Vec<farukon_core::optimization::ParameterSet>) -> Vec<farukon_core::optimization::OptimizationResult> {
        // Runs Grid Search in parallel across all CPU cores.
        // Each parameter set is evaluated by running a full backtest.
        // Uses Atomic counter to track progress.

        // Determine the number of threads to use for parallel execution.
        // Defaults to the number of logical CPU cores if not specified in settings.
        let threads = self.strategy_settings.threads.unwrap_or(num_cpus::get());
        let mode = self.mode;

        if mode == "Debug" {
            println!("Starting grid search optimization:");
            println!("Strategy: {}", self.strategy_settings.strategy_name);
            println!("Configured threads: {:?}", threads);
            println!("Total combinations: {}", total_combinations);
        }

        // Capture necessary data for the parallel execution closure.
        let initial_capital = self.initial_capital_for_strategy;
        let strategy_settings = self.strategy_settings.clone();
        let strategy_instruments_info = self.strategy_instruments_info.clone();
        let pos_sizer_name = self.strategy_settings.pos_sizer_params.pos_sizer_name.clone();
        let pos_sizer_additional_params = self.strategy_settings.pos_sizer_params.pos_sizer_params.clone();
        // Shared atomic counter to track the number of completed evaluations.
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // Create a Rayon thread pool with the specified number of threads.
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("Failed to create thread pool");

        // Execute the optimization within the thread pool.
        let results = pool.install(|| {
            self.grid_search_optimizer.run_optimization(
                // The fitness function executed for each parameter set.
                // It runs a backtest and returns an OptimizationResult.
                move |params| {
                    let start_time = std::time::Instant::now();

                    // Construct the full parameter set object, including non-strategy parameters like pos_sizer and slippage.
                    let full_parameter_set = farukon_core::optimization::ParameterSet::new()
                        .with_strategy_params(params.get_strategy_params().clone())
                        .with_pos_sizer_name(pos_sizer_name.clone())
                        .with_pos_sizer_value(params.get_pos_sizer_value().clone())
                        .with_pos_sizer_additional_params(pos_sizer_additional_params.clone())
                        .with_slippage(params.get_slippage().clone());

                    // Increment the counter and get the current count for logging.
                    let current_count = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    println!(
                        "# {} from {} {}",
                        current_count,
                        total_combinations,
                        full_parameter_set.format_for_display() // Human-readable representation of the parameters.
                    );

                    // Create temporary strategy settings based on the current parameter set.
                    let test_settings = farukon_core::utils::create_stratagy_settings_from_params(
                        &strategy_settings,
                        &full_parameter_set,
                    );

                    // Run a full backtest using the current parameter set.
                    let results = Self::run_backtest_with_settings(
                        &mode,
                        &initial_capital,
                        &test_settings,
                        &strategy_instruments_info
                    );
                    
                    println!("# {} from {} is done in {:.3} seconds ", current_count, total_combinations, start_time.elapsed().as_secs_f64());

                    // Create an OptimizationResult object containing the parameters and the resulting performance metrics.
                    farukon_core::optimization::OptimizationResult::new()
                        .with_parameters(params.clone())
                        .with_results(results)
                },
                threads, // Number of threads to use for the optimization.
                combinations_to_grid_search // Vector of parameter sets to evaluate.
            )
        });

        results
    }

    /// Executes a single backtest run with a given set of strategy parameters.
    /// This is a helper function used by both Grid Search and Genetic Algorithm.
    /// # Arguments
    /// * `mode` - The operational mode.
    /// * `initial_capital_for_strategy` - The starting capital for this backtest run.
    /// * `strategy_settings` - The strategy settings, potentially modified with new parameters.
    /// * `strategy_instruments_info` - Metadata for the instruments traded.
    /// # Returns
    /// * `farukon_core::performance::PerformanceMetrics` - The performance metrics from the completed backtest.
    fn run_backtest_with_settings(
        mode: &String, 
        initial_capital_for_strategy: &f64,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info:  &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    ) -> farukon_core::performance::PerformanceMetrics {
        // Creates a full backtest environment for a single parameter set.
        // Used by Grid Search and Genetic Algorithm.

        // Create the event channel used for communication between components (DataHandler, Strategy, Portfolio, Execution).
        let (event_sender, event_receiver) = std::sync::mpsc::channel::<Box<dyn farukon_core::event::Event>>();

        // Initialize the data handler (uses zero-copy FlatBuffers).
        let data_handler: Box<dyn farukon_core::data_handler::DataHandler> = Box::new(
            data_handler::HistoricFlatBuffersDataHandlerZC::new_with_sequential_load(
                mode,
                event_sender.clone(),
                strategy_settings,
            ).expect("Failed to create data handler")
        );

        // Load the dynamic strategy library (.so/.dylib) specified in settings.
        let dynamic_strategy: Box<strategy_loader::DynamicStratagy> = Box::new(
            strategy_loader::DynamicStratagy::load_from_path(
                mode,
                strategy_settings,
                strategy_instruments_info,
                &event_sender,
            ).expect("Failed to load dynamic strategy")
        );

        // Initialize the portfolio manager.
        let portfolio: Box<dyn farukon_core::portfolio::PortfolioHandler> = Box::new(
            portfolio::Portfolio::new(
                mode,
                initial_capital_for_strategy,
                event_sender.clone(),
                strategy_settings,
                strategy_instruments_info,
            ).expect("Failed to create portfolio")
        );

        // Initialize the simulated execution handler.
        let execution_handler: Box<dyn farukon_core::execution::ExecutionHandler> = Box::new(
            execution::SimulatedExecutionHandler::new(
                event_sender.clone(),
            ).expect("Failed to create execution handler")
        );

        // Create the main backtest controller.
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

        // Run the backtest simulation and return the final performance metrics.
        backtest.simulate_trading().expect("Backtest failed").clone()

    }

    /// Executes a Genetic Algorithm optimization.
    /// Iteratively evolves a population of parameter sets based on their fitness.
    /// # Arguments
    /// * `ga_params` - Configuration parameters for the Genetic Algorithm (population size, mutation rate, etc.).
    /// # Returns
    /// * `anyhow::Result<Vec<farukon_core::optimization::GAStatsPerGeneration>>` - Statistics for each generation or an error.
    pub fn run_genetic_search(self, ga_params: &farukon_core::settings::GAParams) -> anyhow::Result<Vec<farukon_core::optimization::GAStatsPerGeneration>> {
        // Runs Genetic Algorithm optimization.
        // Uses fitness function to evaluate chromosomes.
        
        // Configure the Genetic Algorithm based on the provided parameters.
        let ga_config = farukon_core::optimization::GAConfig::from_settings(ga_params);
        // Get the optimization configuration (parameter ranges) for the strategy.
        let opt_config = farukon_core::utils::parse_optimization_config(&self.strategy_settings);
        // Create the Genetic Algorithm instance.
        let mut ga = farukon_core::optimization::GeneticAlgorythm::new()
            .with_ga_config(ga_config.clone())
            .with_optimization_config(opt_config);
        
        // Run the Genetic Algorithm, providing a fitness function that evaluates parameter sets.
        let stats = ga.run(&self.strategy_settings.clone(), move |params| {
            // Create temporary strategy settings based on the current parameter set for this generation.
            let test_settings = &farukon_core::utils::create_stratagy_settings_from_params(&self.strategy_settings, params);
            // Run a backtest with these parameters.
            let backtest_result = Self::run_backtest_with_settings(
                &self.mode,
                &self.initial_capital_for_strategy,
                test_settings, 
                &self.strategy_instruments_info,
            );
            // Calculate the fitness score based on the backtest results.
            self.calculate_fitness_score(&backtest_result, &ga_config)  
        })?;

        anyhow::Ok(stats)
    }

    /// Calculates a scalar fitness score from performance metrics.
    /// The score is used by the Genetic Algorithm to rank parameter sets.
    /// # Arguments
    /// * `metrics` - The performance metrics from a backtest run.
    /// * `ga_config` - Configuration specifying the fitness metric and direction (maximize/minimize).
    /// # Returns
    /// * `f64` - The calculated fitness score.
    fn calculate_fitness_score(
        &self,
        metrics: &farukon_core::performance::PerformanceMetrics,
        ga_config: &farukon_core::optimization::GAConfig,
    ) -> f64 {
        // Converts performance metrics into a scalar fitness score.
        // Supports max/min direction and composite metrics.

        // Determine which metric to use for the raw fitness score.
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
                // For composite metrics, calculate a combined score.
                &self.calculate_composite_score(metrics, composite_metrics)
            }
        };

        // Apply the fitness direction (maximize or minimize).
        // If direction is "min", the score is negated.
        let fitness = match ga_config.get_fitness_direction().as_str() {
            "max" => *raw_fitness,
            "min" => -raw_fitness,
            _ => *raw_fitness, // Default to "max" if direction is unknown.
        };

        fitness
    }

    /// Combines multiple performance metrics into a single composite fitness score.
    /// Each metric contributes equally (simple average).
    /// # Arguments
    /// * `metrics` - The performance metrics from a backtest run.
    /// * `composite_metrics` - A list of metric names to include in the composite score.
    /// # Returns
    /// * `f64` - The calculated composite score.
    fn calculate_composite_score(
        &self,
        metrics: &farukon_core::performance::PerformanceMetrics,
        composite_metrics: &[String],
    ) -> f64 {
        // Combines multiple metrics into a single score with equal weights.
        // Example: APR/DD_factor + Recovery_Factor + Deals_Count
        
        let mut total_score = 0.0;
        // Calculate the weight for each metric (1 / number of metrics).
        let weight = 1.0 / composite_metrics.len() as f64;

        for metric_name in composite_metrics {
            // Get the value for the current metric name.
            let metric_value = match metric_name.as_str() {
                "Total_Return" => metrics.get_total_return(),
                "Total_Return_%" => metrics.get_total_return_percent(), 
                "APR" => metrics.get_apr_percent(),
                "max_DD" => &(-metrics.get_max_drawdown()), // Negative drawdown for maximization (smaller loss is better)
                "max_DD_%" => &(-metrics.get_max_drawdown_percent()),
                "APR/DD_factor" => metrics.get_apr_to_drawdown_ratio(),
                "Recovery_Factor" => metrics.get_recovery_factor(),
                "Recovery_Factor_%" => metrics.get_recovery_factor_percent(),
                "Deals_Count" => &(-(*metrics.get_deals_count() as f64)), // Negative count for maximization (fewer trades might be better depending on context, but often more is desired, this might need review)
                _ => &0.0, // Default to 0 if the metric name is unknown.
            };
            // Add the weighted value of this metric to the total score.
            total_score += metric_value * weight;
        }

        total_score
    }

}

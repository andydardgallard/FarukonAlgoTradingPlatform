//! Farukon_2/src/optimizers.rs

//! Optimization engine for hyperparameter tuning.
//! Supports Grid Search (exhaustive) and Genetic Algorithm (evolutionary).
//! Uses Rayon for parallel evaluation of thousands of parameter combinations.

use ::std::io::Write;

use anyhow::Context;

use crate::backtest;
use crate::data_engine;
use crate::execution;
use crate::portfolio;
use crate::strategy_loader;

#[derive(Debug, Clone)]
/// Orchestrates the optimization process for a single strategy.
/// Manages configuration, runs Grid Search or Genetic Algorithm, and evaluates parameter sets.
pub struct OptimizationRunner {
    /// Initial capital allocated to this specific strategy for optimization runs.
    initial_capital_for_strategy: f64,

    /// Operational mode (e.g., "Debug", "Optimize", "Visual").
    common_settings: farukon_core::settings::CommonSettings,

    /// Settings specific to the strategy being optimized (parameters, paths, etc.).
    strategy_settings: farukon_core::settings::StrategySettings,

    /// Instrument metadata (e.g., margin, step, exchange) for all symbols traded by the strategy.
    strategy_instruments_info:
        std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,

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
        initial_capital_for_strategy: &f64,
        common_settings: &farukon_core::settings::CommonSettings,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<
            String,
            farukon_core::instruments_info::InstrumentInfo,
        >,
    ) -> Self {
        // Parse the strategy settings to extract the ranges for parameters to be optimized.
        let config = farukon_core::utils::parse_optimization_config(strategy_settings);

        // Create the Grid Search optimizer and configure it with the extracted ranges.
        let grid_search_optimizer =
            farukon_core::optimization::GridSearchOptimizer::new().with_optimization_config(config);

        Self {
            initial_capital_for_strategy: *initial_capital_for_strategy,
            common_settings: common_settings.clone(),
            strategy_settings: strategy_settings.clone(),
            strategy_instruments_info: strategy_instruments_info.clone(),
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
    /// * `anyhow::Result<Vec<farukon_core::optimization::OptimizationResult>>` - One result per
    ///   successfully evaluated parameter set, or the first candidate failure (a failed candidate
    ///   is never reported as a silent placeholder).
    pub fn run_grid_search(
        &self,
        total_combinations: u128,
        global_data_store: std::sync::Arc<data_engine::global_data_storage::GlobalDataStore>,
    ) -> anyhow::Result<Vec<farukon_core::optimization::OptimizationResult>> {
        // Runs Grid Search in parallel across all CPU cores.
        // Each parameter set is evaluated by running a full backtest.
        // Uses Atomic counter to track progress.

        self.grid_search_optimizer
            .check_memory_limit(self.grid_search_optimizer.get_config())?;

        // Determine the number of threads to use for parallel execution.
        // Defaults to the number of logical CPU cores if not specified in settings.
        let threads = self.strategy_settings.threads.unwrap_or(num_cpus::get());
        let global_data_mode = self.common_settings.global_data_storage_mode.clone();
        let mode = self.common_settings.mode.clone();

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

        // Shared atomic counter to track the number of completed evaluations.
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let lib_path = &strategy_settings.strategy_path;
        let lib = std::sync::Arc::new(
            unsafe { libloading::Library::new(lib_path) }
                .with_context(|| format!("failed to load the strategy library {}", lib_path))?,
        );

        // Create a Rayon thread pool with the specified number of threads.
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .context("failed to create the grid search thread pool")?;

        // First failure of a candidate evaluation; it fails the whole search after the run.
        let candidate_error: std::sync::Arc<std::sync::Mutex<Option<anyhow::Error>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let candidate_error_worker = std::sync::Arc::clone(&candidate_error);

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
                        .with_pos_sizer_name(params.get_pos_sizer_name().clone())
                        .with_pos_sizer_value(*params.get_pos_sizer_value())
                        .with_pos_sizer_additional_params(
                            params.get_pos_sizer_additional_params().clone(),
                        )
                        .with_slippage(*params.get_slippage());

                    // Increment the counter and get the current count for logging.
                    let current_count =
                        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    println!(
                        "# {} from {} {}",
                        current_count,
                        total_combinations,
                        full_parameter_set.format_for_display() // Human-readable representation of the parameters.
                    );

                    // Once a candidate failed the search is already doomed: skip the remaining
                    // backtests instead of spending time on work that will be discarded.
                    if candidate_error_worker.lock().unwrap().is_some() {
                        return farukon_core::optimization::OptimizationResult::new()
                            .with_parameters(params.clone());
                    }

                    // Create temporary strategy settings based on the current parameter set.
                    let test_settings = farukon_core::utils::create_stratagy_settings_from_params(
                        &strategy_settings,
                        &full_parameter_set,
                    );

                    // Run a full backtest using the current parameter set.
                    let results = Self::evaluate_grid_candidate(
                        &full_parameter_set,
                        &candidate_error_worker,
                        || match global_data_mode.as_str() {
                            "arc" => Self::run_backtest_with_settings_arc(
                                &mode,
                                &initial_capital,
                                &test_settings,
                                &strategy_instruments_info,
                                std::sync::Arc::clone(&global_data_store),
                                lib.clone(),
                                &full_parameter_set,
                            ),
                            _ => Self::run_backtest_with_settings_deep(
                                &mode,
                                &initial_capital,
                                &test_settings,
                                &strategy_instruments_info,
                                std::sync::Arc::clone(&global_data_store),
                                lib.clone(),
                                &full_parameter_set,
                            ),
                        },
                    );

                    // A failed candidate is reported by `finish_grid_search`; the placeholder
                    // below is dropped there together with the error.
                    let Some(results) = results else {
                        eprintln!(
                            "# {} from {} failed to evaluate; see the reported error",
                            current_count, total_combinations
                        );
                        return farukon_core::optimization::OptimizationResult::new()
                            .with_parameters(params.clone());
                    };

                    println!(
                        "# {} from {} is done in {:.3} seconds.",
                        current_count,
                        total_combinations,
                        start_time.elapsed().as_secs_f64()
                    );

                    // Create an OptimizationResult object containing the parameters and the resulting performance metrics.
                    farukon_core::optimization::OptimizationResult::new()
                        .with_parameters(params.clone())
                        .with_results(results)
                },
                threads, // Number of threads to use for the optimization.
                self.grid_search_optimizer.get_config(),
            )
        });

        // Report the first candidate failure instead of returning placeholder results.
        Self::finish_grid_search(results, &candidate_error)
    }

    /// Evaluates one Grid Search candidate and remembers the first failure.
    ///
    /// The optimizer core requires a value for every candidate, so a failing candidate yields
    /// `None`; its placeholder is dropped by `finish_grid_search`, which turns the remembered
    /// failure into the error of the whole search.
    fn evaluate_grid_candidate<F>(
        candidate: &farukon_core::optimization::ParameterSet,
        candidate_error: &std::sync::Mutex<Option<anyhow::Error>>,
        evaluate: F,
    ) -> Option<farukon_core::performance::PerformanceMetrics>
    where
        F: FnOnce() -> anyhow::Result<farukon_core::performance::PerformanceMetrics>,
    {
        match evaluate() {
            Ok(metrics) => Some(metrics),
            Err(error) => {
                let mut slot = candidate_error.lock().unwrap();
                if slot.is_none() {
                    *slot = Some(error.context(Self::candidate_error_context(
                        "grid search candidate failed",
                        candidate,
                    )));
                }
                None
            }
        }
    }

    /// Converts the raw Grid Search output into a result set: the first candidate failure wins, so
    /// a failed candidate can never be reported as a silent placeholder.
    fn finish_grid_search(
        results: Vec<farukon_core::optimization::OptimizationResult>,
        candidate_error: &std::sync::Mutex<Option<anyhow::Error>>,
    ) -> anyhow::Result<Vec<farukon_core::optimization::OptimizationResult>> {
        let candidate_error = candidate_error.lock().unwrap().take();
        match candidate_error {
            Some(error) => Err(error),
            None => anyhow::Ok(results),
        }
    }

    /// Executes a single backtest run with a given set of strategy parameters.
    /// This is a helper function used by both Grid Search and Genetic Algorithm.
    /// # Arguments
    /// * `mode` - The operational mode.
    /// * `initial_capital_for_strategy` - The starting capital for this backtest run.
    /// * `strategy_settings` - The strategy settings, potentially modified with new parameters.
    /// * `strategy_instruments_info` - Metadata for the instruments traded.
    /// # Returns
    /// * `anyhow::Result<farukon_core::performance::PerformanceMetrics>` - The performance metrics
    ///   from the completed backtest, or an error carrying the failing candidate's parameters.
    fn run_backtest_with_settings_arc(
        mode: &String,
        initial_capital_for_strategy: &f64,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<
            String,
            farukon_core::instruments_info::InstrumentInfo,
        >,
        global_data_store: std::sync::Arc<data_engine::global_data_storage::GlobalDataStore>,
        strategy_library: std::sync::Arc<libloading::Library>,
        candidate: &farukon_core::optimization::ParameterSet,
    ) -> anyhow::Result<farukon_core::performance::PerformanceMetrics> {
        // Creates a full backtest environment for a single parameter set.
        // Used by Grid Search and Genetic Algorithm.

        // Create the event channel used for communication between components (DataHandler, Strategy, Portfolio, Execution).
        let (event_sender, event_receiver) =
            std::sync::mpsc::channel::<Box<dyn farukon_core::event::Event>>();

        // Initialize the data handler (uses zero-copy FlatBuffers).
        let data_handler: Box<dyn farukon_core::data_handler::DataHandler> = Box::new(
            data_engine::data_handler_arc::SOADataHandlerArc::new(global_data_store.clone()),
        );

        // Load the dynamic strategy library (.so/.dylib) specified in settings.
        let dynamic_strategy: Box<strategy_loader::DynamicStratagy> = Box::new(
            strategy_loader::DynamicStratagy::new_from_library(
                mode,
                strategy_settings,
                strategy_instruments_info,
                &event_sender,
                strategy_library.clone(),
            )
            .with_context(|| {
                Self::candidate_error_context("failed to construct the strategy", candidate)
            })?,
        );

        // Initialize the portfolio manager.
        let portfolio: Box<dyn farukon_core::portfolio::PortfolioHandler> = Box::new(
            portfolio::Portfolio::new(
                mode,
                initial_capital_for_strategy,
                event_sender.clone(),
                strategy_settings,
                strategy_instruments_info,
            )
            .with_context(|| {
                Self::candidate_error_context("failed to create the portfolio", candidate)
            })?,
        );

        // Initialize the simulated execution handler.
        let execution_handler: Box<dyn farukon_core::execution::ExecutionHandler> = Box::new(
            execution::SimulatedExecutionHandler::new(event_sender.clone()).with_context(|| {
                Self::candidate_error_context("failed to create the execution handler", candidate)
            })?,
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
            execution_handler,
        );

        // Run the backtest simulation and return the final performance metrics.
        let metrics = backtest
            .simulate_trading()
            .with_context(|| Self::candidate_error_context("backtest failed", candidate))?;
        anyhow::Ok(metrics.clone())
    }

    /// Executes a single backtest in `deep` data mode (per-candidate dataset clone).
    /// # Returns
    /// * `anyhow::Result<farukon_core::performance::PerformanceMetrics>` - The performance metrics
    ///   from the completed backtest, or an error carrying the failing candidate's parameters.
    fn run_backtest_with_settings_deep(
        mode: &String,
        initial_capital_for_strategy: &f64,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<
            String,
            farukon_core::instruments_info::InstrumentInfo,
        >,
        global_data_store: std::sync::Arc<data_engine::global_data_storage::GlobalDataStore>,
        strategy_library: std::sync::Arc<libloading::Library>,
        candidate: &farukon_core::optimization::ParameterSet,
    ) -> anyhow::Result<farukon_core::performance::PerformanceMetrics> {
        // Creates a full backtest environment for a single parameter set.
        // Used by Grid Search and Genetic Algorithm.

        // Create the event channel used for communication between components (DataHandler, Strategy, Portfolio, Execution).
        let (event_sender, event_receiver) =
            std::sync::mpsc::channel::<Box<dyn farukon_core::event::Event>>();

        // Initialize the data handler (uses zero-copy FlatBuffers).
        let data_handler: Box<dyn farukon_core::data_handler::DataHandler> = Box::new(
            data_engine::data_handler::SOADataHandler::new(global_data_store),
        );

        // Load the dynamic strategy library (.so/.dylib) specified in settings.
        let dynamic_strategy: Box<strategy_loader::DynamicStratagy> = Box::new(
            strategy_loader::DynamicStratagy::new_from_library(
                mode,
                strategy_settings,
                strategy_instruments_info,
                &event_sender,
                strategy_library.clone(),
            )
            .with_context(|| {
                Self::candidate_error_context("failed to construct the strategy", candidate)
            })?,
        );

        // Initialize the portfolio manager.
        let portfolio: Box<dyn farukon_core::portfolio::PortfolioHandler> = Box::new(
            portfolio::Portfolio::new(
                mode,
                initial_capital_for_strategy,
                event_sender.clone(),
                strategy_settings,
                strategy_instruments_info,
            )
            .with_context(|| {
                Self::candidate_error_context("failed to create the portfolio", candidate)
            })?,
        );

        // Initialize the simulated execution handler.
        let execution_handler: Box<dyn farukon_core::execution::ExecutionHandler> = Box::new(
            execution::SimulatedExecutionHandler::new(event_sender.clone()).with_context(|| {
                Self::candidate_error_context("failed to create the execution handler", candidate)
            })?,
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
            execution_handler,
        );

        // Run the backtest simulation and return the final performance metrics.
        let metrics = backtest
            .simulate_trading()
            .with_context(|| Self::candidate_error_context("backtest failed", candidate))?;
        anyhow::Ok(metrics.clone())
    }

    /// Builds the error message attached to every failure of one optimization candidate.
    fn candidate_error_context(
        action: &str,
        candidate: &farukon_core::optimization::ParameterSet,
    ) -> String {
        format!(
            "{} for candidate {}",
            action,
            candidate.format_for_display()
        )
    }

    /// Fitness assigned to a candidate whose evaluation failed: the worst possible value for the
    /// configured direction, so ranking and selection never prefer it.
    fn worst_fitness(fitness_direction: &str) -> f64 {
        if fitness_direction == "min" {
            f64::INFINITY
        } else {
            f64::NEG_INFINITY
        }
    }

    /// Executes a Genetic Algorithm optimization.
    /// Iteratively evolves a population of parameter sets based on their fitness.
    /// # Arguments
    /// * `ga_params` - Configuration parameters for the Genetic Algorithm (population size, mutation rate, etc.).
    /// # Returns
    /// * `anyhow::Result<Vec<farukon_core::optimization::GAStatsPerGeneration>>` - Statistics for each generation or an error.
    pub fn run_genetic_search(
        self,
        ga_params: &farukon_core::settings::GAParams,
        global_data_store: std::sync::Arc<data_engine::global_data_storage::GlobalDataStore>,
    ) -> anyhow::Result<Vec<farukon_core::optimization::GAStatsPerGeneration>> {
        // Runs Genetic Algorithm optimization.
        // Uses fitness function to evaluate chromosomes.

        // Configure the Genetic Algorithm based on the provided parameters.
        let ga_config = farukon_core::optimization::GAConfig::from_settings(ga_params);
        // Get the optimization configuration (parameter ranges) for the strategy.
        let opt_config = self.get_grid_search_optimizer().get_config();
        // Create the Genetic Algorithm instance.
        let mut ga = farukon_core::optimization::GeneticAlgorythm::new()
            .with_ga_config(ga_config.clone())
            .with_optimization_config(opt_config.clone());

        let lib_path = &self.strategy_settings.strategy_path;
        let lib = std::sync::Arc::new(
            unsafe { libloading::Library::new(lib_path) }
                .with_context(|| format!("failed to load the strategy library {}", lib_path))?,
        );

        let initial_capital_for_strategy = self.initial_capital_for_strategy;
        let common_settings = self.common_settings.clone();
        let strategy_settings = self.strategy_settings.clone();
        let strategy_instruments_info = self.strategy_instruments_info.clone();
        let ga_config_closure = ga_config.clone();

        let all_ga_results_shared = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let all_ga_results_clone = std::sync::Arc::clone(&all_ga_results_shared);

        // First failure of a candidate evaluation; reported once the optimizer returns.
        let candidate_error: std::sync::Arc<std::sync::Mutex<Option<anyhow::Error>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let candidate_error_worker = std::sync::Arc::clone(&candidate_error);

        // Run the Genetic Algorithm, providing a fitness function that evaluates parameter sets.
        let stats = ga.run(&strategy_settings.clone(), move |params| {
            let mode = &common_settings.mode;
            let global_data_storage_mode = &common_settings.global_data_storage_mode;
            let fitness_direction = ga_config_closure.get_fitness_direction().as_str();

            // Once a candidate failed the run is already compromised: skip the remaining
            // backtests and report the stored error instead of running garbage.
            if candidate_error_worker.lock().unwrap().is_some() {
                return Self::worst_fitness(fitness_direction);
            }

            // Create temporary strategy settings based on the current parameter set for this generation.
            let test_settings = &farukon_core::utils::create_stratagy_settings_from_params(
                &strategy_settings,
                params,
            );
            // Run a backtest with these parameters.
            let results = match global_data_storage_mode.as_str() {
                "arc" => Self::run_backtest_with_settings_arc(
                    mode,
                    &initial_capital_for_strategy,
                    test_settings,
                    &strategy_instruments_info,
                    std::sync::Arc::clone(&global_data_store),
                    lib.clone(),
                    params,
                ),
                _ => Self::run_backtest_with_settings_deep(
                    mode,
                    &initial_capital_for_strategy,
                    test_settings,
                    &strategy_instruments_info,
                    std::sync::Arc::clone(&global_data_store),
                    lib.clone(),
                    params,
                ),
            };
            let results = match results {
                Ok(results) => results,
                Err(error) => {
                    *candidate_error_worker.lock().unwrap() = Some(error);
                    return Self::worst_fitness(fitness_direction);
                }
            };

            // Create an OptimizationResult object containing the parameters and the resulting performance metrics.
            let fitness =
                Self::calculate_fitness_score(&results, ga_config_closure.get_fitness_metric());
            let results_obj = farukon_core::optimization::OptimizationResult::new()
                .with_parameters(params.clone())
                .with_results(results.clone());

            {
                let mut results_guard = all_ga_results_clone.lock().unwrap();
                results_guard.push(results_obj);
            }

            match fitness {
                Ok(fitness) => fitness,
                Err(error) => {
                    *candidate_error_worker.lock().unwrap() = Some(error);
                    Self::worst_fitness(fitness_direction)
                }
            }
        })?;

        let final_results = {
            let results_guard = all_ga_results_shared.lock().unwrap();
            results_guard.clone()
        };

        // save results
        self.save_grid_search_optimization_results(&final_results)?;

        // A failed candidate invalidates the generation statistics: report it instead of
        // writing misleading numbers.
        let candidate_error = candidate_error.lock().unwrap().take();
        if let Some(error) = candidate_error {
            return Err(error.context("genetic search aborted: a candidate failed"));
        }

        ga.save_generation_stats_to_csv(&stats, &self.strategy_settings)?;

        anyhow::Ok(stats)
    }

    /// Calculates the raw scalar fitness value from performance metrics.
    /// The score is used by the Genetic Algorithm and by LSHADE-RSP to rank parameter sets.
    ///
    /// The value is returned untransformed: the optimizers apply `fitness_direction`
    /// ("max"/"min") themselves when ranking, selecting and reporting individuals.
    /// # Arguments
    /// * `metrics` - The performance metrics from a backtest run.
    /// * `fitness_metric` - The configured metric to extract.
    /// # Returns
    /// * `anyhow::Result<f64>` - The raw metric value, or an error for an unknown composite metric.
    fn calculate_fitness_score(
        metrics: &farukon_core::performance::PerformanceMetrics,
        fitness_metric: &farukon_core::settings::FitnessValue,
    ) -> anyhow::Result<f64> {
        let raw_fitness = match fitness_metric {
            farukon_core::settings::FitnessValue::TotalReturn => *metrics.get_total_return(),
            farukon_core::settings::FitnessValue::TotalReturnPercent => {
                *metrics.get_total_return_percent()
            }
            farukon_core::settings::FitnessValue::APR => *metrics.get_apr(),
            farukon_core::settings::FitnessValue::MaxDD => *metrics.get_max_drawdown(),
            farukon_core::settings::FitnessValue::AprDDFactor => {
                *metrics.get_apr_to_drawdown_ratio()
            }
            farukon_core::settings::FitnessValue::RecoveryFactor => *metrics.get_recovery_factor(),
            farukon_core::settings::FitnessValue::DealsCount => *metrics.get_deals_count() as f64,
            farukon_core::settings::FitnessValue::Composite {
                metrics: composite_metrics,
            } => {
                // For composite metrics, calculate a combined score.
                Self::calculate_composite_score(metrics, composite_metrics)?
            }
        };

        anyhow::Ok(raw_fitness)
    }

    /// Combines multiple performance metrics into a single composite fitness score.
    /// Each metric contributes equally (simple average).
    /// # Arguments
    /// * `metrics` - The performance metrics from a backtest run.
    /// * `composite_metrics` - A list of metric names to include in the composite score.
    /// # Returns
    /// * `anyhow::Result<f64>` - The calculated composite score, or an error for an unknown
    ///   metric name (an unknown name must never silently contribute zero).
    fn calculate_composite_score(
        metrics: &farukon_core::performance::PerformanceMetrics,
        composite_metrics: &[String],
    ) -> anyhow::Result<f64> {
        // Combines multiple metrics into a single score with equal weights.
        // Example: APR/DD_factor + Recovery_Factor + Deals_Count

        let mut total_score = 0.0;
        // Calculate the weight for each metric (1 / number of metrics).
        let weight = 1.0 / composite_metrics.len() as f64;

        for metric_name in composite_metrics {
            // Get the value for the current metric name.
            let metric_value = match metric_name.as_str() {
                "Total_Return" => *metrics.get_total_return(),
                "Total_Return_%" => *metrics.get_total_return_percent(),
                "APR" => *metrics.get_apr(),
                "max_DD" => *metrics.get_max_drawdown(),
                "max_DD_%" => *metrics.get_max_drawdown_pct(),
                "max_DD_30%" => {
                    if *metrics.get_max_drawdown() < -0.3 {
                        -100.0
                    } else {
                        0.0
                    }
                }
                "APR/DD_factor_3" => {
                    if *metrics.get_apr_to_drawdown_ratio() < 3.0 {
                        0.0
                    } else {
                        *metrics.get_apr_to_drawdown_ratio() * 3.0
                    }
                }
                "APR/DD_factor" => *metrics.get_apr_to_drawdown_ratio(),
                "Recovery_Factor" => *metrics.get_recovery_factor(),
                "Recovery_Factor_5" => {
                    if *metrics.get_recovery_factor() < 5.0 {
                        0.0
                    } else {
                        *metrics.get_recovery_factor() / 3.0
                    }
                }
                "Deals_Count" => -((*metrics.get_deals_count() as f64) + 1.0).ln(), // Negative count for maximization (fewer trades might be better depending on context, but often more is desired, this might need review)
                "Composite" => metrics.get_composite_score(),
                unknown => {
                    anyhow::bail!(
                        "unknown composite metric '{}': it would silently contribute 0.0",
                        unknown
                    );
                }
            };
            // Add the weighted value of this metric to the total score.
            total_score += metric_value * weight;
        }

        anyhow::Ok(total_score)
    }

    /// Executes a LSHADE-RSP optimization.
    ///
    /// The optimizer evaluates the population in parallel and writes per-iteration statistics
    /// to `lshade_optimization_results.csv` itself; the evaluated parameter sets are saved
    /// with `save_grid_search_optimization_results`.
    pub fn run_lshade_rsp_search(
        self,
        lshade_params: &farukon_core::settings::LshadeRspParams,
        global_data_store: std::sync::Arc<data_engine::global_data_storage::GlobalDataStore>,
    ) -> anyhow::Result<()> {
        let lshade_config =
            farukon_core::optimization::LshadeRspConfig::from_settings(lshade_params);
        let opt_config = self.get_grid_search_optimizer().get_config();
        let mut lshade = farukon_core::optimization::LshadeRspOptimizer::new()
            .with_config(lshade_config.clone())
            .with_optimization_config(opt_config.clone());

        let lib_path = &self.strategy_settings.strategy_path;
        let lib = std::sync::Arc::new(
            unsafe { libloading::Library::new(lib_path) }
                .with_context(|| format!("failed to load the strategy library {}", lib_path))?,
        );

        let initial_capital_for_strategy = self.initial_capital_for_strategy;
        let common_settings = self.common_settings.clone();
        let strategy_settings = self.strategy_settings.clone();
        let strategy_instruments_info = self.strategy_instruments_info.clone();
        let lshade_config_closure = lshade_config.clone();

        let all_results_shared = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let all_results_clone = std::sync::Arc::clone(&all_results_shared);

        // First failure of a candidate evaluation; reported once the optimizer returns.
        let candidate_error: std::sync::Arc<std::sync::Mutex<Option<anyhow::Error>>> =
            std::sync::Arc::new(std::sync::Mutex::new(None));
        let candidate_error_worker = std::sync::Arc::clone(&candidate_error);

        lshade.run(&strategy_settings.clone(), move |params| {
            let mode = &common_settings.mode;
            let global_data_storage_mode = &common_settings.global_data_storage_mode;
            let fitness_direction = lshade_config_closure.get_fitness_direction().as_str();

            // Once a candidate failed the run is already compromised: skip the remaining
            // backtests and report the stored error instead of running garbage.
            if candidate_error_worker.lock().unwrap().is_some() {
                return Self::worst_fitness(fitness_direction);
            }

            let test_settings = &farukon_core::utils::create_stratagy_settings_from_params(
                &strategy_settings,
                params,
            );
            let results = match global_data_storage_mode.as_str() {
                "arc" => Self::run_backtest_with_settings_arc(
                    mode,
                    &initial_capital_for_strategy,
                    test_settings,
                    &strategy_instruments_info,
                    std::sync::Arc::clone(&global_data_store),
                    lib.clone(),
                    params,
                ),
                _ => Self::run_backtest_with_settings_deep(
                    mode,
                    &initial_capital_for_strategy,
                    test_settings,
                    &strategy_instruments_info,
                    std::sync::Arc::clone(&global_data_store),
                    lib.clone(),
                    params,
                ),
            };
            let results = match results {
                Ok(results) => results,
                Err(error) => {
                    *candidate_error_worker.lock().unwrap() = Some(error);
                    return Self::worst_fitness(fitness_direction);
                }
            };

            let fitness =
                Self::calculate_fitness_score(&results, lshade_config_closure.get_fitness_metric());
            let results_obj = farukon_core::optimization::OptimizationResult::new()
                .with_parameters(params.clone())
                .with_results(results.clone());

            {
                let mut results_guard = all_results_clone.lock().unwrap();
                results_guard.push(results_obj);
            }

            match fitness {
                Ok(fitness) => fitness,
                Err(error) => {
                    *candidate_error_worker.lock().unwrap() = Some(error);
                    Self::worst_fitness(fitness_direction)
                }
            }
        })?;

        let final_results = {
            let results_guard = all_results_shared.lock().unwrap();
            results_guard.clone()
        };

        // Save results using existing method
        self.save_grid_search_optimization_results(&final_results)?;

        // A failed candidate means the produced statistics are not trustworthy: report it.
        let candidate_error = candidate_error.lock().unwrap().take();
        if let Some(error) = candidate_error {
            return Err(error.context("LSHADE-RSP search aborted: a candidate failed"));
        }

        anyhow::Ok(())
    }

    /// Saves the results of a Grid Search optimization to a CSV file.
    /// The CSV file contains the tested parameter sets and their corresponding performance metrics.
    /// This allows for easy analysis and comparison of different hyperparameter combinations.
    ///
    /// # Arguments
    /// * `results` - A slice of `OptimizationResult` objects, each containing a parameter set and its performance metrics.
    ///
    /// # Returns
    /// * `anyhow::Result<()>` - `Ok(())` on success, or an `Err` if file creation or writing fails.
    pub fn save_grid_search_optimization_results(
        &self,
        results: &[farukon_core::optimization::OptimizationResult],
    ) -> anyhow::Result<()> {
        // --- 1. Determine Output Filename ---
        // Constructs the path for the output CSV file based on the strategy's settings.
        // The filename includes a fixed name "optimization_results.csv" appended to the 'exit_results_path'.
        let filename = if self.common_settings.mode == "Portfolio" {
            format!(
                "{}/optimization_results_{}.csv",
                self.strategy_settings.exit_results_path, self.strategy_settings.strategy_name
            )
        } else {
            format!(
                "{}/optimization_results.csv",
                self.strategy_settings.exit_results_path
            )
        };
        let path = std::path::Path::new(&filename);

        // --- 2. Create/Open Output File ---
        // Attempts to create the file. If it exists, it will be truncated.
        // Returns an error if the file cannot be created (e.g., directory doesn't exist, permission denied).
        std::fs::create_dir_all(&self.strategy_settings.exit_results_path)?;
        let file_exist = path.exists();

        // --- 3. Prepare Header Row (Column Names) ---
        // --- 3.1: Extract Strategy Parameter Names ---
        // Gets the names of the strategy-specific parameters that were optimized.
        // These become the first few column names in the CSV.
        let mut strategy_params: Vec<String> = self
            .strategy_settings
            .strategy_params
            .keys()
            .cloned()
            .collect();
        strategy_params.sort();

        // --- 3.2: Extract Position Sizer Additional Parameter Names ---
        // Gets the names of any additional parameters for the position sizer (e.g., MPR multiplier).
        let mut possizers_additional_params: Vec<String> = self
            .strategy_settings
            .pos_sizer_params
            .pos_sizer_params
            .keys()
            .cloned()
            .collect();
        possizers_additional_params.sort();

        // --- 3.3: Extract Performance Metric Names ---
        // Gets the names of the performance metrics from the *first* result.
        // Assumes all results have the same set of metrics.
        // This could be fragile if results have different metrics, but is common for grid search.
        let result_names = if !file_exist && !results.is_empty() {
            let mut names: Vec<String> = results[0]
                .get_results()
                .to_stats_list()
                .iter()
                .map(|(key, _value)| key.clone())
                .collect();
            names.sort();
            Some(names)
        } else if !results.is_empty() {
            let mut names: Vec<String> = results[0]
                .get_results()
                .to_stats_list()
                .iter()
                .map(|(key, _value)| key.clone())
                .collect();
            names.sort();
            Some(names)
        } else {
            return anyhow::Ok(());
        };

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&filename)?;

        if !file_exist && let Some(ref names) = result_names {
            // add Strategy name as first column
            write!(file, "strategy_name;")?;

            // --- 3.4: Write Strategy Parameter Column Names ---
            // Writes the names of the strategy parameters to the file header, separated by semicolons.
            for name in &strategy_params {
                write!(file, "{};", name)?;
            }

            // --- 3.5: Write Position Sizer Column Names ---
            // Adds column names for the position sizing method name and its value.
            write!(file, "pos_sizer_name;")?;
            write!(file, "pos_sizer_value;")?;

            // --- 3.6: Write Position Sizer Additional Parameter Column Names ---
            // Writes the names of the position sizer's additional parameters to the header.
            for name in &possizers_additional_params {
                write!(file, "{};", name)?;
            }

            // --- 3.7: Write Slippage Column Name ---
            // Adds a column name for the slippage parameter used in the test.
            write!(file, "slippage;")?;

            // --- 3.8: Write Performance Metric Column Names ---
            // Writes all performance metric names except the last one, followed by a semicolon.
            // The last metric name is written with a newline character using `writeln!`.element
            for name in &names[0..names.len() - 1] {
                write!(file, "{};", name)?;
            }
            // Write the last metric name and add a newline to complete the header row.
            writeln!(file, "{:?}", names.last().unwrap())?;
        }

        // --- 4. Write Data Rows ---
        // Iterates through each OptimizationResult and writes its parameters and metrics as a row in the CSV.
        if let Some(ref names) = result_names {
            for result in results {
                // write strategy name
                write!(file, "{};", self.strategy_settings.strategy_name)?;
                // --- 4.1: Extract Strategy Parameter Values for Current Result ---
                // Gets the map of (name, value) for the current result's strategy parameters.
                let params_map: std::collections::HashMap<_, _> = result
                    .get_parameters()
                    .get_strategy_params()
                    .clone()
                    .into_iter()
                    .collect();

                // Gets the values corresponding to the sorted parameter names, preserving order.
                let strategy_params_values: Vec<serde_json::Value> = strategy_params
                    .iter()
                    .filter_map(|key| params_map.get(key))
                    .cloned()
                    .collect();

                // --- 4.2: Write Strategy Parameter Values ---
                // Writes each strategy parameter value, followed by a semicolon.
                for value in &strategy_params_values {
                    write!(file, "{};", value)?;
                }

                // --- 4.3: Write Position Sizer Name and Value ---
                // Writes the name and value of the position sizing method used for this result.
                write!(file, "{};", result.get_parameters().get_pos_sizer_name())?;
                write!(file, "{};", result.get_parameters().get_pos_sizer_value())?;

                // --- 4.4: Extract Position Sizer Additional Parameter Values ---
                // Gets the map of (name, value) for the current result's position sizer additional parameters.
                let pos_sizer_additional_params_map: std::collections::HashMap<_, _> = result
                    .get_parameters()
                    .get_pos_sizer_additional_params()
                    .clone()
                    .into_iter()
                    .collect();

                // Gets the values corresponding to the sorted additional parameter names, preserving order.
                let pos_sizer_additional_params_values: Vec<serde_json::Value> =
                    possizers_additional_params
                        .iter()
                        .filter_map(|key| pos_sizer_additional_params_map.get(key))
                        .cloned()
                        .collect();

                // --- 4.5: Write Position Sizer Additional Parameter Values ---
                // Writes each position sizer additional parameter value, followed by a semicolon.
                for value in pos_sizer_additional_params_values {
                    write!(file, "{};", value)?;
                }

                // --- 4.6: Write Slippage Value ---
                // Writes the slippage value used for this result.
                write!(file, "{};", result.get_parameters().get_slippage())?;

                // --- 4.7: Extract Performance Metric Values ---
                // Gets the map of (metric_name, metric_value_string) for the current result's performance metrics.
                let performance_metrics_map: std::collections::HashMap<_, _> =
                    result.get_results().to_stats_list().into_iter().collect();

                // Gets the values corresponding to the sorted metric names, preserving order.
                let performance_metrics_values: Vec<String> = names
                    .iter()
                    .filter_map(|key| performance_metrics_map.get(key))
                    .cloned()
                    .collect();

                // --- 4.8: Write Performance Metric Values ---
                // Writes each performance metric value, followed by a semicolon, except the last one.
                for value in &performance_metrics_values[0..performance_metrics_values.len() - 1] {
                    write!(file, "{};", value)?;
                }
                writeln!(file, "{}", performance_metrics_values.last().unwrap())?;
            }
        }

        // --- 6. Return Success ---
        // Indicates that the file was written successfully.
        anyhow::Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use farukon_core::optimization::{OptimizationResult, ParameterSet};
    use farukon_core::performance::PerformanceMetrics;
    use farukon_core::settings::FitnessValue;

    /// Builds `PerformanceMetrics` for fitness tests: the struct exposes no public constructor,
    /// but it is `Deserialize`.
    fn test_metrics(max_drawdown: f64, max_drawdown_pct: f64) -> PerformanceMetrics {
        serde_json::from_value(serde_json::json!({
            "total_return": 100.0,
            "total_return_percent": 10.0,
            "apr": 20.0,
            "max_drawdown": max_drawdown,
            "max_drawdown_pct": max_drawdown_pct,
            "apr_to_drawdown_ratio": 2.0,
            "recovery_factor": 4.0,
            "deals_count": 10,
            "max_drawdown_datetime": null,
        }))
        .expect("test metrics must deserialize")
    }

    fn test_candidate() -> ParameterSet {
        ParameterSet::new()
            .with_pos_sizer_name("test_sizer".to_string())
            .with_pos_sizer_value(1.5)
            .with_slippage(0.001)
    }

    /// Regression: a candidate whose strategy construction or backtest fails must fail the grid
    /// search with candidate context, never produce a placeholder result and a successful run.
    #[test]
    fn failing_candidate_produces_error_with_candidate_context() {
        let candidate = test_candidate();
        let candidate_error = std::sync::Mutex::new(None);

        let metrics =
            OptimizationRunner::evaluate_grid_candidate(&candidate, &candidate_error, || {
                Err(anyhow::anyhow!("strategy construction failed"))
            });
        assert!(
            metrics.is_none(),
            "a failed candidate must not produce performance metrics"
        );

        // The optimizer core still yields a placeholder for the failed candidate; the search
        // must turn that into an error instead of reporting success.
        let placeholder = OptimizationResult::new().with_parameters(candidate.clone());
        let error = OptimizationRunner::finish_grid_search(vec![placeholder], &candidate_error)
            .expect_err("a failed candidate must fail the grid search");
        let message = format!("{:#}", error);
        assert!(
            message.contains("grid search candidate failed"),
            "missing failure context: {message}"
        );
        assert!(
            message.contains(candidate.format_for_display().as_str()),
            "missing candidate parameters in: {message}"
        );
        assert!(
            message.contains("strategy construction failed"),
            "missing root cause in: {message}"
        );
    }

    /// Regression: without a failure the grid search output is returned unchanged.
    #[test]
    fn successful_grid_search_returns_results() {
        let candidate = test_candidate();
        let candidate_error = std::sync::Mutex::new(None);

        let metrics =
            OptimizationRunner::evaluate_grid_candidate(&candidate, &candidate_error, || {
                Ok(test_metrics(-0.1, -1000.0))
            })
            .expect("a successful candidate must produce performance metrics");
        assert!(metrics.get_max_drawdown().eq(&-0.1));

        let result = OptimizationResult::new()
            .with_parameters(candidate)
            .with_results(metrics);
        let results = OptimizationRunner::finish_grid_search(vec![result], &candidate_error)
            .expect("a search without failures must succeed");
        assert_eq!(results.len(), 1);
    }

    /// Regression: the host returns the RAW metric. The optimizer applies `fitness_direction`
    /// itself, so for "min" a lower drawdown must score lower; the pre-fix code negated the
    /// metric here, which inverted the direction.
    #[test]
    fn fitness_score_is_the_raw_metric_for_minimization() {
        let worse_drawdown = test_metrics(-0.40, -4000.0);
        let better_drawdown = test_metrics(-0.10, -1000.0);

        let worse_score =
            OptimizationRunner::calculate_fitness_score(&worse_drawdown, &FitnessValue::MaxDD)
                .expect("fitness score");
        let better_score =
            OptimizationRunner::calculate_fitness_score(&better_drawdown, &FitnessValue::MaxDD)
                .expect("fitness score");

        assert!(
            (worse_score - (-0.40)).abs() < 1e-12,
            "must be raw: {worse_score}"
        );
        assert!(
            (better_score - (-0.10)).abs() < 1e-12,
            "must be raw: {better_score}"
        );
        assert!(
            worse_score < better_score,
            "a lower drawdown must yield a lower score for minimization ({worse_score} vs {better_score})"
        );
    }

    /// Regression: `max_DD` and `max_DD_%` are accepted metric names and must contribute the
    /// actual drawdown values instead of silently contributing 0.0.
    #[test]
    fn composite_score_uses_max_drawdown_metrics() {
        let metrics = test_metrics(-0.25, -2500.0);

        let max_dd =
            OptimizationRunner::calculate_composite_score(&metrics, &["max_DD".to_string()])
                .expect("max_DD is a valid composite metric");
        assert!(
            (max_dd - (-0.25)).abs() < 1e-12,
            "unexpected max_DD: {max_dd}"
        );

        let max_dd_pct =
            OptimizationRunner::calculate_composite_score(&metrics, &["max_DD_%".to_string()])
                .expect("max_DD_% is a valid composite metric");
        assert!(
            (max_dd_pct - (-2500.0)).abs() < 1e-12,
            "unexpected max_DD_%: {max_dd_pct}"
        );
    }

    /// Regression: an unknown composite metric name must be an error, not a silent 0.0.
    #[test]
    fn unknown_composite_metric_is_an_error() {
        let metrics = test_metrics(-0.25, -2500.0);
        let error = OptimizationRunner::calculate_composite_score(
            &metrics,
            &["unknown_metric".to_string()],
        )
        .expect_err("an unknown composite metric must be rejected");
        assert!(
            error.to_string().contains("unknown_metric"),
            "error must name the metric: {error}"
        );
    }
}

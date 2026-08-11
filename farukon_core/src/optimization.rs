// farukon_core/src/optimization.rs

//! Optimization engine for hyperparameter tuning.
//! Supports Grid Search (exhaustive) and Genetic Algorithm (evolutionary).
//! Uses Rayon for parallel evaluation of thousands of parameter combinations.

use ::std::io::Write;
use itertools::Itertools;
use rand::prelude::*;
use rayon::prelude::*;
use std::hash::Hash;
use std::hash::Hasher;

use crate::performance;
use crate::settings;
use crate::utils;

/// Represents the result of evaluating a single parameter set.
/// Contains the parameters used and the resulting performance metrics.
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    parameters: ParameterSet,
    results: performance::PerformanceMetrics,
}

impl OptimizationResult {
    /// Creates a new, empty OptimizationResult.
    pub fn new() -> Self {
        Self {
            parameters: ParameterSet::new(),
            results: performance::PerformanceMetrics::default(),
        }
    }

    /// Sets the parameters for this result.
    pub fn with_parameters(mut self, parameters: ParameterSet) -> Self {
        self.parameters = parameters;
        self
    }

    /// Sets the performance metrics for this result.
    pub fn with_results(mut self, results: performance::PerformanceMetrics) -> Self {
        self.results = results;
        self
    }

    /// --- Getters ---
    pub fn get_parameters(&self) -> &ParameterSet {
        &self.parameters
    }

    pub fn get_results(&self) -> &performance::PerformanceMetrics {
        &self.results
    }
}

/// Represents a single set of strategy, position sizing, and slippage parameters.
/// Used as input to the backtest engine during optimization.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterSet {
    /// Strategy-specific parameters (e.g., short_window, long_window).
    strategy_params: Vec<(String, serde_json::Value)>,
    /// Name of the position sizing method (e.g., "mpr", "fixed_ratio").
    pos_sizer_name: String,
    /// Value for the position sizing method (e.g., 1.5 for MPR).
    pos_sizer_value: f64,
    /// Additional parameters for the position sizer (currently unused for MPR).
    pos_sizer_additional_params: Vec<(String, serde_json::Value)>,
    /// Slippage value to apply during execution (percentage of price).
    slippage: f64,
}

impl ParameterSet {
    /// Creates a new, empty ParameterSet.
    pub fn new() -> Self {
        Self {
            strategy_params: Vec::<(String, serde_json::Value)>::new(),
            pos_sizer_name: String::new(),
            pos_sizer_value: 0.0,
            pos_sizer_additional_params: Vec::<(String, serde_json::Value)>::new(),
            slippage: 0.0,
        }
    }

    /// Sets the strategy parameters.
    pub fn with_strategy_params(mut self, params: Vec<(String, serde_json::Value)>) -> Self {
        self.strategy_params = params;
        self
    }

    /// Sets the name of the position sizing method.
    pub fn with_pos_sizer_name(mut self, name: String) -> Self {
        self.pos_sizer_name = name;
        self
    }

    /// Sets the value for the position sizing method.
    pub fn with_pos_sizer_value(mut self, value: f64) -> Self {
        self.pos_sizer_value = value;
        self
    }

    /// Sets additional parameters for the position sizer.
    pub fn with_pos_sizer_additional_params(
        mut self,
        params: Vec<(String, serde_json::Value)>,
    ) -> Self {
        self.pos_sizer_additional_params = params;
        self
    }

    /// Sets the slippage value.
    pub fn with_slippage(mut self, value: f64) -> Self {
        self.slippage = value;
        self
    }

    /// --- Getters ---
    /// Returns a reference to the strategy parameters.
    pub fn get_strategy_params(&self) -> &Vec<(String, serde_json::Value)> {
        &self.strategy_params
    }

    /// Returns a reference to the position sizer value.
    pub fn get_pos_sizer_value(&self) -> &f64 {
        &self.pos_sizer_value
    }

    /// Returns a reference to the position sizer name
    pub fn get_pos_sizer_name(&self) -> &String {
        &self.pos_sizer_name
    }

    /// Returns a reference to the position sizer additional parameters
    pub fn get_pos_sizer_additional_params(&self) -> &Vec<(String, serde_json::Value)> {
        &self.pos_sizer_additional_params
    }

    /// Returns a reference to the slippage value.
    pub fn get_slippage(&self) -> &f64 {
        &self.slippage
    }

    /// Generates a human-readable string representation for logging and display.
    pub fn format_for_display(&self) -> String {
        // Human-readable string for logging.

        let strategy_str = if self.strategy_params.is_empty() {
            "{}".to_string()
        } else {
            let params_str = self
                .strategy_params
                .iter()
                .map(|(k, v)| format!("'{}': {}", k, v))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{}}}", params_str)
        };

        let additional_params_str = if self.pos_sizer_additional_params.is_empty() {
            "".to_string()
        } else {
            let params_str = self
                .pos_sizer_additional_params
                .iter()
                .map(|(k, v)| format!("'{}': {}", k, v))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}", params_str)
        };

        let pos_sizer_str = format!(
            "{{'pos_sizer_type': '{}', 'pos_sizer_value': {:.2}, 'pos_sizer_additional_params': {}}}",
            self.pos_sizer_name, self.pos_sizer_value, additional_params_str,
        );

        let slippage_str = format!("{{'slippage': {}}}", self.slippage);

        format!("{} {} {}", strategy_str, pos_sizer_str, slippage_str)
    }

    pub fn aproximate_size_in_bytes(&self) -> usize {
        let mut total = 0;

        // strategy_params: Vec<(String, serde_json::Value)>
        for (s, v) in &self.strategy_params {
            total += std::mem::size_of_val(s);
            total += s.len();
            total += std::mem::size_of::<serde_json::Value>();
            if let serde_json::Value::String(s_val) = v {
                total += s_val.len();
            }
        }

        // pos_sizer_name: String
        total += std::mem::size_of_val(&self.pos_sizer_name);
        total += self.pos_sizer_name.len();

        // pos_sizer_value: f64
        total += std::mem::size_of_val(&self.pos_sizer_value);

        // pos_sizer_additional_params: Vec<(String, serde_json::Value)>
        for (s, v) in &self.pos_sizer_additional_params {
            total += std::mem::size_of_val(s);
            total += s.len();
            total += std::mem::size_of::<serde_json::Value>();
            if let serde_json::Value::String(s_val) = v {
                total += s_val.len();
            }
        }

        // slippage: f64
        total += std::mem::size_of_val(&self.slippage);

        total
    }
}

/// Configuration for the optimization process.
/// Defines the ranges of values to test for each parameter.
#[derive(Debug, Clone)]
pub struct OptimizationConfig {
    /// Maps strategy parameter names to lists of possible values.
    strategy_params_ranges: std::collections::HashMap<String, settings::ParamSpec>,
    /// List of possible values for the position sizer.
    pos_sizer_value_range: settings::ParamSpec,
    /// List of possible values for slippage.
    slippage_range: settings::ParamSpec,
    /// Position sizer name
    pos_sizer_name: String,
    /// Maps position sizer additional params
    pos_sizer_additional_params: std::collections::HashMap<String, settings::ParamSpec>,
}

impl OptimizationConfig {
    /// Creates a new, empty OptimizationConfig.
    pub fn new() -> Self {
        Self {
            strategy_params_ranges: std::collections::HashMap::new(),
            pos_sizer_value_range: settings::ParamSpec::Discrete(vec![]),
            slippage_range: settings::ParamSpec::Discrete(vec![]),
            pos_sizer_name: String::new(),
            pos_sizer_additional_params: std::collections::HashMap::new(),
        }
    }

    /// Sets the ranges for strategy parameters.
    pub fn with_strategy_params_ranges(
        mut self,
        ranges: std::collections::HashMap<String, settings::ParamSpec>,
    ) -> Self {
        self.strategy_params_ranges = ranges;
        self
    }

    /// Sets position sizer name
    pub fn with_pos_sizer_name(mut self, name: String) -> Self {
        self.pos_sizer_name = name;
        self
    }

    /// Sets position sizer additional parameters
    pub fn with_pos_sizer_additional_params(
        mut self,
        params: std::collections::HashMap<String, settings::ParamSpec>,
    ) -> Self {
        self.pos_sizer_additional_params = params;
        self
    }

    /// Sets the range of values for the position sizer.
    pub fn with_pos_sizer_value_ranges(mut self, range: settings::ParamSpec) -> Self {
        self.pos_sizer_value_range = range;
        self
    }

    /// Sets the range of values for slippage.
    pub fn with_slippage_range(mut self, range: settings::ParamSpec) -> Self {
        self.slippage_range = range;
        self
    }

    /// Returns a reference to the strategy parameters ranges.
    pub fn get_strategy_params_ranges(&self) -> &std::collections::HashMap<String, settings::ParamSpec> {
        &self.strategy_params_ranges
    }

    /// Returns a reference to the pos sizer value range.
    pub fn get_pos_sizer_value_range(&self) -> &settings::ParamSpec {
        &self.pos_sizer_value_range
    }

    /// Returns a reference to the slippage range.
    pub fn get_slippage_range(&self) -> &settings::ParamSpec {
        &self.slippage_range
    }

    /// Returns a reference to the pos sizer name.
    pub fn get_pos_sizer_name(&self) -> &String {
        &self.pos_sizer_name
    }

    /// Generates an iterator over all possible combinations of parameters.
    ///
    /// This method creates a lazy iterator that produces `ParameterSet` objects by combining:
    /// - All strategy parameters defined in `strategy_params_ranges` (using cartesian product).
    /// - All values from `pos_sizer_value_range`.
    /// - All values from `slippage_range`.
    /// - Additional position sizer parameters from `pos_sizer_additional_params`.
    ///
    /// Each `ParameterSet` contains:
    /// - Strategy parameters (e.g., `short_window`, `long_window`).
    /// An iterator that yields `ParameterSet` objects.
    fn generate_all_combinations_iter(&self) -> impl Iterator<Item = ParameterSet> + '_ {
        let strategy_params_names: Vec<String> =
            self.strategy_params_ranges.keys().cloned().collect();
        let pos_sizer_name = self.pos_sizer_name.clone();
        let pos_sizer_additional_params_names: Vec<String> =
            self.pos_sizer_additional_params.keys().cloned().collect();

        let slippage_values: Vec<f64> = self
            .slippage_range
            .expand()
            .into_iter()
            .filter_map(|v| v.as_f64())
            .collect();

        slippage_values.into_iter().flat_map({
            let pos_sizer_name = pos_sizer_name.clone();
            let pos_sizer_additional_params_names = pos_sizer_additional_params_names.clone();
            move |slippage| {
                let pos_sizer_values_raw: Vec<f64> = self
                    .pos_sizer_value_range
                    .expand()
                    .into_iter()
                    .filter_map(|v| v.as_f64())
                    .collect();
                let pos_sizer_values = if pos_sizer_values_raw.is_empty() {
                    vec![0.0]
                } else {
                    pos_sizer_values_raw
                };

                pos_sizer_values.into_iter().flat_map({
                    let pos_sizer_name = pos_sizer_name.clone();
                    let pos_sizer_additional_params_names =
                        pos_sizer_additional_params_names.clone();
                    let value = strategy_params_names.clone();
                    move |pos_sizer_val| {
                        let strategy_ranges: Vec<Vec<serde_json::Value>> = self
                            .strategy_params_ranges
                            .values()
                            .map(|spec| spec.expand())
                            .collect();

                        strategy_ranges.into_iter().multi_cartesian_product().map({
                            let pos_sizer_name = pos_sizer_name.clone();
                            let pos_sizer_additional_params_names =
                                pos_sizer_additional_params_names.clone();
                            let value = value.clone();
                            move |strategy_values| {
                                let strategy_params = value
                                    .clone()
                                    .iter()
                                    .zip(strategy_values)
                                    .map(|(name, value)| (name.clone(), value))
                                    .collect();

                                let pos_sizer_additional_params: Vec<(String, serde_json::Value)> =
                                    pos_sizer_additional_params_names
                                        .iter()
                                        .map(|name| {
                                            let spec =
                                                self.pos_sizer_additional_params.get(name).unwrap();
                                            let val = spec
                                                .expand()
                                                .into_iter()
                                                .next()
                                                .unwrap_or(serde_json::Value::Null);
                                            (name.clone(), val)
                                        })
                                        .collect();

                                ParameterSet::new()
                                    .with_strategy_params(strategy_params)
                                    .with_pos_sizer_name(pos_sizer_name.clone())
                                    .with_pos_sizer_additional_params(pos_sizer_additional_params)
                                    .with_pos_sizer_value(pos_sizer_val)
                                    .with_slippage(slippage)
                            }
                        })
                    }
                })
            }
        })
    }
}

// --- GRID SEARCH OPTIMIZER ---

/// A simple, exhaustive optimizer that tests every combination of parameters.
#[derive(Debug, Clone)]
pub struct GridSearchOptimizer {
    config: OptimizationConfig,
}

impl GridSearchOptimizer {
    /// Creates a new GridSearchOptimizer.
    pub fn new() -> Self {
        Self {
            config: OptimizationConfig::new(),
        }
    }

    /// Sets the optimization configuration.
    pub fn with_optimization_config(mut self, config: OptimizationConfig) -> Self {
        self.config = config;
        self
    }

    /// Returns a reference to the optimization configuration.
    pub fn get_config(&self) -> &OptimizationConfig {
        &self.config
    }

    /// Calculates the total number of parameter combinations to test.
    pub fn calculate_total_combinations(&self) -> anyhow::Result<u128> {
        let strategy_combinations = self
            .config
            .strategy_params_ranges
            .values()
            .map(|spec| spec.count_expanded_values())
            .collect::<anyhow::Result<Vec<u128>>>()?
            .into_iter()
            .product::<u128>()
            .max(1);

        let pos_sizer_additional_combinations = self
            .config
            .pos_sizer_additional_params
            .values()
            .map(|spec| spec.count_expanded_values())
            .collect::<anyhow::Result<Vec<u128>>>()?
            .into_iter()
            .product::<u128>()
            .max(1);

        let pos_sizer_value_count = self
            .config
            .pos_sizer_value_range
            .count_expanded_values()?
            .max(1);
        let slippage_count = self.config.slippage_range.count_expanded_values()?.max(1);

        let total_combinations = strategy_combinations
            * pos_sizer_additional_combinations
            * pos_sizer_value_count
            * slippage_count;

        anyhow::Ok(total_combinations)
    }

    /// Checks if the total memory required for grid search exceeds available system memory.
    ///
    /// This method estimates the total memory needed to store all parameter combinations
    /// generated by `generate_all_combinations_iter()` and compares it against 80% of available RAM.
    /// If the estimated memory usage exceeds the limit, it returns an error to prevent OOM (Out of Memory).
    ///
    /// The estimation is based on:
    /// - Total number of combinations calculated via `calculate_total_combinations()`.
    /// - Approximate size in bytes of a single `ParameterSet` (obtained by generating one sample set).
    /// - 80% of available system memory as the safe threshold.
    ///
    /// # Arguments
    /// * `config` - The optimization configuration containing parameter ranges to estimate memory for.
    ///
    /// # Returns
    /// * `Ok(())` if memory usage is within limits.
    /// * `Err` with a descriptive message if memory usage would exceed available RAM.
    pub fn check_memory_limit(&self, config: &OptimizationConfig) -> anyhow::Result<()> {
        let total_combinations = self.calculate_total_combinations()?;
        if total_combinations == 0 {
            return anyhow::Ok(());
        }

        if total_combinations > std::usize::MAX as u128 {
            anyhow::bail!(
                "Total combinations ({}) exceed maximum possible value ({}).",
                total_combinations,
                std::usize::MAX
            );
        }

        let sample_set = config.generate_all_combinations_iter().next();
        let aprox_size_per_set = if let Some(set) = sample_set {
            set.aproximate_size_in_bytes()
        } else {
            ParameterSet::new().aproximate_size_in_bytes()
        };
        let total_combinations_u64 = total_combinations as u64;
        let estimated_bytes = total_combinations_u64 as u64 * aprox_size_per_set as u64;

        let mut sys = sysinfo::System::new_all();
        sys.refresh_memory();
        let available_memory = sys.available_memory() * 1024;
        let limit_memory = (available_memory as f64 * 0.8) as u64;

        if estimated_bytes > limit_memory {
            anyhow::bail!(
                "Grid search would require ~{} bytes ({} combinations * ~{} bytes per set), but only ~{} bytes are available (80% of available memory).",
                estimated_bytes,
                total_combinations,
                aprox_size_per_set,
                limit_memory
            );
        }

        anyhow::Ok(())
    }

    /// Runs the grid search optimization.
    /// Evaluates each parameter set in parallel using the provided fitness function.
    /// # Arguments
    /// * `fitness_function` - A function that takes a ParameterSet and returns an OptimizationResult.
    /// * `threads` - Number of threads to use for parallel evaluation.
    /// * `combinations` - Vector of ParameterSet objects to evaluate.
    /// # Returns
    /// * A vector of OptimizationResult objects.
    pub fn run_optimization<F>(
        &self,
        fitness_function: F,
        threads: usize,
        config: &OptimizationConfig,
    ) -> Vec<OptimizationResult>
    where
        F: Fn(&ParameterSet) -> OptimizationResult + Send + Sync + 'static,
    {
        let combinations: Vec<ParameterSet> = config.generate_all_combinations_iter().collect();

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("Failed to create thread pool");

        pool.install(|| {
            combinations
                .into_par_iter()
                .map(|parameter_set| fitness_function(&parameter_set))
                .collect()
        })
    }
}

// --- GENETIC ALGORITHM OPTIMIZER ---

/// Statistics for a single generation of the Genetic Algorithm.
#[derive(Debug, Clone)]
pub struct GAStatsPerGeneration {
    best_fitness: f64,
    worst_fitness: f64,
    mean_fitness: f64,
    std_dev: f64,
    best_chromosome_id: Vec<String>,
    generation: usize,
}

impl GAStatsPerGeneration {
    /// Creates a new, empty GAStatsPerGeneration.
    pub fn new() -> Self {
        Self {
            best_fitness: 0.0,
            worst_fitness: 0.0,
            mean_fitness: 0.0,
            std_dev: 0.0,
            best_chromosome_id: Vec::new(),
            generation: 0,
        }
    }

    /// Sets the best fitness score for this generation.
    pub fn with_best_fitness(mut self, value: f64) -> Self {
        self.best_fitness = value;
        self
    }

    /// Sets the worst fitness score for this generation.
    pub fn with_worst_fitness(mut self, value: f64) -> Self {
        self.worst_fitness = value;
        self
    }

    /// Sets the mean fitness score for this generation.
    pub fn with_mean_fitness(mut self, value: f64) -> Self {
        self.mean_fitness = value;
        self
    }

    /// Sets the standart_deviation fitness score for this generation.
    pub fn with_standart_deviation(mut self, std_dev: f64) -> Self {
        self.std_dev = std_dev;
        self
    }

    /// Sets the ID of the best chromosome for this generation.
    pub fn with_best_chromosome_id(mut self, id: Vec<String>) -> Self {
        self.best_chromosome_id = id;
        self
    }

    /// Sets the generation number.
    pub fn with_generation(mut self, number: usize) -> Self {
        self.generation = number;
        self
    }
}

/// Configuration for the Genetic Algorithm.
#[derive(Debug, Clone)]
pub struct GAConfig {
    population_size: usize,
    elite_size: usize,
    max_generations: usize,
    p_crossover: f64,
    p_mutation: f64,
    fitness_metric: settings::FitnessValue,
    fitness_direction: String,
    std_stop: f64,
}

impl GAConfig {
    /// Creates a new, empty GAConfig.
    pub fn new() -> Self {
        Self {
            population_size: 0,
            elite_size: 0,
            max_generations: 0,
            p_crossover: 0.0,
            p_mutation: 0.0,
            fitness_metric: settings::FitnessValue::default(),
            fitness_direction: String::new(),
            std_stop: 0.0,
        }
    }

    /// Creates a GAConfig from the provided GAParams.
    pub fn from_settings(ga_params: &settings::GAParams) -> Self {
        Self {
            population_size: ga_params.population_size,
            elite_size: ga_params.elite_size,
            max_generations: ga_params.max_generations,
            p_crossover: ga_params.p_crossover,
            p_mutation: ga_params.p_mutation,
            fitness_metric: ga_params.fitness_params.fitness_value.clone(),
            fitness_direction: ga_params.fitness_params.fitness_direction.clone(),
            std_stop: ga_params.std_stop,
        }
    }

    /// Returns a reference to the fitness metric.
    pub fn get_fitness_metric(&self) -> &settings::FitnessValue {
        &self.fitness_metric
    }

    /// Returns a reference to the fitness direction.
    pub fn get_fitness_direction(&self) -> &String {
        &self.fitness_direction
    }
}

/// The main Genetic Algorithm optimizer.
pub struct GeneticAlgorythm {
    ga_config: GAConfig,
    optimization_config: OptimizationConfig,
    chromosome_bank: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<u64, f64>>>,
    populations: Vec<Vec<ParameterSet>>,
}

impl GeneticAlgorythm {
    /// Creates a new GeneticAlgorythm.
    pub fn new() -> Self {
        Self {
            ga_config: GAConfig::new(),
            optimization_config: OptimizationConfig::new(),
            chromosome_bank: std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::HashMap::new(),
            )),
            populations: Vec::new(),
        }
    }

    /// Sets the GA configuration.
    pub fn with_ga_config(mut self, ga_config: GAConfig) -> Self {
        self.ga_config = ga_config;
        self
    }

    /// Sets the optimization configuration.
    pub fn with_optimization_config(mut self, opt_config: OptimizationConfig) -> Self {
        self.optimization_config = opt_config;
        self
    }

    /// Runs the genetic algorithm optimization.
    /// # Arguments
    /// * `initial_strategy_settings` - The initial strategy settings.
    /// * `evaluate` - A function that takes a ParameterSet and returns a fitness score.
    /// # Returns
    /// * A vector of GAStatsPerGeneration objects.
    pub fn run<F>(
        &mut self,
        initial_strategy_settings: &settings::StrategySettings,
        evaluate: F,
    ) -> anyhow::Result<Vec<GAStatsPerGeneration>>
    where
        F: Fn(&ParameterSet) -> f64 + Send + Sync + Clone + 'static,
    {
        let threads = initial_strategy_settings.threads.unwrap_or(num_cpus::get());
        let mut stats = Vec::new();
        let total_population = self.ga_config.population_size;
        self.create_initial_population(total_population);

        for gen_idx in 0..self.ga_config.max_generations {
            let start_time = std::time::Instant::now();
            println!("Generation: # {}", gen_idx);

            // Beginer population
            let current_population = self.populations[gen_idx].clone();
            let results =
                self.evaluate_population(threads, &current_population, evaluate.clone())?;

            let stat = self.calculate_generation_stats(&results, gen_idx);
            stats.push(stat.clone());

            println!(
                "Generation {}: Time elapsed: {:.3} seconds, Best Fitness= {:.5}, Worst Fitness= {:.5}, Mean Fitness= {:.5}, std fitness= {:.5}, Chromosome ID: {:?}",
                gen_idx,
                start_time.elapsed().as_secs_f64(),
                stat.best_fitness,
                stat.worst_fitness,
                stat.mean_fitness,
                stat.std_dev,
                stat.best_chromosome_id
            );

            // Check Convergence Criteria "GA parameter: std_stop"
            if stat.std_dev < self.ga_config.std_stop {
                println!(
                    "Genetic Algorythm stopped due to meet of Convergence Criteria. Current std= {}, GA stop_std= {}",
                    stat.std_dev, self.ga_config.std_stop,
                );
                break;
            }

            // Next Populations
            if gen_idx + 1 < self.ga_config.max_generations {
                let next_population = self.tournament_selection(&results);
                self.populations.push(next_population);
            }
        }

        anyhow::Ok(stats)
    }

    /// Evaluates a population of parameter sets in parallel.
    /// Caches fitness scores to avoid redundant calculations.
    fn evaluate_population<F>(
        &self,
        threads: usize,
        population: &[ParameterSet],
        evaluate: F,
    ) -> anyhow::Result<Vec<(ParameterSet, f64)>>
    where
        F: Fn(&ParameterSet) -> f64 + Send + Sync + Clone + 'static,
    {
        let chromosome_bank = self.chromosome_bank.clone();

        let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let total_evaluations = population.len();

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("Failed to create thread pool");

        let results = pool.install(|| {
            population
                .par_iter()
                .map(|params| {
                    let start_time = std::time::Instant::now();
                    let current_count =
                        counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

                    println!(
                        "# {} from {} {}",
                        current_count,
                        total_evaluations,
                        params.format_for_display()
                    );

                    let hash = hash_parameter_set(params);

                    let cached_fitness = {
                        let bank = chromosome_bank.lock().unwrap();
                        bank.get(&hash).copied()
                    };

                    let fitness = if let Some(cahed_f) = cached_fitness {
                        println!(
                            "# {} from {} is done in {:.3} seconds, fitnesss= {}",
                            current_count,
                            total_evaluations,
                            start_time.elapsed().as_secs_f64(),
                            cahed_f
                        );
                        cahed_f
                    } else {
                        let calculated_f = evaluate(params);
                        let mut bank = chromosome_bank.lock().unwrap();
                        bank.insert(hash, calculated_f);
                        drop(bank);

                        println!(
                            "# {} from {} is done in {:.3} seconds, fitnesss= {}",
                            current_count,
                            total_evaluations,
                            start_time.elapsed().as_secs_f64(),
                            calculated_f
                        );
                        calculated_f
                    };
                    (params.clone(), fitness)
                })
                .collect()
        });

        anyhow::Ok(results)
    }

    /// Creates the initial population by sampling from all possible parameter combinations.
    fn create_initial_population(&mut self, target_size: usize) {
        let mut rng = rand::thread_rng();
        let population = utils::generate_lhs_parameter_sets(
            &self.optimization_config.strategy_params_ranges,
            &self.optimization_config.pos_sizer_value_range,
            &self.optimization_config.slippage_range,
            &self.optimization_config.pos_sizer_additional_params,
            &self.optimization_config.pos_sizer_name,
            target_size,
            &mut rng,
        );
        self.populations.push(population);
    }

    /// Creates a human-readable ID for a chromosome (parameter set) for display.
    fn create_chromosome_id_for_display(&self, params: &ParameterSet) -> Vec<String> {
        let mut id = Vec::new();
        for (k, v) in &params.strategy_params {
            id.push(format!("{}:{:?}", k, v));
        }
        id.push(format!("pos_sizer_value:{}", params.pos_sizer_value));
        id.push(format!("slippage:{}", params.slippage));
        id
    }

    /// Calculates statistics (mean, standart deviation) for a vector of fitness scores.
    fn calculate_stats(&self, values: &[f64]) -> (f64, f64) {
        if values.is_empty() {
            return (0.0, 0.0);
        }

        let mut sum = 0.0;
        for &value in values {
            sum += value;
        }
        let mean = sum / values.len() as f64;

        let chunks = values.chunks_exact(4);
        let remainder = chunks.remainder();

        let mut sum_sq_diffs = [0.0; 4];
        for chunk in chunks {
            for i in 0..4 {
                let diff = chunk[i] - mean;
                sum_sq_diffs[i] += diff * diff;
            }
        }
        for (i, &value) in remainder.iter().enumerate() {
            let diff = value - mean;
            sum_sq_diffs[i] += diff * diff;
        }

        let total_sum_sq_diffs: f64 = sum_sq_diffs.iter().sum();
        let variance = total_sum_sq_diffs / values.len() as f64;
        let std_dev = variance.sqrt();

        (mean, std_dev)
    }

    /// Calculates statistics for a generation.
    fn calculate_generation_stats(
        &self,
        results: &[(ParameterSet, f64)],
        gen_idx: usize,
    ) -> GAStatsPerGeneration {
        let fitness_values: Vec<f64> = results.iter().map(|(_, f)| *f).collect();
        let (mean, std_dev) = self.calculate_stats(&fitness_values);
        if results.is_empty() {
            return GAStatsPerGeneration::new().with_generation(gen_idx);
        }

        let best_result_entry = match self.ga_config.fitness_direction.as_str() {
            "max" => results
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
            "min" => results
                .iter()
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
            _ => results
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
        };

        let (best_params, best_fitness_actual) = best_result_entry.unwrap();

        let worst_result_entry = match self.ga_config.fitness_direction.as_str() {
            "max" => results
                .iter()
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
            "min" => results
                .iter()
                .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
            _ => results
                .iter()
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
        };
        let worst_fitness_actual = worst_result_entry.unwrap().1;

        let best_chromosome_id = self.create_chromosome_id_for_display(best_params);

        GAStatsPerGeneration::new()
            .with_best_fitness(*best_fitness_actual)
            .with_worst_fitness(worst_fitness_actual)
            .with_mean_fitness(mean)
            .with_standart_deviation(std_dev)
            .with_best_chromosome_id(best_chromosome_id)
            .with_generation(gen_idx)
    }

    /// Performs crossover and mutation on two parent chromosomes to create a child.
    fn crossover_mutation(&self, a: &ParameterSet, b: &ParameterSet) -> ParameterSet {
        let max_attempts = (self.ga_config.population_size as f64).sqrt() as usize;
        for _ in 0..max_attempts {
            // 1. Crossover
            let child_after_crossover = self.perform_crossover(a, b);

            // 2. Mutation
            let new_set = self.perform_mutation(child_after_crossover);

            // 3. Check in chromosome bank
            let hash = hash_parameter_set(&new_set);
            let bank = self.chromosome_bank.lock().unwrap();
            if !bank.contains_key(&hash) {
                drop(bank);
                return new_set;
            }
            drop(bank);
        }
        a.clone()
    }

    /// Applies mutation to a single `ParameterSet`.
    ///
    /// This function iterates through each parameter (strategy, pos_sizer_value, slippage, pos_sizer_additional_params)
    /// and applies mutation based on the configured `p_mutation` probability.
    /// For each parameter, it retrieves the corresponding `ParamSpec` and uses it to generate a new value.
    /// The mutated `ParameterSet` is returned.
    ///
    /// # Arguments
    /// * `param_set` - The `ParameterSet` to mutate.
    ///
    /// # Returns
    /// A new `ParameterSet` with potentially mutated parameters.
    fn perform_mutation(&self, mut param_set: ParameterSet) -> ParameterSet {
        let mut rng = thread_rng();

        // 5 Mutation for strategy_params
        param_set.strategy_params = param_set
            .strategy_params
            .into_iter()
            .map(|(name, old_val)| {
                if rng.r#gen::<f64>() < self.ga_config.p_mutation {
                    let new_val = self.mutate_param_value(
                        &name,
                        old_val.clone(),
                        &self.optimization_config.strategy_params_ranges,
                    );
                    (name, new_val)
                } else {
                    (name, old_val)
                }
            })
            .collect::<Vec<_>>();

        // 6 Mutation for pos_sizer_value
        if rng.r#gen::<f64>() < self.ga_config.p_mutation {
            param_set.pos_sizer_value = self.mutate_param_f64(
                param_set.pos_sizer_value,
                &self.optimization_config.pos_sizer_value_range,
            );
        }

        // 7 Mutation for slippage
        if rng.r#gen::<f64>() < self.ga_config.p_mutation {
            param_set.slippage =
                self.mutate_param_f64(param_set.slippage, &self.optimization_config.slippage_range);
        }

        // 8 Mutation for pos_sizer_additional_params
        param_set.pos_sizer_additional_params = param_set
            .pos_sizer_additional_params
            .into_iter()
            .map(|(name, old_val)| {
                if rng.r#gen::<f64>() < self.ga_config.p_mutation {
                    if let Some(spec) = self
                        .optimization_config
                        .pos_sizer_additional_params
                        .get(&name)
                    {
                        let new_val = self.mutate_param_value_json(old_val.clone(), spec);
                        (name, new_val)
                    } else {
                        (name, old_val)
                    }
                } else {
                    (name, old_val)
                }
            })
            .collect::<Vec<_>>();
        param_set
    }

    /// Mutates an `f64` value based on its `ParamSpec`.
    ///
    /// Handles both `Discrete` and `Range` specifications.
    /// For `Discrete`, selects a random value from the list.
    /// For `Range`, generates a random value within the bounds, applying the step if defined.
    ///
    /// # Arguments
    /// * `old_val` - The current `f64` value.
    /// * `spec` - The `settings::ParamSpec` defining the valid values for this parameter.
    ///
    /// # Returns
    /// A new `f64` value resulting from the mutation.
    fn mutate_param_f64(&self, old_val: f64, spec: &settings::ParamSpec) -> f64 {
        let mut rng = thread_rng();
        match spec {
            settings::ParamSpec::Discrete(values) => {
                if !values.is_empty() {
                    if let Some(random_json_value) = values.choose(&mut rng) {
                        if let Some(random_f64_val) = random_json_value.as_f64() {
                            random_f64_val
                        } else {
                            old_val
                        }
                    } else {
                        old_val
                    }
                } else {
                    old_val
                }
            }
            settings::ParamSpec::Range { start, end, .. } => {
                let mut val = rng.gen_range(*start..=*end);
                if let Some(step_val) = spec.step() {
                    val = (val / step_val).round() * step_val;
                    val = val.clamp(*start, *end);
                }
                val
            }
        }
    }

    /// Delegates mutation of a `serde_json::Value` based on its name and the overall parameter map.
    ///
    /// Looks up the `ParamSpec` for the given parameter name and calls `mutate_param_value_json`.
    ///
    /// # Arguments
    /// * `_name` - The name of the parameter (used to find its spec).
    /// * `old_val` - The current `serde_json::Value`.
    /// * `config_map` - A map of parameter names to their `ParamSpec`.
    ///
    /// # Returns
    /// A new `serde_json::Value` resulting from the mutation.
    fn mutate_param_value(
        &self,
        _name: &str,
        old_val: serde_json::Value,
        config_map: &std::collections::HashMap<String, settings::ParamSpec>,
    ) -> serde_json::Value {
        if let Some(spec) = config_map.get(_name) {
            self.mutate_param_value_json(old_val, spec)
        } else {
            old_val
        }
    }

    /// Mutates a `serde_json::Value` based on its `ParamSpec`.
    ///
    /// Handles both `Discrete` and `Range` specifications.
    /// For `Discrete`, selects a random `Value` from the list.
    /// For `Range`, generates a new `f64` value and wraps it in a `Value::Number`.
    ///
    /// # Arguments
    /// * `_old_val` - The current `serde_json::Value` (not directly used here, but passed in for consistency).
    /// * `spec` - The `settings::ParamSpec` defining the valid values for this parameter.
    ///
    /// # Returns
    /// A new `serde_json::Value` resulting from the mutation.
    fn mutate_param_value_json(
        &self,
        _old_val: serde_json::Value,
        spec: &settings::ParamSpec,
    ) -> serde_json::Value {
        let mut rng = thread_rng();
        match spec {
            settings::ParamSpec::Discrete(values) => {
                if !values.is_empty() {
                    let random_val = values.choose(&mut rng).unwrap();
                    random_val.clone()
                } else {
                    _old_val
                }
            }
            settings::ParamSpec::Range { start, end, .. } => {
                let mut val = rng.gen_range(*start..=*end);
                if let Some(step_val) = spec.step() {
                    val = (val / step_val).round() * step_val;
                    val = val.clamp(*start, *end);
                }
                serde_json::Value::Number(serde_json::Number::from_f64(val).unwrap())
            }
        }
    }

    /// Performs crossover between two parent `ParameterSet`s.
    ///
    /// This function combines parameters from two parents (`a` and `b`) to create a new offspring.
    /// Crossover is applied to `strategy_params`, `pos_sizer_value`, `slippage`, and `pos_sizer_additional_params`
    /// based on the configured `p_crossover` probability.
    /// Parameters are matched by name (using HashMaps for efficient lookup).
    ///
    /// # Arguments
    /// * `a` - The first parent `ParameterSet`.
    /// * `b` - The second parent `ParameterSet`.
    ///
    /// # Returns
    /// A new `ParameterSet` representing the offspring.
    fn perform_crossover(&self, a: &ParameterSet, b: &ParameterSet) -> ParameterSet {
        let mut rng = thread_rng();

        // 1. Crossover for strategy_params
        let b_params_map: std::collections::HashMap<&String, &serde_json::Value> =
            b.strategy_params.iter().map(|(k, v)| (k, v)).collect();
        let mut new_strategy_params = Vec::new();
        for (name_a, val_a) in &a.strategy_params {
            if let Some(val_b) = b_params_map.get(name_a) {
                if rng.r#gen::<f64>() < self.ga_config.p_crossover {
                    new_strategy_params.push((name_a.clone(), (*val_a).clone()));
                } else {
                    new_strategy_params.push((name_a.clone(), (*val_b).clone()));
                }
            } else {
                new_strategy_params.push((name_a.clone(), (*val_a).clone()));
            }
        }

        // 2. Crossover for pos_sizer_value
        let pos_sizer_value_after_crossover = if rng.r#gen::<f64>() < self.ga_config.p_crossover {
            a.pos_sizer_value
        } else {
            b.pos_sizer_value
        };

        // 3. Crossover for slippage
        let slippage_after_crossover = if rng.r#gen::<f64>() < self.ga_config.p_crossover {
            a.slippage
        } else {
            b.slippage
        };

        // 4. Crossover for pos_sizer_additional_params
        let b_psap_map: std::collections::HashMap<&String, &serde_json::Value> = b
            .pos_sizer_additional_params
            .iter()
            .map(|(k, v)| (k, v))
            .collect();
        let mut new_pos_sizer_additional_params = Vec::new();
        for (name_a, val_a) in &a.pos_sizer_additional_params {
            if let Some(val_b) = b_psap_map.get(name_a) {
                if rng.r#gen::<f64>() < self.ga_config.p_crossover {
                    new_pos_sizer_additional_params.push((name_a.clone(), (*val_a).clone()));
                } else {
                    new_pos_sizer_additional_params.push((name_a.clone(), (*val_b).clone()));
                }
            } else {
                new_pos_sizer_additional_params.push((name_a.clone(), (*val_a).clone()));
            }
        }

        ParameterSet::new()
            .with_strategy_params(new_strategy_params)
            .with_pos_sizer_name(a.pos_sizer_name.clone())
            .with_pos_sizer_value(pos_sizer_value_after_crossover)
            .with_pos_sizer_additional_params(new_pos_sizer_additional_params)
            .with_slippage(slippage_after_crossover)
    }

    /// Performs tournament selection to create the next generation.
    fn tournament_selection(&self, results: &[(ParameterSet, f64)]) -> Vec<ParameterSet> {
        let mut next_gen = Vec::with_capacity(self.ga_config.population_size);
        let mut rng = rand::thread_rng();

        // 1. Save best individs
        let mut sorted_results = results.to_vec();
        sorted_results.sort_by(|a, b| match self.ga_config.fitness_direction.as_str() {
            "max" => b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal),
            "min" => a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal),
            _ => b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal),
        });

        let elite_count = self
            .ga_config
            .elite_size
            .min(self.ga_config.population_size);
        let mut elite = Vec::with_capacity(elite_count);
        for i in 0..elite_count {
            let elite_individ = sorted_results[i].0.clone();
            next_gen.push(elite_individ.clone());
            elite.push(elite_individ);
        }

        // 2. add random individs
        let random_count = (self.ga_config.population_size as f64).sqrt() as usize;
        let random_sets = utils::generate_lhs_parameter_sets(
            &self.optimization_config.strategy_params_ranges,
            &self.optimization_config.pos_sizer_value_range,
            &self.optimization_config.slippage_range,
            &self.optimization_config.pos_sizer_additional_params,
            &self.optimization_config.pos_sizer_name,
            random_count,
            &mut rng,
        );
        for random_set in random_sets {
            let hash = hash_parameter_set(&random_set);
            let bank = self.chromosome_bank.lock().unwrap();
            if !bank.contains_key(&hash) {
                drop(bank);
                next_gen.push(random_set);
            } else {
                drop(bank);
                let fallback = self.generate_random_individ();
                next_gen.push(fallback);
            }
        }

        // 3. tournament selection
        let remaining_slots = self.ga_config.population_size - next_gen.len();
        let elite_set: std::collections::HashSet<u64> =
            elite.iter().map(|p| hash_parameter_set(p)).collect();
        let mut seen_hashes = std::collections::HashSet::new();
        let mut uniqe_other_results = Vec::new();
        for (params, fitness) in &sorted_results {
            let hash = hash_parameter_set(params);
            if elite_set.contains(&hash) || seen_hashes.contains(&hash) {
                continue;
            }
            seen_hashes.insert(hash);
            uniqe_other_results.push((params, fitness));
        }

        for _ in 0..remaining_slots {
            // choose parant_a randomly from elite
            let elite_parent = elite[rng.gen_range(0..elite.len())].clone();
            let other_results = &uniqe_other_results;

            if other_results.is_empty() {
                let non_elite_in_next_gen: Vec<_> = next_gen
                    .iter()
                    .filter(|individ| !elite.iter().any(|elite_params| elite_params == *individ))
                    .collect();

                let fallback_parent = if !non_elite_in_next_gen.is_empty() {
                    non_elite_in_next_gen[rng.gen_range(0..non_elite_in_next_gen.len())].clone()
                } else {
                    other_results[rng.gen_range(0..other_results.len())]
                        .0
                        .clone()
                };

                let child = self.crossover_mutation(&elite_parent, &fallback_parent);
                next_gen.push(child);
                continue;
            }

            let tournament_size = 2;
            let mut tournament_contestants = Vec::new();
            for _ in 0..tournament_size {
                let idx = rng.gen_range(0..other_results.len());
                tournament_contestants.push(other_results[idx]);
            }
            let parent_b = match self.ga_config.fitness_direction.as_str() {
                "max" => tournament_contestants
                    .iter()
                    .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
                "min" => tournament_contestants
                    .iter()
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
                _ => tournament_contestants
                    .iter()
                    .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
            };
            if let Some((params_b, _)) = parent_b {
                let child = self.crossover_mutation(&elite_parent, params_b);
                next_gen.push(child);
            } else {
                let fallback_parent = &other_results[rng.gen_range(0..other_results.len())].0;
                let child = self.crossover_mutation(&elite_parent, fallback_parent);
                next_gen.push(child);
            }
        }

        next_gen.shuffle(&mut rng);
        next_gen
    }

    /// Saves Genetic Algorithm generation statistics to a CSV file for later analysis and plotting.
    ///
    /// This function writes the collected generation statistics to a CSV file in a format compatible
    /// with the Python plotting script. The output file contains columns for generation number,
    /// best fitness value, mean fitness value, and the chromosome ID of the best individual.
    ///
    /// The file is saved to `{exit_results_path}/ga_optimization_results.csv` where `exit_results_path`
    /// is taken from the provided `strategy_settings`.
    ///
    /// # Arguments
    /// - `best_individ`: Best fitness value in that generation
    /// - `mean`: Average fitness value in that generation
    /// - `best_hromosome_ID`: String representation of the best individual's parameters
    pub fn save_generation_stats_to_csv(
        &self,
        stats: &[GAStatsPerGeneration],
        strategy_settings: &settings::StrategySettings,
    ) -> anyhow::Result<()> {
        let filename = format!(
            "{}/ga_optimization_results.csv",
            strategy_settings.exit_results_path,
        );
        let path = std::path::Path::new(&filename);
        std::fs::create_dir_all(strategy_settings.exit_results_path.clone())?;
        let file_exist = path.exists();

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(&filename)?;

        if !file_exist {
            let headers = &[
                "strategy_name;",
                "number_of_generation;",
                "best_individ;",
                "mean;",
                "best_hromosome_ID",
            ];

            for header in headers {
                write!(file, "{}", header)?;
            }
            let _ = writeln!(file, "");
        }

        for stat in stats {
            let records = &[
                strategy_settings.strategy_name.clone(),
                stat.generation.to_string(),
                stat.best_fitness.to_string(),
                stat.mean_fitness.to_string(),
                stat.best_chromosome_id.join(", "),
            ];

            for record in &records[0..records.len() - 1] {
                write!(file, "{};", record)?;
            }
            let _ = writeln!(file, "{}", records.last().unwrap());
        }

        anyhow::Ok(())
    }

    /// Generates a completely random `ParameterSet` by sampling values according to their `ParamSpec`.
    ///
    /// This function is intended as a fallback mechanism when generating unique individuals fails
    /// or when creating initial random blood. It respects both `Discrete` lists (choosing randomly)
    /// and `Range` definitions (generating values within bounds and steps).
    ///
    /// # Returns
    /// A new `ParameterSet` with randomly sampled parameter values.
    fn generate_random_individ(&self) -> ParameterSet {
        let mut rng = rand::thread_rng();

        // 1. Generate strategy_params
        let strategy_params: Vec<(String, serde_json::Value)> = self
            .optimization_config
            .strategy_params_ranges
            .iter()
            .map(|(name, spec)| {
                let val_f64 = self.sample_param_value_from_spec(spec, &mut rng);
                (
                    name.clone(),
                    serde_json::Value::Number(serde_json::Number::from_f64(val_f64).unwrap()),
                )
            })
            .collect();

        // 2. Get pos_sizer_value
        let pos_sizer_value = {
            let spec = &self.optimization_config.pos_sizer_value_range;
            self.sample_param_value_from_spec(spec, &mut rng)
        };

        // 3. Get slippage
        let slippage = {
            let spec = &self.optimization_config.slippage_range;
            self.sample_param_value_from_spec(spec, &mut rng)
        };

        // 4. Generate pos_sizer_additional_params
        let pos_sizer_additional_params: Vec<(String, serde_json::Value)> = self
            .optimization_config
            .pos_sizer_additional_params
            .iter()
            .map(|(name, spec)| {
                let val_f64 = self.sample_param_value_from_spec(spec, &mut rng);
                (
                    name.clone(),
                    serde_json::Value::Number(serde_json::Number::from_f64(val_f64).unwrap()),
                )
            })
            .collect();

        // 5. Create ParameterSet
        ParameterSet::new()
            .with_strategy_params(strategy_params)
            .with_pos_sizer_name(self.optimization_config.pos_sizer_name.clone())
            .with_pos_sizer_additional_params(pos_sizer_additional_params)
            .with_pos_sizer_value(pos_sizer_value)
            .with_slippage(slippage)
    }

    /// Samples a single `f64` value from a `ParamSpec` using the provided random number generator.
    ///
    /// This helper function encapsulates the logic for handling both `Discrete` and `Range` parameter
    /// specifications, ensuring correct sampling for each type.
    ///
    /// # Arguments
    /// * `spec` - A reference to the `settings::ParamSpec` defining the parameter space.
    /// * `rng` - A mutable reference to a random number generator implementing `rand::Rng`.
    ///
    /// # Returns
    /// An `f64` value sampled according to the rules defined in `spec`.
    fn sample_param_value_from_spec(
        &self,
        spec: &settings::ParamSpec,
        rng: &mut impl rand::Rng,
    ) -> f64 {
        match spec {
            settings::ParamSpec::Discrete(values) => {
                if !values.is_empty() {
                    if let Some(random_json_val) = values.choose(rng) {
                        if let Some(random_f64_val) = random_json_val.as_f64() {
                            random_f64_val
                        } else {
                            eprintln!(
                                "Warning: Discrete value {:?} is not a number, returning 0.0",
                                random_json_val
                            );
                            0.0
                        }
                    } else {
                        0.0
                    }
                } else {
                    eprintln!("Warning: Discrete list is empty, returning 0.0");
                    0.0
                }
            }
            settings::ParamSpec::Range {
                start,
                end,
                step: _,
            } => {
                let mut val = rng.gen_range(*start..=*end);
                if let Some(step_val) = spec.step() {
                    val = (val / step_val).round() * step_val;
                    val = val.clamp(*start, *end);
                }
                val
            }
        }
    }
}

/// Hashes a ParameterSet for caching fitness scores.
fn hash_parameter_set(params: &ParameterSet) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    hash_params(&params.strategy_params, &mut hasher);
    params.pos_sizer_value.to_bits().hash(&mut hasher);
    params.slippage.to_bits().hash(&mut hasher);
    hash_params(&params.pos_sizer_additional_params, &mut hasher);

    hasher.finish()
}

/// Hashes a slice of key-value pairs representing parameters using a provided hasher.
///
/// This function ensures consistent hashing by first sorting the parameters by their keys.
/// For numeric values (`serde_json::Value::Number`), it normalizes the string representation
/// to mitigate potential differences caused by internal `i64`, `u64`, or `f64` storage,
/// ensuring that logically identical numbers produce the same hash.
///
/// # Arguments
/// * `params` - A slice of tuples containing parameter names (String) and their values (serde_json::Value).
/// * `state` - A mutable reference to a hasher implementing the `Hasher` trait, used to accumulate the hash.
fn hash_params<H: std::hash::Hasher>(params: &[(String, serde_json::Value)], state: &mut H) {
    // Create a mutable vector from the input slice to allow sorting.
    let mut sorted_params = params.to_vec();
    // Sort the parameters by their string keys to ensure a consistent order for hashing,
    // regardless of the original order in the ParameterSet.
    sorted_params.sort_by(|a, b| a.0.cmp(&b.0));

    // Iterate over the sorted parameters.
    for (k, v) in sorted_params {
        k.hash(state);

        // Normalize the string representation of the parameter value before hashing.
        // This is crucial for numeric values to ensure consistency.
        let normalized_value_str = match v {
            // Handle the case where the value is a JSON number.
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    // Check if the f64 value is effectively an integer within the i64 range.
                    // If so, convert it to i64 and then to a string (e.g., 10.0 -> "10").
                    // Otherwise, convert the f64 directly to a string (e.g., 10.5 -> "10.5").
                    if f.fract() == 0.0 && f >= (i64::MIN as f64) && f <= (i64::MAX as f64) {
                        (f as i64).to_string()
                    } else {
                        f.to_string()
                    }
                // If it's not an f64, attempt i64.
                } else if let Some(i) = n.as_i64() {
                    i.to_string()
                // If it's not an i64, attempt u64.
                } else if let Some(u) = n.as_u64() {
                    u.to_string()
                // If none of the standard numeric conversions work, fall back to the
                // internal string representation of the Number object.
                } else {
                    n.to_string()
                }
            }
            // For non-numeric values (String, Bool, Null, Array, Object), use the standard
            // serde_json serialization to get a consistent string representation.
            _ => serde_json::to_string(&v).expect("Failed to serialize param value for hashing"),
        };

        // Hash the normalized string representation of the parameter value.
        normalized_value_str.hash(state);
    }
}

// --- LSHADE-RSP OPTIMIZER ---

/// Configuration for the LSHADE-RSP optimizer.
#[derive(Debug, Clone)]
pub struct LshadeRspConfig {
    population_size: usize,
    max_evaluations: usize,
    p_best: f64,
    archive_rate: f64,
    memory_size: usize,
    fitness_metric: settings::FitnessValue,
    fitness_direction: String,
}

impl LshadeRspConfig {
    pub fn new() -> Self {
        Self {
            population_size: 50,
            max_evaluations: 0,
            p_best: 0.1,
            archive_rate: 1.0,
            memory_size: 5,
            fitness_metric: settings::FitnessValue::default(),
            fitness_direction: String::new(),
        }
    }

    pub fn from_settings(lshade_params: &settings::LshadeRspParams) -> Self {
        Self {
            population_size: lshade_params.population_size,
            max_evaluations: lshade_params.max_evaluations,
            p_best: lshade_params.p_best,
            archive_rate: lshade_params.archive_rate,
            memory_size: lshade_params.memory_size,
            fitness_metric: lshade_params.fitness_params.fitness_value.clone(),
            fitness_direction: lshade_params.fitness_params.fitness_direction.clone(),
        }
    }

    pub fn get_fitness_metric(&self) -> &settings::FitnessValue { &self.fitness_metric }
    pub fn get_fitness_direction(&self) -> &String { &self.fitness_direction }
}

/// Statistics for one iteration of LSHADE-RSP.
#[derive(Debug, Clone)]
pub struct LshadeRspIterationStats {
    pub iteration: usize,
    pub best_fitness: f64,
    pub worst_fitness: f64,
    pub mean_fitness: f64,
    pub population_size: usize,
    pub evaluations: usize,
}

/// The LSHADE-RSP (Linear Population Size Reduction with SHADE) optimizer.
/// Differential evolution with rank-based selective pressure.
pub struct LshadeRspOptimizer {
    config: LshadeRspConfig,
    optimization_config: OptimizationConfig,
    population: Vec<ParameterSet>,
    fitness_values: Vec<f64>,
    archive: Vec<ParameterSet>,
    memory_cr: Vec<f64>,
    memory_f: Vec<f64>,
    memory_index: usize,
    eval_count: usize,
}

impl LshadeRspOptimizer {
    pub fn new() -> Self {
        Self {
            config: LshadeRspConfig::new(),
            optimization_config: OptimizationConfig::new(),
            population: Vec::new(),
            fitness_values: Vec::new(),
            archive: Vec::new(),
            memory_cr: Vec::new(),
            memory_f: Vec::new(),
            memory_index: 0,
            eval_count: 0,
        }
    }

    pub fn with_config(mut self, config: LshadeRspConfig) -> Self {
        self.config = config;
        self
    }

    pub fn with_optimization_config(mut self, opt_config: OptimizationConfig) -> Self {
        self.optimization_config = opt_config;
        self
    }

    /// Initializes the historical memory with default values.
    fn init_memory(&mut self) {
        let msize = self.config.memory_size;
        self.memory_cr = vec![0.5; msize];
        self.memory_f = vec![0.5; msize];
        self.memory_index = 0;
    }

    /// Generates a random initial population within the parameter bounds.
    fn create_initial_population(&mut self, pop_size: usize) {
        let mut rng = rand::thread_rng();
        self.population.clear();
        let param_names: Vec<String> = self.optimization_config.get_strategy_params_ranges().keys().cloned().collect();

        for _ in 0..pop_size {
            let mut strategy_params = Vec::new();
            for name in &param_names {
                if let Some(spec) = self.optimization_config.get_strategy_params_ranges().get(name) {
                    let val = Self::random_value_in_spec(spec, &mut rng);
                    strategy_params.push((name.clone(), val));
                }
            }

            let pos_sizer_val = {
                let spec = self.optimization_config.get_pos_sizer_value_range();
                let vals = spec.expand();
                vals.choose(&mut rng).cloned().unwrap_or(serde_json::Value::Null)
            };

            let slippage = {
                let spec = self.optimization_config.get_slippage_range();
                let vals = spec.expand();
                vals.choose(&mut rng).and_then(|v| v.as_f64()).unwrap_or(0.005)
            };

            let ps = ParameterSet::new()
                .with_strategy_params(strategy_params)
                .with_pos_sizer_name(self.optimization_config.get_pos_sizer_name().clone())
                .with_pos_sizer_value(pos_sizer_val.as_f64().unwrap_or(0.0))
                .with_slippage(slippage);

            self.population.push(ps);
        }
    }

    /// Generates a random value conforming to the ParamSpec.
    fn random_value_in_spec(spec: &settings::ParamSpec, rng: &mut impl rand::Rng) -> serde_json::Value {
        match spec {
            settings::ParamSpec::Discrete(values) => {
                values.choose(rng).cloned().unwrap_or(serde_json::Value::Null)
            }
            settings::ParamSpec::Range { start, end, step } => {
                let mut val = rng.gen_range(*start..=*end);
                if *step > 0.0 {
                    val = (val / step).round() * step;
                    val = val.clamp(*start, *end);
                }
                serde_json::Value::Number(serde_json::Number::from_f64(val).unwrap_or(serde_json::Number::from_f64(*start).unwrap()))
            }
        }
    }

    /// Performs DE/current-to-pbest/1 mutation with archive.
    fn mutate(&self, target_idx: usize, rng: &mut impl rand::Rng) -> Vec<(String, serde_json::Value)> {
        let pop_size = self.population.len();
        let p_best = self.config.p_best;
        let p_num = (pop_size as f64 * p_best).max(1.0) as usize;

        // Select F from memory
        let ri = rng.gen_range(0..self.memory_f.len());
        let f = self.memory_f[ri];

        // Select pbest individual
        let pbest_idx = {
            let mut indices: Vec<usize> = (0..pop_size).collect();
            indices.sort_by(|&a, &b| {
                self.fitness_values[b].partial_cmp(&self.fitness_values[a]).unwrap_or(std::cmp::Ordering::Equal)
            });
            indices[rng.gen_range(0..p_num)]
        };

        // Select r1 from population (not target, not pbest)
        let mut r1 = rng.gen_range(0..pop_size);
        while r1 == target_idx || r1 == pbest_idx { r1 = rng.gen_range(0..pop_size); }

        // Select r2 from population ∪ archive (not target, not pbest, not r1)
        let total_pool = pop_size + self.archive.len();
        let mut r2 = rng.gen_range(0..total_pool);
        while r2 == target_idx || r2 == pbest_idx || r2 == r1 {
            r2 = rng.gen_range(0..total_pool);
        }

        let pbest = &self.population[pbest_idx];
        let target = &self.population[target_idx];
        let x_r1 = &self.population[r1];
        let x_r2 = if r2 < pop_size {
            &self.population[r2]
        } else {
            &self.archive[r2 - pop_size]
        };

        // Create mutant: target + F*(pbest - target) + F*(r1 - r2)
        let param_names: Vec<String> = self.optimization_config.get_strategy_params_ranges().keys().cloned().collect();
        let mut mutant = Vec::new();

        for name in &param_names {
            let spec = self.optimization_config.get_strategy_params_ranges().get(name);
            let target_val = target.strategy_params.iter().find(|(n, _)| n == name).map(|(_, v)| v);
            let pbest_val = pbest.strategy_params.iter().find(|(n, _)| n == name).map(|(_, v)| v);
            let r1_val = x_r1.strategy_params.iter().find(|(n, _)| n == name).map(|(_, v)| v);
            let r2_val = x_r2.strategy_params.iter().find(|(n, _)| n == name).map(|(_, v)| v);

            if let (Some(tv), Some(pv), Some(r1v), Some(r2v), Some(spec)) = (target_val, pbest_val, r1_val, r2_val, spec) {
                if let (Some(t), Some(p), Some(r1_n), Some(r2_n)) = (tv.as_f64(), pv.as_f64(), r1v.as_f64(), r2v.as_f64()) {
                    let mut mutant_val = t + f * (p - t) + f * (r1_n - r2_n);
                    // Clamp to bounds
                    if let settings::ParamSpec::Range { start, end, step } = spec {
                        mutant_val = mutant_val.clamp(*start, *end);
                        if *step > 0.0 {
                            mutant_val = (mutant_val / step).round() * step;
                            mutant_val = mutant_val.clamp(*start, *end);
                        }
                    }
                    mutant.push((name.clone(), serde_json::Value::Number(
                        serde_json::Number::from_f64(mutant_val).unwrap()
                    )));
                } else {
                    mutant.push((name.clone(), pv.clone()));
                }
            } else {
                mutant.push((name.clone(), pbest_val.cloned().unwrap_or(serde_json::Value::Null)));
            }
        }

        mutant
    }

    /// Binomial crossover between target and mutant to produce trial vector.
    fn crossover(&self, target_idx: usize, mutant: &[(String, serde_json::Value)], cr: f64, rng: &mut impl rand::Rng) -> ParameterSet {
        let target = &self.population[target_idx];
        let param_names: Vec<String> = self.optimization_config.get_strategy_params_ranges().keys().cloned().collect();
        let j_rand = rng.gen_range(0..param_names.len());
        let mut trial_params = Vec::new();

        for (j, name) in param_names.iter().enumerate() {
            let use_mutant = j == j_rand || rng.r#gen::<f64>() <= cr;
            let target_val = target.strategy_params.iter().find(|(n, _)| n == name).map(|(_, v)| v);
            let mutant_val = mutant.iter().find(|(n, _)| n == name).map(|(_, v)| v);

            let val = if use_mutant {
                mutant_val.cloned().unwrap_or_else(|| target_val.cloned().unwrap_or(serde_json::Value::Null))
            } else {
                target_val.cloned().unwrap_or(serde_json::Value::Null)
            };
            trial_params.push((name.clone(), val));
        }

        ParameterSet::new()
            .with_strategy_params(trial_params)
            .with_pos_sizer_name(target.pos_sizer_name.clone())
            .with_pos_sizer_value(target.pos_sizer_value)
            .with_slippage(target.slippage)
    }

    /// Linear Population Size Reduction: removes the worst individual.
    fn reduce_population(&mut self) {
        if self.population.len() <= 4 {
            return;
        }

        // Find index of worst fitness
        let worst_idx = self.fitness_values.iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i);

        if let Some(idx) = worst_idx {
            // Move to archive before removing (if archive not full)
            let max_archive = (self.population.len() as f64 * self.config.archive_rate) as usize;
            if self.archive.len() < max_archive {
                self.archive.push(self.population[idx].clone());
            }
            self.population.remove(idx);
            self.fitness_values.remove(idx);
        }
    }

    /// Updates the historical memory with successful F and CR values.
    fn update_memory(&mut self, successful_cr: &[f64], successful_f: &[f64]) {
        if successful_cr.is_empty() || successful_f.is_empty() {
            return;
        }

        // Weighted Lehmer mean for CR
        let mean_cr = {
            let sum_sq: f64 = successful_cr.iter().map(|x| x * x).sum();
            let sum: f64 = successful_cr.iter().sum();
            if sum > 0.0 { sum_sq / sum } else { 0.5 }
        };

        // Weighted Lehmer mean for F
        let mean_f = {
            let sum_sq: f64 = successful_f.iter().map(|x| x * x).sum();
            let sum: f64 = successful_f.iter().sum();
            if sum > 0.0 { sum_sq / sum } else { 0.5 }
        };

        if !self.memory_cr.is_empty() {
            self.memory_cr[self.memory_index] = mean_cr.clamp(0.0, 1.0);
            self.memory_f[self.memory_index] = mean_f.clamp(0.0, 2.0);
            self.memory_index = (self.memory_index + 1) % self.memory_cr.len();
        }
    }

    /// Calculates the target population size based on LPSR.
    fn calculate_target_population_size(&self) -> usize {
        let n_init = self.config.population_size as f64;
        let n_min = 4.0;
        let max_evals = self.config.max_evaluations as f64;
        let current_evals = self.eval_count as f64;

        // Linear reduction: N = round(N_init - (N_init - N_min) * evals / max_evals)
        let new_n = n_init - (n_init - n_min) * (current_evals / max_evals);
        new_n.round() as usize
    }

    /// Runs the LSHADE-RSP optimization.
    /// # Arguments
    /// * `initial_strategy_settings` - The initial strategy settings.
    /// * `evaluate` - A function that takes a `ParameterSet` and returns a fitness score.
    /// # Returns
    /// * A vector of `LshadeRspIterationStats`.
    pub fn run<F>(
        &mut self,
        initial_strategy_settings: &settings::StrategySettings,
        evaluate: F,
    ) -> anyhow::Result<Vec<LshadeRspIterationStats>>
    where
        F: Fn(&ParameterSet) -> f64 + Send + Sync + Clone + 'static,
    {
        let threads = initial_strategy_settings.threads.unwrap_or(num_cpus::get());
        let pop_size = self.config.population_size;
        let max_evals = self.config.max_evaluations;
        let mut stats = Vec::new();

        // Initialize memory
        self.init_memory();

        // Create initial population
        self.create_initial_population(pop_size);

        // Evaluate initial population
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).build()?;
        self.fitness_values = pool.install(|| {
            self.population.par_iter()
                .map(|ps| evaluate(ps))
                .collect()
        });
        self.eval_count = self.population.len();

        let mut iteration = 0;

        loop {
            if self.eval_count >= max_evals {
                break;
            }

            let mut successful_cr = Vec::new();
            let mut successful_f = Vec::new();

            let pop_size_before = self.population.len();

            // Evaluate trials in parallel batches
            // Always count evaluations; track success for CR/F memory
            let trial_results: Vec<(usize, ParameterSet, f64, f64, f64, bool)> = pool.install(|| {
                (0..pop_size_before).into_par_iter()
                    .map(|i| {
                        let mut local_rng = rand::thread_rng();
                        let ri = local_rng.gen_range(0..self.memory_cr.len());
                        let mut cr_val = self.memory_cr[ri];
                        let mut f_val;

                        if local_rng.r#gen::<f64>() < 0.1 {
                            cr_val = local_rng.r#gen::<f64>();
                        } else {
                            let noise = (local_rng.r#gen::<f64>() - 0.5) * 0.2;
                            cr_val = (cr_val + noise).clamp(0.0, 1.0);
                        }

                        let ri = local_rng.gen_range(0..self.memory_f.len());
                        let mf = self.memory_f[ri];
                        loop {
                            f_val = mf + 0.1 * (local_rng.r#gen::<f64>() * 2.0 - 1.0).tan();
                            if f_val > 0.0 { break; }
                        }
                        f_val = f_val.min(1.0);

                        let mutant = self.mutate(i, &mut local_rng);
                        let trial = self.crossover(i, &mutant, cr_val, &mut local_rng);
                        let trial_fitness = evaluate(&trial);

                        let is_better = trial_fitness.partial_cmp(&self.fitness_values[i])
                            == Some(std::cmp::Ordering::Greater);
                        (i, trial, trial_fitness, cr_val, f_val, is_better)
                    })
                    .collect()
            });

            // Count all evaluations and apply successful trials
            self.eval_count += trial_results.len();
            for (_orig_idx, trial, fitness, cr_v, f_v, is_better) in &trial_results {
                if *is_better {
                    successful_cr.push(*cr_v);
                    successful_f.push(*f_v);
                    self.population.push(trial.clone());
                    self.fitness_values.push(*fitness);
                }
            }

            // LPSR: reduce population to target size
            let target_size = self.calculate_target_population_size();
            while self.population.len() > target_size.max(4) {
                self.reduce_population();
            }

            // Update memory
            self.update_memory(&successful_cr, &successful_f);

            // Calculate stats
            let best = self.fitness_values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let worst = self.fitness_values.iter().cloned().fold(f64::INFINITY, f64::min);
            let mean = self.fitness_values.iter().sum::<f64>() / self.fitness_values.len() as f64;

            stats.push(LshadeRspIterationStats {
                iteration,
                best_fitness: best,
                worst_fitness: worst,
                mean_fitness: mean,
                population_size: self.population.len(),
                evaluations: self.eval_count,
            });

            println!(
                "LSHADE-RSP Iteration {}: Evals={}/{}, PopSize={}, Best={:.5}, Mean={:.5}",
                iteration, self.eval_count, max_evals, self.population.len(), best, mean,
            );

            iteration += 1;
            if self.eval_count >= max_evals {
                break;
            }
        }

        anyhow::Ok(stats)
    }

    /// Saves LSHADE-RSP iteration statistics to a CSV file.
    pub fn save_stats_to_csv(
        &self,
        stats: &[LshadeRspIterationStats],
        strategy_settings: &settings::StrategySettings,
    ) -> anyhow::Result<()> {
        let path = std::path::Path::new(&strategy_settings.exit_results_path)
            .join("lshade_optimization_results.csv");

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut wtr = csv::WriterBuilder::new()
            .delimiter(b';')
            .from_path(&path)?;

        wtr.write_record(&[
            "strategy_name",
            "iteration",
            "best_fitness",
            "worst_fitness",
            "mean_fitness",
            "population_size",
            "evaluations",
        ])?;

        for s in stats {
            wtr.write_record(&[
                &strategy_settings.strategy_name,
                &s.iteration.to_string(),
                &format!("{:.6}", s.best_fitness),
                &format!("{:.6}", s.worst_fitness),
                &format!("{:.6}", s.mean_fitness),
                &s.population_size.to_string(),
                &s.evaluations.to_string(),
            ])?;
        }

        wtr.flush()?;
        println!(
            "LSHADE-RSP statistics saved to {}",
            path.display()
        );
        anyhow::Ok(())
    }
}

#[cfg(test)]
mod lshade_tests {
    use super::*;
    use crate::settings;
    use serde_json::Value;
    use std::collections::HashMap;

    fn make_test_opt_config() -> OptimizationConfig {
        let strategy_params_ranges: HashMap<String, settings::ParamSpec> = [
            (
                "param_a".to_string(),
                settings::ParamSpec::Range {
                    start: 0.0,
                    end: 10.0,
                    step: 1.0,
                },
            ),
            (
                "param_b".to_string(),
                settings::ParamSpec::Discrete(vec![
                    Value::Number(serde_json::Number::from(1)),
                    Value::Number(serde_json::Number::from(2)),
                    Value::Number(serde_json::Number::from(3)),
                ]),
            ),
        ]
        .into_iter()
        .collect();

        OptimizationConfig::new()
            .with_strategy_params_ranges(strategy_params_ranges)
            .with_pos_sizer_value_ranges(settings::ParamSpec::Discrete(vec![
                Value::Number(serde_json::Number::from_f64(1.0).unwrap()),
            ]))
            .with_slippage_range(settings::ParamSpec::Discrete(vec![
                Value::Number(serde_json::Number::from_f64(0.005).unwrap()),
            ]))
            .with_pos_sizer_name("mpr".to_string())
    }

    // ─── Test 1 ────────────────────────────────────────────────────────

    #[test]
    fn test_lshade_config_from_settings() {
        let lshade_params = settings::LshadeRspParams {
            population_size: 100,
            max_evaluations: 5000,
            p_best: 0.15,
            archive_rate: 1.2,
            memory_size: 8,
            fitness_params: settings::FitnessParams {
                fitness_direction: "max".to_string(),
                fitness_value: settings::FitnessValue::TotalReturn,
            },
        };

        let config = LshadeRspConfig::from_settings(&lshade_params);

        assert_eq!(config.population_size, 100);
        assert_eq!(config.max_evaluations, 5000);
        assert_eq!(config.p_best, 0.15);
        assert_eq!(config.archive_rate, 1.2);
        assert_eq!(config.memory_size, 8);
        assert_eq!(*config.get_fitness_direction(), "max");
    }

    // ─── Test 2 ────────────────────────────────────────────────────────

    #[test]
    fn test_population_initialization() {
        let opt_config = make_test_opt_config();
        let mut optimizer = LshadeRspOptimizer::new()
            .with_optimization_config(opt_config);

        optimizer.create_initial_population(20);

        assert_eq!(optimizer.population.len(), 20);

        for ps in &optimizer.population {
            let params = ps.get_strategy_params();

            // Both "param_a" and "param_b" must be present
            let param_a = params.iter().find(|(n, _)| n == "param_a").unwrap();
            let param_b = params.iter().find(|(n, _)| n == "param_b").unwrap();

            // "param_a" in range [0, 10]
            let a_val = param_a.1.as_f64().unwrap();
            assert!(
                a_val >= 0.0 && a_val <= 10.0,
                "param_a value {} out of [0, 10]",
                a_val
            );

            // "param_b" in {1, 2, 3}
            let b_val = param_b.1.as_f64().unwrap();
            assert!(
                b_val == 1.0 || b_val == 2.0 || b_val == 3.0,
                "param_b value {} not in {{1, 2, 3}}",
                b_val
            );
        }
    }

    // ─── Test 3 ────────────────────────────────────────────────────────

    #[test]
    fn test_mutation_produces_valid_vector() {
        let opt_config = make_test_opt_config();
        let mut optimizer = LshadeRspOptimizer::new()
            .with_optimization_config(opt_config);

        optimizer.create_initial_population(10);
        optimizer.fitness_values = vec![0.0; 10];
        optimizer.init_memory();

        let mut rng = rand::thread_rng();
        let mutant = optimizer.mutate(0, &mut rng);

        // Same number of params as the population members
        assert_eq!(
            mutant.len(),
            optimizer.population[0].get_strategy_params().len()
        );

        for (name, val) in &mutant {
            let spec = optimizer
                .optimization_config
                .get_strategy_params_ranges()
                .get(name)
                .unwrap();
            // Every param must be a valid f64 number and present
            assert!(val.is_number(), "mutant {} value {:?} is not a number", name, val);
            match spec {
                settings::ParamSpec::Range { start, end, .. } => {
                    let v = val.as_f64().unwrap();
                    assert!(
                        v >= *start && v <= *end,
                        "mutant {} value {} out of [{}, {}]",
                        name,
                        v,
                        start,
                        end
                    );
                }
                // For Discrete params, the DE arithmetic may produce
                // values outside the discrete set — that's expected behaviour.
                settings::ParamSpec::Discrete(_) => {}
            }
        }
    }

    // ─── Test 4 ────────────────────────────────────────────────────────

    #[test]
    fn test_crossover_produces_valid_vector() {
        let opt_config = make_test_opt_config();
        let mut optimizer = LshadeRspOptimizer::new()
            .with_optimization_config(opt_config);

        // Use a larger population so mutate() can find distinct r1 / r2.
        optimizer.create_initial_population(10);
        optimizer.fitness_values = vec![0.0; 10];
        optimizer.init_memory();

        let mut rng = rand::thread_rng();
        let mutant = optimizer.mutate(0, &mut rng);
        let trial = optimizer.crossover(0, &mutant, 0.9, &mut rng);

        // Trial must have both strategy params
        let trial_params = trial.get_strategy_params();
        assert_eq!(trial_params.len(), 2);

        for (name, val) in trial_params {
            let spec = optimizer
                .optimization_config
                .get_strategy_params_ranges()
                .get(name)
                .unwrap();
            assert!(val.is_number(), "crossover {} value {:?} is not a number", name, val);
            match spec {
                settings::ParamSpec::Range { start, end, .. } => {
                    let v = val.as_f64().unwrap();
                    assert!(v >= *start && v <= *end);
                }
                // Crossover copies from either target or mutant, both have
                // valid-number values (discrete containment not guaranteed).
                settings::ParamSpec::Discrete(_) => {}
            }
        }
    }

    // ─── Test 5 ────────────────────────────────────────────────────────

    #[test]
    fn test_lpsr_reduces_population() {
        let opt_config = make_test_opt_config();
        let mut optimizer = LshadeRspOptimizer::new()
            .with_optimization_config(opt_config)
            .with_config(LshadeRspConfig::new()); // archive_rate = 1.0

        optimizer.create_initial_population(10);
        optimizer.fitness_values = vec![0.0; 10];

        let len_before = optimizer.population.len();
        optimizer.reduce_population();
        let len_after = optimizer.population.len();

        assert_eq!(len_before, 10);
        assert_eq!(len_after, 9);
    }

    // ─── Test 6 ────────────────────────────────────────────────────────

    #[test]
    fn test_target_population_size_decreases() {
        let mut optimizer = LshadeRspOptimizer::new().with_config(LshadeRspConfig {
            population_size: 50,
            max_evaluations: 1000,
            p_best: 0.1,
            archive_rate: 1.0,
            memory_size: 5,
            fitness_metric: settings::FitnessValue::default(),
            fitness_direction: "max".to_string(),
        });

        optimizer.eval_count = 0;
        let size_start = optimizer.calculate_target_population_size();

        optimizer.eval_count = 500;
        let size_mid = optimizer.calculate_target_population_size();

        optimizer.eval_count = 1000;
        let size_end = optimizer.calculate_target_population_size();

        assert!(size_mid < size_start, "size_mid={} should be < size_start={}", size_mid, size_start);
        assert!(size_end < size_mid, "size_end={} should be < size_mid={}", size_end, size_mid);
        assert!(size_end >= 4, "size_end={} should be >= min size 4", size_end);
    }

    // ─── Test 7 ────────────────────────────────────────────────────────

    #[test]
    fn test_memory_initialization() {
        let mut optimizer = LshadeRspOptimizer::new().with_config(LshadeRspConfig {
            population_size: 50,
            max_evaluations: 1000,
            p_best: 0.1,
            archive_rate: 1.0,
            memory_size: 5,
            fitness_metric: settings::FitnessValue::default(),
            fitness_direction: "max".to_string(),
        });

        optimizer.init_memory();

        assert_eq!(optimizer.memory_cr.len(), 5);
        assert_eq!(optimizer.memory_f.len(), 5);

        for &val in &optimizer.memory_cr {
            assert!((val - 0.5).abs() < 1e-10, "memory_cr value {} != 0.5", val);
        }
        for &val in &optimizer.memory_f {
            assert!((val - 0.5).abs() < 1e-10, "memory_f value {} != 0.5", val);
        }
    }
}

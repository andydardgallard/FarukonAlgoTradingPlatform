// farukon_core/src/optimization.rs

//! Optimization engine for hyperparameter tuning.
//! Supports Grid Search (exhaustive) and Genetic Algorithm (evolutionary).
//! Uses Rayon for parallel evaluation of thousands of parameter combinations.

use std::hash::Hash;
use rand::prelude::*;
use std::hash::Hasher;
use rayon::prelude::*;
use itertools::Itertools;

use crate::settings;
use crate::performance;

/// Represents the result of evaluating a single parameter set.
/// Contains the parameters used and the resulting performance metrics.
#[derive(Debug, Clone)]
pub struct OptimizationResult {
    parameters: ParameterSet,
    results:  performance::PerformanceMetrics,
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
#[derive(Debug, Clone)]
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
    pub fn with_pos_sizer_additional_params(mut self, params: Vec<(String, serde_json::Value)>) -> Self {
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
            let params_str = self.strategy_params
                .iter()
                .map(|(k, v)| format!("'{}': {}", k, v))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{}}}", params_str)
        };

        let additional_params_str = if self.pos_sizer_additional_params.is_empty() {
            "".to_string()
        } else {
            let params_str = self.pos_sizer_additional_params
                .iter()
                .map(|(k, v)| format!("'{}': {}", k , v))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}", params_str)
        };

        let pos_sizer_str = format!(
            "{{'pos_sizer_type': '{}', 'pos_sizer_value': {}, 'pos_sizer_additional_params': {}}}",
            self.pos_sizer_name,
            self.pos_sizer_value,
            additional_params_str,
        );

        let slippage_str = format!("{{'slippage': {}}}", self.slippage);

        format!("{} {} {}", strategy_str, pos_sizer_str, slippage_str)
    }

}

/// Configuration for the optimization process.
/// Defines the ranges of values to test for each parameter.
#[derive(Debug, Clone)]
pub struct OptimizationConfig {
    /// Maps strategy parameter names to lists of possible values.
    strategy_params_ranges: std::collections::HashMap<String, Vec<serde_json::Value>>,
    /// List of possible values for the position sizer.
    pos_sizer_value_range: Vec<f64>,
    /// List of possible values for slippage.
    slippage_range: Vec<f64>,
    /// Position sizer name
    pos_sizer_name: String,
    /// Maps position sizer additional params
    pos_sizer_additional_params: std::collections::HashMap<String, Vec<serde_json::Value>>,
}

impl OptimizationConfig {
    /// Creates a new, empty OptimizationConfig.
    pub fn new() -> Self{
        Self {
            strategy_params_ranges: std::collections::HashMap::new(),
            pos_sizer_value_range: Vec::new(),
            slippage_range: Vec::new(),
            pos_sizer_name: String::new(),
            pos_sizer_additional_params: std::collections::HashMap::new(),
        }
    }

    /// Sets the ranges for strategy parameters.
    pub fn with_strategy_params_ranges(mut self, ranges: std::collections::HashMap<String,  Vec<serde_json::Value>>) -> Self {
        self.strategy_params_ranges = ranges;
        self
    }
    
    /// Sets position sizer name
    pub fn with_pos_sizer_name(mut self, name: String) -> Self {
        self.pos_sizer_name = name;
        self
    }
    
    /// Sets position sizer additional parameters
    pub fn with_pos_sizer_additional_params(mut self, params: std::collections::HashMap<String,  Vec<serde_json::Value>>) -> Self {
        self.pos_sizer_additional_params = params;
        self
    }
    
    /// Sets the range of values for the position sizer.
    pub fn with_pos_sizer_value_ranges(mut self, range: Vec<f64>) -> Self {
        self.pos_sizer_value_range = range;
        self
    }

    /// Sets the range of values for slippage.
    pub fn with_slippage_range(mut self, range: Vec<f64>) -> Self {
        self.slippage_range = range;
        self
    }

    /// Generates all possible combinations of parameters.
    /// Returns a vector of ParameterSet objects.
    pub fn generate_all_combinations_vec(&self) -> Vec<ParameterSet> {
        self.generate_all_combinations_iter().collect()
    }

    /// Generates an iterator over all possible combinations of parameters.
    fn generate_all_combinations_iter(&self) -> impl Iterator<Item = ParameterSet> + '_ {
        // println!("DEBUG {:#?}", self);
        let strategy_params_names: Vec<String> = self.strategy_params_ranges.keys().cloned().collect();
        let pos_sizer_name = self.pos_sizer_name.clone();
        let pos_sizer_additional_params: Vec<(String, serde_json::Value)> = self.pos_sizer_additional_params
            .iter()
            .flat_map(|(key, values)| {
                values.iter().map(|value| (key.clone(), value.clone()))
            })
            .collect();

        self.slippage_range
            .iter()
            .flat_map({
                let pos_sizer_name = pos_sizer_name.clone();
                let pos_sizer_additional_params = pos_sizer_additional_params.clone();
                let strategy_params_names = strategy_params_names.clone();
                move |&slippage| {
                    self.pos_sizer_value_range
                        .iter()
                        .flat_map({
                            let pos_sizer_name = pos_sizer_name.clone();
                            let pos_sizer_additional_params = pos_sizer_additional_params.clone();
                            let strategy_params_names = strategy_params_names.clone();
                            move |&pos_sizer_val| {
                                self.strategy_params_ranges
                                    .values()
                                    .multi_cartesian_product()
                                    .map({
                                        let pos_sizer_name = pos_sizer_name.clone();
                                        let pos_sizer_additional_params = pos_sizer_additional_params.clone();
                                        let strategy_params_names = strategy_params_names.clone();
                                        move |stratagy_values| {
                                            let strategy_params = strategy_params_names
                                                .iter()
                                                .zip(stratagy_values)
                                                .map(|(name, value)| (name.clone(), value.clone()))
                                                .collect();

                                            ParameterSet::new()
                                                .with_strategy_params(strategy_params)
                                                .with_pos_sizer_name(pos_sizer_name.clone())
                                                .with_pos_sizer_additional_params(pos_sizer_additional_params.clone())
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
            config: OptimizationConfig::new()
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
    pub fn calculate_total_combinations(&self) -> usize {
        let strategy_combinations = self.config.strategy_params_ranges
            .values()
            .map(|v| v.len())
            .product::<usize>()
            .max(1);

        if self.config.pos_sizer_value_range.len() != 0 {
            strategy_combinations *
            self.config.slippage_range.len() *
            self.config.pos_sizer_value_range.len()
        } else {
            strategy_combinations *
            self.config.slippage_range.len()
        }
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
        combinations: Vec<ParameterSet>,
    ) -> Vec<OptimizationResult>
    where
        F: Fn(&ParameterSet) -> OptimizationResult + Send + Sync + 'static,
    {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("Failed to create thread pool");

        pool.install(|| {
            combinations
                .into_par_iter()
                .map(|parameter_set| {
                    fitness_function(&parameter_set)
                })
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
    best_chromosome_id: Vec<String>,
    generation: usize, 
}

impl GAStatsPerGeneration {
    /// Creates a new, empty GAStatsPerGeneration.
    pub fn new() -> Self{
        Self {
            best_fitness: 0.0,
            worst_fitness: 0.0,
            mean_fitness: 0.0,
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
    max_generations: usize,
    p_crossover: f64,
    p_mutation: f64,
    fitness_metric: settings::FitnessValue,
    fitness_direction: String,
}

impl GAConfig {
    /// Creates a new, empty GAConfig.
    pub fn new() -> Self {
        Self {
            population_size: 0,
            max_generations: 0,
            p_crossover: 0.0,
            p_mutation: 0.0,
            fitness_metric: settings::FitnessValue::default(),
            fitness_direction: String::new(),
        }
    }

    /// Creates a GAConfig from the provided GAParams.
    pub fn from_settings(ga_params: &settings::GAParams) -> Self {
        Self {
            population_size: ga_params.population_size,
            max_generations: ga_params.max_generations,
            p_crossover: ga_params.p_crossover,
            p_mutation: ga_params.p_mutation,
            fitness_metric: ga_params.fitness_params.fitness_value.clone(),
            fitness_direction: ga_params.fitness_params.fitness_direction.clone(),
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
    chromosome_bank: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<u64, Option<f64>>>>,
    populations: Vec<Vec<ParameterSet>>,
}

impl GeneticAlgorythm {
    /// Creates a new GeneticAlgorythm.
    pub fn new() -> Self {
        Self {
            ga_config: GAConfig::new(),
            optimization_config: OptimizationConfig::new(),
            chromosome_bank: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
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
        evaluate: F
    ) -> anyhow::Result<Vec<GAStatsPerGeneration>>
    where
        F: Fn(&ParameterSet) -> f64 + Send + Sync + Clone + 'static,
    {
        let threads = initial_strategy_settings.threads.unwrap_or(num_cpus::get());
        let mut stats = Vec::new();
        let total_population = self.ga_config.population_size;
        self.create_initial_population(total_population);

        for gen_idx in 0..self.ga_config.max_generations {
            println!("Generation: # {}", gen_idx);
            // Beginer population
            let current_population = self.populations[gen_idx].clone();
            let results = self.evaluate_population(threads, &current_population, evaluate.clone())?;

            let stat = self.calculate_generation_stats(&results, gen_idx);
            stats.push(stat.clone());

            println!(
                "Generation {}: Best Fitness= {:.3}, Mean Fitness= {:.3}, Worst Fitness= {:.3}",
                gen_idx, stat.best_fitness, stat.mean_fitness, stat.worst_fitness
            );

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
        evaluate: F
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
                    let current_count = counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;

                    println!(
                        "# {} from {} {}",
                        current_count,
                        total_evaluations,
                        params.format_for_display()
                    );
                    
                    let hash = hash_parameter_set(params);

                    let cached_fitness = {
                        let bank = chromosome_bank.lock().unwrap();
                        bank.get(&hash).copied().flatten()
                    };

                    let fitness = if let Some(fitness) = cached_fitness {
                        fitness
                    } else {
                        let fitness = evaluate(params);
                        let mut bank = chromosome_bank.lock().unwrap();
                        bank.insert(hash, Some(fitness));
                        fitness
                    };

                    println!("# {} from {} is done in {:.3} seconds ", current_count, total_evaluations, start_time.elapsed().as_secs_f64());

                    (params.clone(), fitness)
                })
                .collect()
        });
        
        anyhow::Ok(results)
    }

    /// Creates the initial population by sampling from all possible parameter combinations.
    fn create_initial_population(&mut self, target_size: usize) {
        let all_combinations = self.optimization_config.generate_all_combinations_vec();
        let mut unique_pool: std::collections::HashSet<u64> = std::collections::HashSet::new();
        let mut population = Vec::with_capacity(target_size.min(all_combinations.len()));
        let mut rng = rand::thread_rng();

        let sample_size = target_size.min(all_combinations.len());
        let choices: Vec<_> = all_combinations.choose_multiple(&mut rng, sample_size).collect();

        for param_set in choices {
            let hash = hash_parameter_set(&param_set);
            // println!("{}", hash);
            if !unique_pool.contains(&hash) {
                population.push(param_set.clone());
                unique_pool.insert(hash);
            }
        }
        
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

    /// Calculates statistics (mean, best, worst) for a vector of fitness scores.
    fn calculate_stats(&self, values: &[f64]) -> (f64, f64, f64) {
        if values.is_empty() {
            return (0.0, 0.0, 0.0);
        }

        let mut sums = [0.0; 4];
        let mut mins = [std::f64::INFINITY; 4];
        let mut maxs = [std::f64::NEG_INFINITY; 4];

        let chunks = values.chunks_exact(4);
        let remainder = chunks.remainder();

        for chunk in chunks {
            for i in 0..4 {
                sums[i] += chunk[i];
                mins[i] = mins[i].min(chunk[i]);
                maxs[i] = maxs[i].max(chunk[i]);
            }
        }

        for (i, &value) in remainder.iter().enumerate() {
            sums[i] += value;
            mins[i] = mins[i].min(value);
            maxs[i] = maxs[i].max(value);
        }

        let total_sum: f64 = sums.iter().sum();
        let global_min = mins.iter().fold(std::f64::INFINITY, |a, &b| a.min(b));
        let global_max = maxs.iter().fold(std::f64::NEG_INFINITY, |a, &b| a.max(b));

        let mean = total_sum / values.len() as f64;
        (mean, global_max, global_min)
    }

    /// Calculates statistics for a generation.
    fn calculate_generation_stats(&self, results: &[(ParameterSet, f64)], gen_idx: usize) -> GAStatsPerGeneration {
        let fitness_values: Vec<f64> = results.iter().map(|(_, f)| *f).collect();
        
        let (mean, best_fitness, worst_fitness) = self.calculate_stats(&fitness_values);

        let (best, worst, _) = match self.ga_config.fitness_direction.as_str() {
            "max" => (best_fitness, worst_fitness, std::cmp::Ordering::Greater),
            "min" => (worst_fitness, best_fitness, std::cmp::Ordering::Less),
            _ => (best_fitness, worst_fitness, std::cmp::Ordering::Greater),
        };

        let best_params = results.iter()
            .find(|(_, f)| (*f - best).abs() < 1e-8)
            .map(|(p, _)| p)
            .unwrap_or(&results[0].0);

        let best_chromosome_id = self.create_chromosome_id_for_display(best_params);

        GAStatsPerGeneration::new()
            .with_best_fitness(best)
            .with_worst_fitness(worst)
            .with_mean_fitness(mean)
            .with_best_chromosome_id(best_chromosome_id)
            .with_generation(gen_idx)

    }

    /// Performs crossover and mutation on two parent chromosomes to create a child.
    fn crossover_mutation(&self, a: &ParameterSet, b: &ParameterSet) -> ParameterSet {
        let mut new_params = Vec::new();

        for ((name_a, val_a), (_, val_b)) in a.strategy_params.iter().zip(b.strategy_params.iter()) {
            if rand::random::<f64>() < self.ga_config.p_crossover {
                new_params.push((name_a.clone(), val_a.clone()));
            } else {
                new_params.push((name_a.clone(), val_b.clone()));
            }
        }

        let pos_sizer_value = if rand::random::<f64>() < self.ga_config.p_mutation {
            self.optimization_config.pos_sizer_value_range
                .choose(&mut rand::thread_rng())
                .copied()
                .unwrap_or(a.pos_sizer_value)
        } else {
            a.pos_sizer_value
        };

        let slippage = if rand::random::<f64>() < self.ga_config.p_mutation {
            self.optimization_config.slippage_range
                .choose(&mut rand::thread_rng())
                .copied()
                .unwrap_or(a.slippage)
        } else {
            a.slippage
        };

        ParameterSet::new()
            .with_strategy_params(new_params)
            .with_pos_sizer_name(a.pos_sizer_name.clone())
            .with_pos_sizer_value(pos_sizer_value)
            .with_pos_sizer_additional_params(a.pos_sizer_additional_params.clone())
            .with_slippage(slippage)

    }

    /// Selects a parent chromosome using tournament selection.
    fn choose_parent(&self, results: &[(ParameterSet, f64)], rng: &mut impl rand::Rng) -> ParameterSet {
        let idx_a = rng.gen_range(0..results.len());
        let idx_b = rng.gen_range(0..results.len());
        let (params_a, fitness_a) = &results[idx_a];
        let (params_b, fitness_b) = &results[idx_b];
    
        match self.ga_config.fitness_direction.as_str() {
            "max" => {
                if fitness_a >= fitness_b {
                    params_a.clone()
                } else {
                    params_b.clone()
                }
            },
            "min" => {
                if fitness_a <= fitness_b {
                    params_a.clone()
                } else {
                    params_b.clone()
                }
            }
            _ => params_a.clone(),
        }
    }

    /// Performs tournament selection to create the next generation.
    fn tournament_selection(&self, results: &[(ParameterSet, f64)]) -> Vec<ParameterSet> {
        let mut next_gen = Vec::with_capacity(self.ga_config.population_size);
        let mut rng = rand::thread_rng();

        // Save bset individ
        let best_individ = match self.ga_config.fitness_direction.as_str() {
            "max" => results.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
            "min" => results.iter().min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
            _ => results.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal)),
        };

        if let Some((best_params, _)) = best_individ {
            next_gen.push(best_params.clone());
        }

        // tournament selection
        while next_gen.len() < self.ga_config.population_size {
            let parent_a = self.choose_parent(results, &mut rng);
            let parent_b = self.choose_parent(results, &mut rng);

            let child = self.crossover_mutation(&parent_a, &parent_b);
            next_gen.push(child);
        }

        next_gen
    }

}

/// Hashes a ParameterSet for caching fitness scores.
fn hash_parameter_set(params: &ParameterSet) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for (k, v) in &params.strategy_params {
        k.hash(&mut hasher);
        format!("{:?}", v).hash(&mut hasher);
    }
    params.pos_sizer_value.to_bits().hash(&mut hasher);
    params.slippage.to_bits().hash(&mut hasher);
    hasher.finish()
}

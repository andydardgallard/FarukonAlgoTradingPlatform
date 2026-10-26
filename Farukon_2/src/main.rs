// Farukon_2_0/src/main.rs

//! Main entry point of the Farukon backtesting engine.
//! Parses command-line arguments, loads settings and instruments,
//! and either runs optimization (Grid Search / Genetic Algorithm)
//! or single backtest (not implemented yet — optimization only).

mod backtest;
mod cli;
mod data_engine;
mod execution;
mod optimizers;
mod portfolio;
mod risks;
mod strategy_loader;

use anyhow::Context;

// mimalloc global allocator: rayon-parallel backtesting churns many small allocations per candidate, where the system allocator's locks become the bottleneck.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> anyhow::Result<()> {
    let start_time = std::time::Instant::now();

    let args = cli::Args::parse(); // Parse --config path

    // Load full settings (common + portfolio)
    let mut all_settings = farukon_core::settings::Settings::load(args.config)?;
    let common_settings = &all_settings.common.clone();

    // Load global instrument metadata
    let instruments_info =
        &farukon_core::instruments_info::InstrumentsInfoRegistry::load(&all_settings)?;

    // Load commission plans
    let _commission_plans =
        farukon_core::commission_plans::CommissionPlans::load(&mut all_settings, instruments_info)?;

    // For each strategy in portfolio, run optimization
    let mut portfolio_strategy_inputs: Vec<(String, f64, usize, f64)> = Vec::new();
    let mut reference_strategy_settings: Option<farukon_core::settings::StrategySettings> = None;
    for (_strategy_id, strategy_settings) in all_settings.portfolio {
        let strategy_instruments_info =
            &instruments_info.get_instrument_info_for_strategy(&strategy_settings.symbols)?;
        let initial_capital_for_strategy =
            strategy_settings.strategy_weight * all_settings.common.initial_capital;

        // load global data store
        let global_data_store = std::sync::Arc::new(
            data_engine::global_data_storage::GlobalDataStore::load(&strategy_settings).with_context(
                || {
                    format!(
                        "failed to load market data for strategy '{}' (data_path '{}', timeframe '{}', symbols {:?})",
                        strategy_settings.strategy_name,
                        strategy_settings.data.data_path,
                        strategy_settings.data.timeframe,
                        strategy_settings.symbols
                    )
                },
            )?,
        );

        if *global_data_store.is_loaded()
            && (common_settings.mode == "Optimize"
                || common_settings.mode == "Debug"
                || common_settings.mode == "Visual"
                || common_settings.mode == "Portfolio")
        {
            let new_runner = |common: &farukon_core::settings::CommonSettings| {
                optimizers::OptimizationRunner::new(
                    &initial_capital_for_strategy,
                    common,
                    &strategy_settings,
                    strategy_instruments_info,
                )
            };

            let execution = resolve_strategy_execution(
                common_settings.mode.as_str(),
                &strategy_settings.optimizer_type,
            );

            match execution {
                StrategyExecution::SingleCombinationGridSearch { mode } => {
                    let optimization_runner = new_runner(common_settings);
                    let results = run_single_combination_grid_search(
                        &optimization_runner,
                        &mode,
                        &strategy_settings,
                        global_data_store,
                    )?;
                    if mode == "Portfolio" {
                        let deals_count = results
                            .first()
                            .map(|r| *r.get_results().get_deals_count())
                            .unwrap_or(0);
                        portfolio_strategy_inputs.push((
                            format!(
                                "{}/equity_curve_{}.csv",
                                strategy_settings.exit_results_path,
                                strategy_settings.strategy_name
                            ),
                            initial_capital_for_strategy,
                            deals_count,
                            strategy_settings.strategy_weight,
                        ));
                        if reference_strategy_settings.is_none() {
                            reference_strategy_settings = Some(strategy_settings.clone());
                        }
                    }
                }
                StrategyExecution::ConfiguredOptimizer { mode } => {
                    // A population search configured for Visual mode runs with Optimize
                    // semantics (its results and statistics files) instead of racing every
                    // candidate on the single equity file of Visual mode.
                    let mut search_settings = common_settings.clone();
                    search_settings.mode = mode;
                    let optimization_runner = new_runner(&search_settings);
                    run_configured_optimizer(
                        optimization_runner,
                        &strategy_settings,
                        global_data_store,
                    )?;
                }
            }
        }
    }

    if common_settings.mode == "Portfolio" && !portfolio_strategy_inputs.is_empty() {
        let output_path = {
            let first_file = std::path::Path::new(&portfolio_strategy_inputs[0].0);
            let dir = first_file.parent().unwrap_or(std::path::Path::new("."));
            dir.join("equity_curve_portfolio.csv")
                .to_string_lossy()
                .to_string()
        };
        let equity_files: Vec<(String, f64)> = portfolio_strategy_inputs
            .iter()
            .map(|(p, ic, _, _)| (p.clone(), *ic))
            .collect();
        // Portfolio capital that no strategy manages (weights summing to less than 1) stays in
        // cash, so the aggregated equity curve must carry it exactly like the metrics do.
        let unallocated_capital =
            farukon_core::utils::unallocated_common_capital(&portfolio_strategy_inputs);
        farukon_core::utils::export_portfolio_equity_csv(
            &equity_files,
            unallocated_capital,
            &output_path,
        )?;

        if let Some(ref settings) = reference_strategy_settings {
            let results_path = std::path::Path::new(&output_path)
                .with_file_name("optimization_results_portfolio.csv")
                .to_string_lossy()
                .to_string();
            farukon_core::utils::export_portfolio_optimization_results_csv(
                &portfolio_strategy_inputs,
                settings,
                &results_path,
            )?;
        }
    }

    println!(
        "The main programm is finished in {:.3} seconds",
        start_time.elapsed().as_secs_f64()
    );
    anyhow::Ok(())
}

/// What one strategy executes for the configured mode.
#[derive(Debug, Clone, PartialEq, Eq)]
enum StrategyExecution {
    /// Exactly one Grid Search combination, run with the carried mode (`Visual`, `Portfolio`).
    SingleCombinationGridSearch { mode: String },
    /// The optimizer the strategy is configured with; `mode` is what the optimization runner sees.
    ConfiguredOptimizer { mode: String },
}

/// Resolves what the configured mode executes for a strategy with a given optimizer type.
///
/// `Portfolio` always runs a single Grid Search combination and ignores the configured optimizer:
/// the portfolio aggregation runs exactly one equity curve per strategy. `Visual` runs a single
/// Grid Search combination for a Grid Search strategy; a population search (Genetic, LSHADE-RSP)
/// runs the configured optimizer with `Optimize` semantics, because every candidate of a
/// population search would otherwise write the same `equity_series.csv` of Visual mode. Every
/// other mode runs the configured optimizer with its own mode unchanged.
fn resolve_strategy_execution(
    mode: &str,
    optimizer_type: &farukon_core::settings::OptimizerType,
) -> StrategyExecution {
    use farukon_core::settings::OptimizerType;
    match mode {
        "Portfolio" => StrategyExecution::SingleCombinationGridSearch {
            mode: mode.to_string(),
        },
        "Visual" => match optimizer_type {
            OptimizerType::GridSearch => StrategyExecution::SingleCombinationGridSearch {
                mode: mode.to_string(),
            },
            OptimizerType::Genetic { .. } | OptimizerType::LshadeRSP { .. } => {
                StrategyExecution::ConfiguredOptimizer {
                    mode: "Optimize".to_string(),
                }
            }
        },
        _ => StrategyExecution::ConfiguredOptimizer {
            mode: mode.to_string(),
        },
    }
}

/// Runs the optimizer the strategy is configured with, through the same dispatch as Optimize mode.
fn run_configured_optimizer(
    optimization_runner: optimizers::OptimizationRunner,
    strategy_settings: &farukon_core::settings::StrategySettings,
    global_data_store: std::sync::Arc<data_engine::global_data_storage::GlobalDataStore>,
) -> anyhow::Result<()> {
    match &strategy_settings.optimizer_type {
        farukon_core::settings::OptimizerType::GridSearch => {
            let total_combinations = optimization_runner
                .get_grid_search_optimizer()
                .calculate_total_combinations()?;

            let results =
                optimization_runner.run_grid_search(total_combinations, global_data_store)?;
            optimization_runner.save_grid_search_optimization_results(&results)?;
        }
        farukon_core::settings::OptimizerType::Genetic { ga_params } => {
            optimization_runner.run_genetic_search(ga_params, global_data_store)?;
        }
        farukon_core::settings::OptimizerType::LshadeRSP { lshade_params } => {
            optimization_runner.run_lshade_rsp_search(lshade_params, global_data_store)?;
        }
    }
    anyhow::Ok(())
}

/// Runs a Grid Search that has to evaluate exactly one combination (Visual, Portfolio) and
/// returns its results.
fn run_single_combination_grid_search(
    optimization_runner: &optimizers::OptimizationRunner,
    mode: &str,
    strategy_settings: &farukon_core::settings::StrategySettings,
    global_data_store: std::sync::Arc<data_engine::global_data_storage::GlobalDataStore>,
) -> anyhow::Result<Vec<farukon_core::optimization::OptimizationResult>> {
    let total_combinations = optimization_runner
        .get_grid_search_optimizer()
        .calculate_total_combinations()?;

    if total_combinations != 1 {
        anyhow::bail!(
            "mode '{}' runs exactly one parameter combination, but strategy '{}' has {}; use mode \"Optimize\" for a full Grid Search over the configured ranges",
            mode,
            strategy_settings.strategy_name,
            total_combinations
        );
    }

    let results = optimization_runner.run_grid_search(total_combinations, global_data_store)?;
    optimization_runner.save_grid_search_optimization_results(&results)?;
    anyhow::Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use farukon_core::settings::{
        FitnessParams, FitnessValue, GAParams, LshadeRspParams, OptimizerType,
    };

    fn fitness_params() -> FitnessParams {
        FitnessParams {
            fitness_direction: "max".to_string(),
            fitness_value: FitnessValue::TotalReturn,
        }
    }

    fn ga_params() -> GAParams {
        GAParams {
            population_size: 10,
            elite_size: 1,
            p_crossover: 0.8,
            p_mutation: 0.1,
            max_generations: 3,
            std_stop: 0.001,
            fitness_params: fitness_params(),
        }
    }

    fn lshade_params() -> LshadeRspParams {
        LshadeRspParams {
            population_size: 10,
            max_evaluations: 20,
            p_best: 0.1,
            archive_rate: 1.0,
            memory_size: 5,
            fitness_params: fitness_params(),
        }
    }

    fn single_grid_search(mode: &str) -> StrategyExecution {
        StrategyExecution::SingleCombinationGridSearch {
            mode: mode.to_string(),
        }
    }

    fn configured_optimizer(mode: &str) -> StrategyExecution {
        StrategyExecution::ConfiguredOptimizer {
            mode: mode.to_string(),
        }
    }

    /// Regression (finding C7): Visual mode with a Grid Search strategy runs exactly one
    /// combination, as the single equity curve of Visual mode requires.
    #[test]
    fn visual_grid_search_runs_a_single_combination() {
        assert_eq!(
            resolve_strategy_execution("Visual", &OptimizerType::GridSearch),
            single_grid_search("Visual")
        );
    }

    /// Regression (finding C7): Visual mode with a Genetic strategy must run the configured
    /// Genetic search instead of silently falling back to Grid Search, and must switch the runner
    /// to `Optimize` mode (every candidate would otherwise race on Visual's equity file).
    #[test]
    fn visual_genetic_runs_the_population_search_as_optimize() {
        assert_eq!(
            resolve_strategy_execution(
                "Visual",
                &OptimizerType::Genetic {
                    ga_params: ga_params()
                }
            ),
            configured_optimizer("Optimize")
        );
    }

    /// Regression (finding C7): the same for LSHADE-RSP, the second population optimizer.
    #[test]
    fn visual_lshade_runs_the_population_search_as_optimize() {
        assert_eq!(
            resolve_strategy_execution(
                "Visual",
                &OptimizerType::LshadeRSP {
                    lshade_params: lshade_params()
                }
            ),
            configured_optimizer("Optimize")
        );
    }

    /// Portfolio aggregation runs exactly one equity curve per strategy, so Portfolio stays a
    /// single Grid Search combination whatever optimizer the strategy is configured with.
    #[test]
    fn portfolio_stays_grid_only_for_every_optimizer() {
        for optimizer_type in [
            OptimizerType::GridSearch,
            OptimizerType::Genetic {
                ga_params: ga_params(),
            },
            OptimizerType::LshadeRSP {
                lshade_params: lshade_params(),
            },
        ] {
            assert_eq!(
                resolve_strategy_execution("Portfolio", &optimizer_type),
                single_grid_search("Portfolio"),
                "unexpected Portfolio execution for {optimizer_type:?}"
            );
        }
    }

    /// Optimize and Debug keep running the configured optimizer with their own mode; the Visual
    /// dispatch must not leak into them.
    #[test]
    fn optimize_and_debug_run_the_configured_optimizer_unchanged() {
        for mode in ["Optimize", "Debug"] {
            for optimizer_type in [
                OptimizerType::GridSearch,
                OptimizerType::Genetic {
                    ga_params: ga_params(),
                },
                OptimizerType::LshadeRSP {
                    lshade_params: lshade_params(),
                },
            ] {
                assert_eq!(
                    resolve_strategy_execution(mode, &optimizer_type),
                    configured_optimizer(mode),
                    "unexpected {mode} execution for {optimizer_type:?}"
                );
            }
        }
    }
}

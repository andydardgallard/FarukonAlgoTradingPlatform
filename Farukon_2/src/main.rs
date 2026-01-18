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
    for (_strategy_id, strategy_settings) in all_settings.portfolio {
        let strategy_instruments_info =
            &instruments_info.get_instrument_info_for_strategy(&strategy_settings.symbols)?;
        let initial_capital_for_strategy =
            strategy_settings.strategy_weight * all_settings.common.initial_capital;

        // load global data store
        let global_data_store = std::sync::Arc::new(
            data_engine::global_data_storage::GlobalDataStore::load(&strategy_settings)
                .expect("Failed to create GlobalDataStore"),
        );

        if *global_data_store.is_loaded() {
            if common_settings.mode == "Optimize"
                || common_settings.mode == "Debug"
                || common_settings.mode == "Visual"
            {
                let optimization_runner = optimizers::OptimizationRunner::new(
                    &initial_capital_for_strategy,
                    &common_settings,
                    &strategy_settings,
                    strategy_instruments_info,
                );

                match common_settings.mode.as_str() {
                    "Visual" => {
                        let total_combinations = optimization_runner
                            .get_grid_search_optimizer()
                            .calculate_total_combinations()?;

                        if total_combinations == 1 {
                            let results = optimization_runner
                                .run_grid_search(total_combinations, global_data_store);
                            optimization_runner.save_grid_search_optimization_results(&results)?;
                        } else {
                            anyhow::bail!("total combinations != 1. Found {}", total_combinations);
                        }
                    }
                    _ => match &strategy_settings.optimizer_type {
                        farukon_core::settings::OptimizerType::GridSearch => {
                            let total_combinations = optimization_runner
                                .get_grid_search_optimizer()
                                .calculate_total_combinations()?;

                            let results = optimization_runner
                                .run_grid_search(total_combinations, global_data_store);
                            optimization_runner.save_grid_search_optimization_results(&results)?;
                        }
                        farukon_core::settings::OptimizerType::Genetic { ga_params } => {
                            optimization_runner.run_genetic_search(ga_params, global_data_store)?;
                        }
                    },
                }
            }
        }
    }

    println!(
        "The main programm is finished in {:.3} seconds",
        start_time.elapsed().as_secs_f64()
    );
    anyhow::Ok(())
}

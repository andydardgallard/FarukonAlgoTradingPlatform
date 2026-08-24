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
    let mut portfolio_strategy_inputs: Vec<(String, f64, usize, f64)> = Vec::new();
    let mut reference_strategy_settings: Option<farukon_core::settings::StrategySettings> = None;
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
                || common_settings.mode == "Portfolio"
            {
                let optimization_runner = optimizers::OptimizationRunner::new(
                    &initial_capital_for_strategy,
                    &common_settings,
                    &strategy_settings,
                    strategy_instruments_info,
                );

                match common_settings.mode.as_str() {
                    "Visual" | "Portfolio" => {
                        let total_combinations = optimization_runner
                            .get_grid_search_optimizer()
                            .calculate_total_combinations()?;

                        if total_combinations == 1 {
                            let results = optimization_runner
                                .run_grid_search(total_combinations, global_data_store);
                            optimization_runner.save_grid_search_optimization_results(&results)?;
                            if common_settings.mode == "Portfolio" {
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
                        farukon_core::settings::OptimizerType::LshadeRSP { lshade_params } => {
                            optimization_runner
                                .run_lshade_rsp_search(lshade_params, global_data_store)?;
                        }
                    },
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
        farukon_core::utils::export_portfolio_equity_csv(&equity_files, &output_path)?;

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

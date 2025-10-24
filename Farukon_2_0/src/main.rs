// Farukon_2_0/src/main.rs

mod cli;
mod risks;
mod backtest;
mod portfolio;
mod execution;
mod optimizers;
mod data_handler;
mod ohlcv_generated;
mod stratagy_loader;

fn main() -> anyhow::Result<()>{
    let start_time = std::time::Instant::now();
    
    let args = cli::Args::parse();
    let instruments_info = &farukon_core::instruments_info::InstrumentsInfoRegistry::load()?;
    let all_settings = farukon_core::settings::Settings::load(args.config, instruments_info)?;
    let mode = &all_settings.common.mode;

    for (_strategy_id, strategy_settings) in all_settings.portfolio {
        let strategy_instruments_info = &instruments_info.get_instrument_info_for_strategy(&strategy_settings.symbols)?;
        let initial_capital_for_strategy = strategy_settings.strategy_weight * all_settings.common.initial_capital;

        if mode == "Optimize" || mode == "Debug"{
            let optimization_runner = optimizers::OptimizationRunner::new(
                mode,
                &initial_capital_for_strategy,
                &strategy_settings,
                strategy_instruments_info,
            );

            match &strategy_settings.optimizer_type {
                farukon_core::settings::OptimizerType::GridSearch => {
                    let total_combinations = optimization_runner
                        .get_grid_search_optimizer()
                        .calculate_total_combinations();
                    let combinations_to_grid_search = optimization_runner
                        .get_grid_search_optimizer()
                        .get_config()
                        .generate_all_combinations_vec();
                    let _results = optimization_runner.run_grid_search(total_combinations, combinations_to_grid_search);
                },
                farukon_core::settings::OptimizerType::Genetic { ga_params }=> {
                    optimization_runner.run_genetic_search(ga_params)?;
                }
            }
        }
    }

    println!("The main programm is finished in {:.3} seconds", start_time.elapsed().as_secs_f64());
    anyhow::Ok(())
}

// Farukon_2_0/src/backtest.rs

use anyhow::Context;

use crate::stratagy_loader;

pub struct Backtest {
    mode: String,
    strategy_settings: farukon_core::settings::StrategySettings,
    strategy_instruments_info: std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
    data_handler: Box<dyn farukon_core::data_handler::DataHandler>,
    event_receiver: std::sync::mpsc::Receiver<Box<dyn farukon_core::event::Event>>,
    dynamic_strategy: Box<stratagy_loader::DynamicStratagy>,
    portfolio: Box<dyn farukon_core::portfolio::PortfolioHandler>,
    execution_handler: Box<dyn farukon_core::execution::ExecutionHandler>,
}

impl Backtest {
    pub fn new(
        mode: &String,
        strategy_settings: &farukon_core::settings::StrategySettings,
        strategy_instruments_info: &std::collections::HashMap<String, farukon_core::instruments_info::InstrumentInfo>,
        data_handler: Box<dyn farukon_core::data_handler::DataHandler>,
        event_receiver: std::sync::mpsc::Receiver<Box<dyn farukon_core::event::Event>>,
        dynamic_strategy: Box<stratagy_loader::DynamicStratagy>,
        portfolio: Box<dyn farukon_core::portfolio::PortfolioHandler>,
        execution_handler: Box<dyn farukon_core::execution::ExecutionHandler>,
    ) -> Self {
        Backtest {
            mode: mode.to_string(),
            strategy_settings: strategy_settings.clone(),
            strategy_instruments_info: strategy_instruments_info.clone(),
            data_handler,
            event_receiver,
            dynamic_strategy,
            portfolio,
            execution_handler,
        }
    }

    fn run_backtest(&mut self) -> anyhow::Result<()> {
        let symbol_list = &self.strategy_settings.symbols;
        let mut fill_flag: Option<Box<dyn farukon_core::event::Event>> = None;

        loop {
            if self.data_handler.get_continue_backtest() {
                self.data_handler.update_bars();

            } else { break; }

            loop {
                match self.event_receiver.try_recv() {
                    Ok(event_box) => {
                        match event_box.event_type() {
                            "MARKET" => {
                                if self.mode == "Debug".to_string() {
                                    print!("Start event, {:?}, ", event_box);
                                    for symbol in symbol_list {
                                        print!("{}, {:?}, ", symbol, self.data_handler.get_latest_bar(symbol))
                                    }
                                    println!();
                                    println!("All position, {:?}", self.portfolio.get_all_positions());
                                    println!("All holdings, {:?}", self.portfolio.get_all_holdings());
                                    println!("All equity, {:?}", self.portfolio.get_all_equity_points())
                                }

                                if let Some(event) = fill_flag.as_ref() {
                                    if self.mode == "Debug".to_string() {
                                        println!("Start event, {:?}, ", event);
                                    }
                                    
                                    self.portfolio.update_fill(
                                        event.get_fill_event_params().unwrap(),
                                        &self.data_handler,
                                    );

                                    if self.mode == "Debug".to_string() {
                                        println!("Finish event, {:?}, ", event);
                                    }
                                };
                                fill_flag = None;
                                
                                if let Some(latest_equity_point) = self.portfolio.get_latest_equity_point() {
                                    if let Err(e) = self.dynamic_strategy.calculate_signals(
                                        &*self.data_handler,
                                        self.portfolio.get_current_positions(),
                                        latest_equity_point,
                                        symbol_list,
                                    ) {
                                        eprintln!("Error in Strategy::calculate_signals: {}", e);
                                        self.data_handler.set_continue_backtest(false);
                                        break;
                                    }
                                }

                                self.portfolio.update_timeindex(&self.data_handler);

                                if self.portfolio.get_latest_equity_point().unwrap().equity_point.capital < 0.0 {
                                    self.data_handler.set_continue_backtest(false);
                                    println!("STOP BACKTEST DUE TO NEGATIVE CAPITAL!");
                                }

                                if self.mode == "Debug".to_string() {
                                    print!("Finish event, {:?}, ", event_box);
                                    for symbol in symbol_list {
                                        print!("{}, {:?}, ", symbol, self.data_handler.get_latest_bar(symbol))
                                    }
                                    println!()
                                }
                            }
                            "SIGNAL" => {
                                if self.mode == "Debug".to_string() {
                                    println!("Start event, {:?}, ", event_box);
                                }

                                self.portfolio.update_signal(
                                    event_box.get_signal_event_params().unwrap(),
                                    &self.data_handler,
                                );
                                
                                if self.mode == "Debug".to_string() {
                                    println!("Finish event, {:?}, ", event_box);
                                }
                            }
                            "ORDER" => {
                                if self.mode == "Debug".to_string() {
                                    println!("Start event, {:?}, ", event_box);
                                }

                                self.execution_handler.execute_order(
                                    event_box.get_order_event_params().unwrap(),
                                    &self.strategy_instruments_info,
                                    &self.strategy_settings,
                                    &*self.data_handler
                                )?;

                                if self.mode == "Debug".to_string() {
                                    println!("Finish event, {:?}, ", event_box);
                                }
                            }
                            "FILL" => {
                                fill_flag = Some(event_box);
                            },
                            _ => {
                                println!("Received unknown event type: {}", event_box.event_type());
                            }
                        }
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => {
                        break;
                    }
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        eprintln!("Event channel disconnected.");
                        self.data_handler.set_continue_backtest(false);
                        break;
                    }
                }
            }

            if self.mode == "Debug".to_string() {
                println!("++++++++++++++++++++++++++++++++++++++");
            }

            if self.mode == "RealTime" {
                let heartbeat = 0.0;
                std::thread::sleep(std::time::Duration::from_secs_f64(heartbeat));
            }
        }
        
        anyhow::Ok(())
    }

    fn output_performance(&mut self) -> anyhow::Result<&farukon_core::performance::PerformanceMetrics> {
        self.portfolio.calculate_final_performance();

        match self.portfolio.output_summary_stats() {
            Ok(stats) => {
                anyhow::Ok(stats)
            }
            Err(e) => {
                eprintln!("Error generating performance summary stats: {}", e);
                Err(e)
            }
        }
    }

    pub fn simulate_trading(&mut self) -> anyhow::Result<&farukon_core::performance::PerformanceMetrics> {
        if self.mode == "Debug" {
            println!("Starting backtest simulation...");
        }

        self.run_backtest()
            .context("Backtest simulation failed")?;

        if self.mode == "Debug" {
            println!("all_positions: {:#?}", self.portfolio.get_all_positions());
            println!("all_holdings: {:#?}", self.portfolio.get_all_holdings());
            println!("all_equity: {:#?}", self.portfolio.get_all_equity_points());
        }
        
        let result = self.output_performance()
            .context("Failed to output performance")?;

        anyhow::Ok(result)
    }

}

// Farukon_2_0/src/cli.rs

/// Command-line argument parser using `clap`.
/// Parses the `--config` flag to locate the strategy configuration file.
#[derive(Debug)]
pub struct Args {
    pub config: std::path::PathBuf,
}

impl Args {
    /// Parses command-line arguments.
    /// Expects exactly one required argument: `--config <path>`
    /// # Returns
    /// * `Args` struct with parsed config path
    /// * Panics if required argument is missing (clap handles this)    
    pub fn parse() -> Self {
        let matches = clap::Command::new("Farukon")
            .version("2.0.0")
            .author("AndyDar")
            .about("Event-driven Backtester")
            .arg(
                clap::Arg::new("config")
                .short('c')
                .long("config")
                .help("Path to the settings.json configuration file")
                .required(true)
                .num_args(1),
            )
            .get_matches();

        Args {
            config: matches.get_one::<String>("config").unwrap().clone().into(),
        }
    }

}

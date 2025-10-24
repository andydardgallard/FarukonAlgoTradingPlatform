/// Structure representing command-line arguments.
#[derive(Debug)]
pub struct Args {
    pub config: std::path::PathBuf,
}

/// Command-line arguments parser using Clap.
///
/// Supports input/output paths, threading, and optional resampling with validation.
impl Args {
    /// Parses command-line arguments using `clap`.
    ///
    /// # Returns
    /// * `Args` - Struct containing parsed arguments.
    ///
    /// # Errors
    /// * If required arguments are missing or invalid.    
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

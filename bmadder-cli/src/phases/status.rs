use crate::logging;
use bmadder_core::config::Config;

pub fn run_status(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    logging::show_status(config)
}

use crate::logging;
use crate::story_io;
use bmadder_core::config::Config;

pub fn run_validate(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    logging::phase_banner("Phase: VALIDATE (Story file integrity check)");

    let errors = story_io::validate_stories(&config.paths.stories_dir)?;

    if errors.is_empty() {
        logging::ok("All story files are valid.");
    } else {
        for e in &errors {
            logging::err(e);
        }
        return Err(format!("{} story file(s) have errors.", errors.len()).into());
    }

    Ok(())
}

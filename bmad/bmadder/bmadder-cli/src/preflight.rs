use crate::logging;
use bmadder_core::config::Config;
use std::process::Command;

pub fn run_preflight(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    logging::phase_banner("Preflight Checks");

    // 1. Check pi.dev on PATH
    logging::info(&format!(
        "Checking pi.dev command: {}",
        config.pi_dev.command
    ));
    match Command::new(&config.pi_dev.command)
        .arg("--version")
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
                logging::ok(&format!("pi.dev found: {}", version));
            } else {
                logging::warn(&format!(
                    "pi.dev --version returned non-zero: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }
        Err(e) => {
            logging::err(&format!(
                "pi.dev ('{}') not found on PATH: {}",
                config.pi_dev.command, e
            ));
            return Err(format!(
                "pi.dev ('{}') not found on PATH. Please install it.",
                config.pi_dev.command
            )
            .into());
        }
    }

    // 2. Check for rogue env vars
    let rogue_vars = [
        "ANTHROPIC_API_KEY",
        "OPENAI_API_KEY",
        "GEMINI_API_KEY",
        "GOOGLE_API_KEY",
    ];

    let mut found_rogue = false;
    for var in &rogue_vars {
        if std::env::var(var).is_ok() {
            logging::warn(&format!(
                "Rogue env var '{}' is set. This may interfere with pi.dev model routing.",
                var
            ));
            found_rogue = true;
        }
    }
    if !found_rogue {
        logging::ok("No rogue API key env vars detected.");
    }

    // 3. Verify models are non-empty
    if config.models.is_empty() {
        logging::err("No [models] defined in bmadder.toml.");
        return Err("No models configured in bmadder.toml [models] section.".into());
    }
    logging::ok(&format!(
        "{} model(s) configured: {:?}",
        config.models.len(),
        config.models.keys().collect::<Vec<_>>()
    ));

    logging::ok("Preflight checks passed.");
    Ok(())
}

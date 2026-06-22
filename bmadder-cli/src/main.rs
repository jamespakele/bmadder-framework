mod agent;
mod bootstrap;
mod git;
mod logging;
mod phases;
mod prompts;
mod story_io;
mod ui;

use bmadder_core::config::Config;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process;

/// BMADder — AI-driven story management and implementation pipeline.
#[derive(Parser)]
#[command(name = "bmadder", version, about)]
struct Cli {
    /// Path to bmadder.toml config file (auto-discovered if omitted)
    #[arg(global = true, short = 'c', long = "config")]
    config_path: Option<PathBuf>,

    /// Max dev iterations per story
    #[arg(global = true, long = "max-iter", value_name = "N")]
    max_iter: Option<u32>,

    /// Max SM↔PO iterations
    #[arg(global = true, long = "max-sm-iter", value_name = "N")]
    max_sm_iter: Option<u32>,

    /// Max dev iterations
    #[arg(global = true, long = "max-dev-iter", value_name = "N")]
    max_dev_iter: Option<u32>,

    /// Dry-run mode: no agents invoked, no git commits
    #[arg(global = true, long = "dry-run")]
    dry_run: bool,

    /// Skip PO review gate
    #[arg(global = true, long = "skip-po")]
    skip_po: bool,

    /// Skip SM story creation
    #[arg(global = true, long = "skip-sm")]
    skip_sm: bool,

    /// Override AI agent/model
    #[arg(global = true, long = "agent")]
    agent: Option<String>,

    /// Skip git commits on QA pass
    #[arg(global = true, long = "no-commit")]
    no_commit: bool,

    /// Story timeout in seconds
    #[arg(global = true, long = "timeout", value_name = "SECONDS")]
    timeout: Option<u64>,

    /// Target a specific story by ID
    #[arg(global = true, long = "story")]
    story: Option<String>,

    /// Resume from existing stories (iterative mode)
    #[arg(global = true, long = "from-existing")]
    from_existing: bool,

    /// Start processing from a specific story ID (iterative mode)
    #[arg(global = true, long = "start-from")]
    start_from: Option<String>,

    /// Output as JSON instead of colored terminal output
    #[arg(global = true, long = "json")]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run SM → PO story planning phase
    Plan,
    /// Run dev implementation phase
    Dev,
    /// Run QA review phase
    Qa,
    /// Run full Plan → Dev → QA cycle
    Cycle,
    /// Run iterative end-to-end pipeline
    Iterative,
    /// Show project status
    Status,
    /// Serve the browser console UI backed by this BMADder runtime
    Ui {
        /// Interface to bind
        #[arg(long, default_value = "127.0.0.1")]
        host: String,
        /// Port to listen on
        #[arg(short, long, default_value_t = 7331)]
        port: u16,
    },
    /// Validate all story files
    Validate,
    /// Bootstrap a new BMADder project
    Bootstrap {
        /// Project directory to bootstrap (default: current directory)
        #[arg(default_value = ".")]
        project_dir: PathBuf,
    },
    /// Generate a fresh bmadder.toml template for comparison/upgrade
    NewConfig,
}

fn main() {
    let cli = Cli::parse();

    // Dispatch special commands that don't need a config
    if let Command::Bootstrap { project_dir } = &cli.command {
        if let Err(e) = bootstrap::run_bootstrap(project_dir) {
            eprintln!("Bootstrap error: {}", e);
            process::exit(2);
        }
        return;
    }

    if let Command::NewConfig = &cli.command {
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let existing = cwd.join("bmadder.toml");
        let target = if existing.exists() {
            cwd.join("bmadder.new.toml")
        } else {
            existing
        };
        let template = bootstrap::default_config_template();
        if let Err(e) = std::fs::write(&target, template) {
            eprintln!("Error writing config: {}", e);
            process::exit(2);
        }
        println!("Wrote fresh config to: {}", target.display());
        if target
            .file_name()
            .map(|n| n == "bmadder.new.toml")
            .unwrap_or(false)
        {
            println!("Compare with: diff bmadder.toml bmadder.new.toml");
        }
        return;
    }

    // Find and load config
    let config_path = match &cli.config_path {
        Some(p) => p.clone(),
        None => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
            match story_io::find_config(&cwd) {
                Some(p) => p,
                None => {
                    eprintln!("Error: bmadder.toml not found. Run 'bmadder bootstrap' first or use --config.");
                    process::exit(2);
                }
            }
        }
    };

    let mut config = match Config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config '{}': {}", config_path.display(), e);
            process::exit(2);
        }
    };

    // Apply env overrides
    config.apply_env_overrides();

    // Apply CLI overrides
    if cli.dry_run {
        config.dry_run = true;
    }
    if cli.json {
        config.json_output = true;
    }
    if let Some(agent) = &cli.agent {
        config.agent_override = Some(agent.clone());
    }
    if let Some(timeout) = cli.timeout {
        config.timeout_override = Some(timeout);
        config.defaults.story_timeout_seconds = timeout;
    }
    if let Some(n) = cli.max_iter {
        config.defaults.max_dev_iterations = n;
        config.defaults.max_sm_iterations = n;
    }
    if let Some(n) = cli.max_sm_iter {
        config.defaults.max_sm_iterations = n;
    }
    if let Some(n) = cli.max_dev_iter {
        config.defaults.max_dev_iterations = n;
    }

    // Dispatch to phase
    let result = match &cli.command {
        Command::Plan => phases::plan::run_plan(&config, cli.skip_sm, cli.skip_po),
        Command::Dev => phases::dev::run_dev(&config, cli.story.as_deref()),
        Command::Qa => phases::qa::run_qa(&config, cli.story.as_deref(), cli.no_commit),
        Command::Cycle => {
            phases::cycle::run_cycle(&config, cli.skip_sm, cli.skip_po, cli.no_commit)
        }
        Command::Iterative => phases::iterative::run_iterative(
            &config,
            cli.from_existing,
            cli.story.as_deref(),
            cli.start_from.as_deref(),
            cli.skip_po,
            cli.no_commit,
        ),
        Command::Status => {
            if cli.json {
                // JSON output mode for status
                let mut status = serde_json::json!({
                    "project_root": config.project_root.to_string_lossy(),
                    "counts": {},
                });
                let stories_dir = &config.paths.stories_dir;
                let statuses = bmadder_core::story::StoryStatus::all();
                let mut counts = serde_json::Map::new();
                for s in &statuses {
                    counts.insert(
                        s.label().to_string(),
                        serde_json::json!(story_io::count_by_status(stories_dir, *s)),
                    );
                }
                let total: usize = statuses
                    .iter()
                    .map(|s| story_io::count_by_status(stories_dir, *s))
                    .sum();
                counts.insert("total".to_string(), serde_json::json!(total));
                status["counts"] = serde_json::Value::Object(counts);
                println!("{}", serde_json::to_string_pretty(&status).unwrap());
                Ok(())
            } else {
                phases::status::run_status(&config)
            }
        }
        Command::Validate => phases::validate::run_validate(&config),
        Command::Ui { host, port } => ui::run_ui(&config, &config_path, host, *port),
        Command::Bootstrap { .. } | Command::NewConfig => {
            // Already handled above
            unreachable!()
        }
    };

    // Handle errors with exit codes
    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

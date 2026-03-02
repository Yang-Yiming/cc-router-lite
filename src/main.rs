mod config;
mod error;
mod settings;
mod state;

use std::path::PathBuf;
use std::process;

use clap::{Parser, Subcommand};

use crate::config::{load_config, resolve_profile};
use crate::error::CcrlError;

#[derive(Parser)]
#[command(name = "ccrl", about = "Claude Code Router Lite")]
struct Cli {
    /// Path to config file
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Inject profile into settings.json
    Set { name: String },
    /// Show active profile
    Now,
    /// List all profiles
    List,
    /// Shell export mode (ccrl <name>)
    #[command(external_subcommand)]
    Export(Vec<String>),
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        process::exit(1);
    }
}

fn default_config_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".config");
    p.push("ccr-lite");
    p.push("config.toml");
    p
}

fn settings_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".claude");
    p.push("settings.json");
    p
}

fn run(cli: Cli) -> Result<(), CcrlError> {
    match cli.command {
        Commands::Set { name } => cmd_set(&cli.config, &name),
        Commands::Now => cmd_now(),
        Commands::List => cmd_list(&cli.config),
        Commands::Export(args) => {
            let name = args
                .first()
                .ok_or_else(|| CcrlError::ProfileNotFound("(no name provided)".into()))?;
            cmd_export(&cli.config, name)
        }
    }
}

fn config_path(custom: &Option<PathBuf>) -> PathBuf {
    custom.clone().unwrap_or_else(default_config_path)
}

fn cmd_set(custom_config: &Option<PathBuf>, name: &str) -> Result<(), CcrlError> {
    let path = config_path(custom_config);
    let profiles = load_config(&path)?;
    let raw = profiles
        .get(name)
        .ok_or_else(|| CcrlError::ProfileNotFound(name.into()))?;
    let profile = resolve_profile(name, raw)?;

    // Collect old profile's env keys for cleanup
    let old_keys: Vec<String> = state::read_current()
        .and_then(|old_name| profiles.get(&old_name))
        .map(|old_raw| {
            let mut keys = vec![
                "ANTHROPIC_BASE_URL".to_string(),
                "ANTHROPIC_AUTH_TOKEN".to_string(),
            ];
            keys.extend(old_raw.env.keys().cloned());
            keys
        })
        .unwrap_or_default();

    settings::inject_profile(&settings_path(), &profile, &old_keys)?;
    state::write_current(name)?;
    println!("Profile '{}' activated", name);
    Ok(())
}

fn cmd_now() -> Result<(), CcrlError> {
    match state::read_current() {
        Some(name) => println!("{}", name),
        None => println!("No active profile"),
    }
    Ok(())
}

fn cmd_list(custom_config: &Option<PathBuf>) -> Result<(), CcrlError> {
    let path = config_path(custom_config);
    let profiles = load_config(&path)?;
    let current = state::read_current();
    let mut names: Vec<&String> = profiles.keys().collect();
    names.sort();
    for name in names {
        if current.as_deref() == Some(name.as_str()) {
            println!("* {}  (active)", name);
        } else {
            println!("  {}", name);
        }
    }
    Ok(())
}

fn cmd_export(custom_config: &Option<PathBuf>, name: &str) -> Result<(), CcrlError> {
    let path = config_path(custom_config);
    let profiles = load_config(&path)?;
    let raw = profiles
        .get(name)
        .ok_or_else(|| CcrlError::ProfileNotFound(name.into()))?;
    let profile = resolve_profile(name, raw)?;
    println!("export ANTHROPIC_BASE_URL='{}'", profile.url);
    println!("export ANTHROPIC_AUTH_TOKEN='{}'", profile.auth);
    for (k, v) in &profile.env {
        let s = match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        println!("export {}='{}'", k, s);
    }
    Ok(())
}

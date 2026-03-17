mod config;
mod error;
mod settings;
mod state;

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use console::style;
use dialoguer::Select;
use owo_colors::OwoColorize;

use crate::config::{load_config, resolve_profile};
use crate::error::CcrlError;

#[derive(Parser)]
#[command(name = "ccrl", about = "Claude Code Router Lite")]
struct Cli {
    /// Path to config file
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Inject profile into settings.json
    Set { name: String },
    /// Remove active profile from settings.json
    Unset,
    /// Show active profile
    Now,
    /// List all profiles
    List,
    /// Check connectivity for all profiles (or a named one)
    Check { name: Option<String> },
    /// Validate all profiles (env var resolution)
    Validate,
    /// Show differences between current and target profile
    Diff { name: String },
    /// Open config file in $EDITOR
    ConfigEdit,
    /// Generate shell completions
    Completions { shell: Shell },
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
        None => {
            if io::stdout().is_terminal() && io::stdin().is_terminal() {
                cmd_interactive(&cli.config)
            } else {
                Cli::command().print_help().map_err(CcrlError::from)
            }
        }
        Some(cmd) => match cmd {
            Commands::Set { name } => cmd_set(&cli.config, &name),
            Commands::Unset => cmd_unset(&cli.config),
            Commands::Now => cmd_now(),
            Commands::List => cmd_list(&cli.config),
            Commands::Check { name } => cmd_check(&cli.config, name.as_deref()),
            Commands::Validate => cmd_validate(&cli.config),
            Commands::Diff { name } => cmd_diff(&cli.config, &name),
            Commands::ConfigEdit => cmd_config_edit(&cli.config),
            Commands::Completions { shell } => cmd_completions(shell),
            Commands::Export(args) => {
                let name = args
                    .first()
                    .ok_or_else(|| CcrlError::ProfileNotFound("(no name provided)".into()))?;
                cmd_export(&cli.config, name)
            }
        },
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
    println!("Profile '{}' {}", name.bold(), "activated".green());
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
        let desc = profiles[name].description.as_deref().map(|d| format!(" — {}", d)).unwrap_or_default();
        if current.as_deref() == Some(name.as_str()) {
            println!("{} {}  {}{}", "*".cyan(), name.bold(), "(active)".cyan(), desc);
        } else {
            println!("  {}{}", name, desc);
        }
    }
    Ok(())
}

fn cmd_check(custom_config: &Option<PathBuf>, name: Option<&str>) -> Result<(), CcrlError> {
    let path = config_path(custom_config);
    let profiles = load_config(&path)?;
    let mut names: Vec<&String> = profiles.keys().collect();
    names.sort();
    for n in names {
        if let Some(filter) = name {
            if n != filter { continue; }
        }
        let raw = &profiles[n];
        let profile = match resolve_profile(n, raw) {
            Ok(p) => p,
            Err(e) => { println!("[✗] {:<20} {}", n, e); continue; }
        };
        let url = format!("{}/v1/models", profile.url.trim_end_matches('/'));
        let start = std::time::Instant::now();
        match ureq::get(&url).set("x-api-key", &profile.auth).call() {
            Ok(_) => println!("[{}] {:<20} 200 OK ({}ms)",
                "✓".green(), n.bold(), start.elapsed().as_millis()),
            Err(ureq::Error::Status(code, _)) => {
                let label = if code == 401 { "unauthorized" } else { "error" };
                println!("[{}] {:<20} {} {}",
                    "!".yellow(), n.bold(), code, label);
            }
            Err(e) => println!("[{}] {:<20} {}",
                "✗".red(), n.bold(), e),
        }
    }
    Ok(())
}

fn cmd_validate(custom_config: &Option<PathBuf>) -> Result<(), CcrlError> {
    let path = config_path(custom_config);
    let profiles = load_config(&path)?;
    let mut names: Vec<&String> = profiles.keys().collect();
    names.sort();
    for name in names {
        match resolve_profile(name, &profiles[name]) {
            Ok(_)  => println!("[{}] {}", "✓".green(), name.bold()),
            Err(e) => println!("[{}] {:<20} {}", "✗".red(), name.bold(), e),
        }
    }
    Ok(())
}

fn cmd_completions(shell: Shell) -> Result<(), CcrlError> {
    clap_complete::generate(shell, &mut Cli::command(), "ccrl", &mut io::stdout());
    Ok(())
}

fn cmd_interactive(custom_config: &Option<PathBuf>) -> Result<(), CcrlError> {
    let path = config_path(custom_config);
    let profiles = load_config(&path)?;
    let current = state::read_current();

    let mut names: Vec<&String> = profiles.keys().collect();
    names.sort();

    let items: Vec<String> = names
        .iter()
        .map(|name| {
            let desc = profiles[*name]
                .description
                .as_deref()
                .map(|d| format!(" — {}", d))
                .unwrap_or_default();

            if current.as_deref() == Some(name.as_str()) {
                format!("{} {}  {}{}",
                    style("*").cyan(),
                    style(name).bold(),
                    style("(active)").cyan(),
                    desc)
            } else {
                format!("  {}{}", name, desc)
            }
        })
        .collect();

    let default = current
        .as_ref()
        .and_then(|c| names.iter().position(|n| *n == c))
        .unwrap_or(0);

    let selection = Select::new()
        .with_prompt("Select a profile")
        .items(&items)
        .default(default)
        .interact_opt()
        .map_err(|e| CcrlError::Io(e.into()))?;

    match selection {
        Some(idx) => cmd_set(custom_config, names[idx]),
        None => Ok(()),
    }
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

fn cmd_config_edit(custom_config: &Option<PathBuf>) -> Result<(), CcrlError> {
    let path = config_path(custom_config);
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    std::process::Command::new(editor)
        .arg(&path)
        .status()?;

    Ok(())
}

fn cmd_unset(custom_config: &Option<PathBuf>) -> Result<(), CcrlError> {
    use std::fs;

    let current_name = match state::read_current() {
        Some(name) => name,
        None => {
            println!("No active profile to unset");
            return Ok(());
        }
    };

    let path = config_path(custom_config);
    let profiles = load_config(&path)?;

    let keys_to_remove: Vec<String> = profiles
        .get(&current_name)
        .map(|raw| {
            let mut keys = vec![
                "ANTHROPIC_BASE_URL".to_string(),
                "ANTHROPIC_AUTH_TOKEN".to_string(),
            ];
            keys.extend(raw.env.keys().cloned());
            keys
        })
        .unwrap_or_else(|| vec![
            "ANTHROPIC_BASE_URL".to_string(),
            "ANTHROPIC_AUTH_TOKEN".to_string(),
        ]);

    settings::remove_keys(&settings_path(), &keys_to_remove)?;

    let state_file = state::state_path();
    if state_file.exists() {
        fs::remove_file(state_file)?;
    }

    println!("Profile '{}' {}", current_name.bold(), "deactivated".green());
    Ok(())
}

fn cmd_diff(custom_config: &Option<PathBuf>, target_name: &str) -> Result<(), CcrlError> {
    use std::collections::HashSet;

    let path = config_path(custom_config);
    let profiles = load_config(&path)?;

    let target_raw = profiles
        .get(target_name)
        .ok_or_else(|| CcrlError::ProfileNotFound(target_name.into()))?;
    let target = resolve_profile(target_name, target_raw)?;

    let current = state::read_current()
        .and_then(|name| profiles.get(&name).map(|raw| (name, raw)))
        .and_then(|(name, raw)| resolve_profile(&name, raw).ok());

    println!("Switching to profile '{}':\n", target_name.bold());

    if let Some(curr) = &current {
        if curr.url != target.url {
            println!("  {} ANTHROPIC_BASE_URL", "~".yellow());
            println!("    {} {}", "-".red(), curr.url);
            println!("    {} {}", "+".green(), target.url);
        }

        if curr.auth != target.auth {
            println!("  {} ANTHROPIC_AUTH_TOKEN", "~".yellow());
            println!("    {} <changed>", "~".yellow());
        }

        let curr_keys: HashSet<_> = curr.env.keys().collect();
        let target_keys: HashSet<_> = target.env.keys().collect();

        for key in curr_keys.difference(&target_keys) {
            println!("  {} {}", "-".red(), key.red());
        }

        for key in target_keys.difference(&curr_keys) {
            println!("  {} {}", "+".green(), key.green());
        }

        for key in curr_keys.intersection(&target_keys) {
            if curr.env[*key] != target.env[*key] {
                println!("  {} {}", "~".yellow(), key.yellow());
            }
        }
    } else {
        println!("  {} ANTHROPIC_BASE_URL = {}", "+".green(), target.url);
        println!("  {} ANTHROPIC_AUTH_TOKEN = <set>", "+".green());
        for key in target.env.keys() {
            println!("  {} {}", "+".green(), key.green());
        }
    }

    Ok(())
}

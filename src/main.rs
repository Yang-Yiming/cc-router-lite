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

use crate::config::{
    global_config_path, load_global_config, load_profiles, parse_hex_color, resolve_profile,
    target_config_path, Target,
};
use crate::error::CcrlError;
use crate::state::CurrentState;

#[derive(Parser)]
#[command(name = "ccrl", about = "Codex/Claude Code Router Lite")]
struct Cli {
    /// Path to global config file
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Target backend
    #[arg(long, global = true)]
    target: Option<Target>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Inject profile into target settings
    Set { name: String },
    /// Remove active profile from target settings
    Unset,
    /// Show active target/profile
    Now,
    /// List profiles for the selected target
    List,
    /// Check connectivity for all profiles (or a named one)
    Check { name: Option<String> },
    /// Validate all profiles (env var resolution)
    Validate,
    /// Show differences between current and target profile
    Diff { name: String },
    /// Open target profile config in $EDITOR
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

fn claude_settings_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".claude");
    p.push("settings.json");
    p
}

fn run(cli: Cli) -> Result<(), CcrlError> {
    match cli.command {
        None => {
            if io::stdout().is_terminal() && io::stdin().is_terminal() {
                cmd_interactive(&cli.config, cli.target)
            } else {
                Cli::command().print_help().map_err(CcrlError::from)
            }
        }
        Some(cmd) => match cmd {
            Commands::Set { name } => cmd_set(&cli.config, cli.target, &name),
            Commands::Unset => cmd_unset(&cli.config, cli.target),
            Commands::Now => cmd_now(),
            Commands::List => cmd_list(&cli.config, cli.target),
            Commands::Check { name } => cmd_check(&cli.config, cli.target, name.as_deref()),
            Commands::Validate => cmd_validate(&cli.config, cli.target),
            Commands::Diff { name } => cmd_diff(&cli.config, cli.target, &name),
            Commands::ConfigEdit => cmd_config_edit(&cli.config, cli.target),
            Commands::Completions { shell } => cmd_completions(shell),
            Commands::Export(args) => {
                let name = args
                    .first()
                    .ok_or_else(|| CcrlError::ProfileNotFound("(no name provided)".into()))?;
                cmd_export(&cli.config, cli.target, name)
            }
        },
    }
}

fn apply_color(text: &str, color: Option<&str>) -> String {
    match color {
        Some("red") => text.red().to_string(),
        Some("green") => text.green().to_string(),
        Some("yellow") => text.yellow().to_string(),
        Some("blue") => text.blue().to_string(),
        Some("magenta") => text.magenta().to_string(),
        Some("cyan") => text.cyan().to_string(),
        Some("white") => text.white().to_string(),
        Some("black") => text.black().to_string(),
        Some(c) => {
            if let Some((r, g, b)) = parse_hex_color(c) {
                text.truecolor(r, g, b).to_string()
            } else {
                text.to_string()
            }
        }
        None => text.to_string(),
    }
}

fn selected_target(
    custom_config: &Option<PathBuf>,
    explicit: Option<Target>,
) -> Result<Target, CcrlError> {
    if let Some(target) = explicit {
        return Ok(target);
    }

    let path = global_config_path(custom_config);
    let global = load_global_config(&path)?;
    Ok(global.default_target)
}

fn profiles_path(custom_config: &Option<PathBuf>, target: Target) -> PathBuf {
    let global_path = global_config_path(custom_config);
    target_config_path(&global_path, target)
}

fn load_target_profiles(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
) -> Result<
    (
        Target,
        PathBuf,
        std::collections::HashMap<String, config::RawProfile>,
    ),
    CcrlError,
> {
    let target = selected_target(custom_config, explicit_target)?;
    ensure_target_implemented(target)?;
    let path = profiles_path(custom_config, target);
    let profiles = load_profiles(&path)?;
    Ok((target, path, profiles))
}

fn ensure_target_implemented(target: Target) -> Result<(), CcrlError> {
    match target {
        Target::Claude => Ok(()),
        Target::Codex => Err(CcrlError::TargetNotImplemented(target.to_string())),
    }
}

fn current_state_for_target(target: Target) -> Option<CurrentState> {
    state::read_current().filter(|state| state.target == target)
}

fn profile_keys(raw: &config::RawProfile) -> Vec<String> {
    let mut keys = vec![
        "ANTHROPIC_BASE_URL".to_string(),
        "ANTHROPIC_AUTH_TOKEN".to_string(),
    ];
    keys.extend(raw.env.keys().cloned());
    keys
}

fn cmd_set(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
    name: &str,
) -> Result<(), CcrlError> {
    let (target, _, profiles) = load_target_profiles(custom_config, explicit_target)?;
    let raw = profiles
        .get(name)
        .ok_or_else(|| CcrlError::ProfileNotFound(name.into()))?;
    let profile = resolve_profile(name, raw)?;

    let old_keys = current_state_for_target(target)
        .and_then(|current| profiles.get(&current.profile))
        .map(profile_keys)
        .unwrap_or_default();

    settings::inject_profile(&claude_settings_path(), &profile, &old_keys)?;
    state::write_current(&CurrentState {
        target,
        profile: name.to_string(),
    })?;
    println!(
        "Target '{}' profile '{}' {}",
        target,
        name.bold(),
        "activated".green()
    );
    Ok(())
}

fn cmd_now() -> Result<(), CcrlError> {
    match state::read_current() {
        Some(current) => println!("{}/{}", current.target, current.profile),
        None => println!("No active profile"),
    }
    Ok(())
}

fn cmd_list(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
) -> Result<(), CcrlError> {
    let (target, _, profiles) = load_target_profiles(custom_config, explicit_target)?;
    let current = current_state_for_target(target);
    let mut names: Vec<&String> = profiles.keys().collect();
    names.sort();
    for name in names {
        let raw = &profiles[name];
        let profile = resolve_profile(name, raw)?;
        let desc = profile
            .description
            .as_deref()
            .map(|d| format!(" - {}", d))
            .unwrap_or_default();
        if current.as_ref().map(|s| s.profile.as_str()) == Some(name.as_str()) {
            let colored_name = apply_color(&name.bold().to_string(), profile.color.as_deref());
            println!(
                "{} {}  {}{}",
                "*".cyan(),
                colored_name,
                "(active)".cyan(),
                desc
            );
        } else {
            let colored_name = apply_color(name, profile.color.as_deref());
            println!("  {}{}", colored_name, desc);
        }
    }
    Ok(())
}

fn cmd_check(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
    name: Option<&str>,
) -> Result<(), CcrlError> {
    let (_, _, profiles) = load_target_profiles(custom_config, explicit_target)?;
    let mut names: Vec<&String> = profiles.keys().collect();
    names.sort();
    for n in names {
        if let Some(filter) = name {
            if n != filter {
                continue;
            }
        }
        let raw = &profiles[n];
        let profile = match resolve_profile(n, raw) {
            Ok(p) => p,
            Err(e) => {
                println!("[✗] {:<20} {}", n, e);
                continue;
            }
        };
        let url = format!("{}/v1/models", profile.url.trim_end_matches('/'));
        let start = std::time::Instant::now();
        match ureq::get(&url).set("x-api-key", &profile.auth).call() {
            Ok(_) => println!(
                "[{}] {:<20} 200 OK ({}ms)",
                "✓".green(),
                n.bold(),
                start.elapsed().as_millis()
            ),
            Err(ureq::Error::Status(code, _)) => {
                let label = if code == 401 { "unauthorized" } else { "error" };
                println!("[{}] {:<20} {} {}", "!".yellow(), n.bold(), code, label);
            }
            Err(e) => println!("[{}] {:<20} {}", "✗".red(), n.bold(), e),
        }
    }
    Ok(())
}

fn cmd_validate(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
) -> Result<(), CcrlError> {
    let (_, _, profiles) = load_target_profiles(custom_config, explicit_target)?;
    let mut names: Vec<&String> = profiles.keys().collect();
    names.sort();
    for name in names {
        match resolve_profile(name, &profiles[name]) {
            Ok(_) => println!("[{}] {}", "✓".green(), name.bold()),
            Err(e) => println!("[{}] {:<20} {}", "✗".red(), name.bold(), e),
        }
    }
    Ok(())
}

fn cmd_completions(shell: Shell) -> Result<(), CcrlError> {
    clap_complete::generate(shell, &mut Cli::command(), "ccrl", &mut io::stdout());
    Ok(())
}

fn cmd_interactive(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
) -> Result<(), CcrlError> {
    let (target, _, profiles) = load_target_profiles(custom_config, explicit_target)?;
    let current = current_state_for_target(target);

    let mut names: Vec<&String> = profiles.keys().collect();
    names.sort();

    let items: Vec<String> = names
        .iter()
        .map(|name| {
            let raw = &profiles[*name];
            let profile = resolve_profile(name, raw).ok();
            let desc = raw
                .description
                .as_deref()
                .map(|d| format!(" - {}", d))
                .unwrap_or_default();

            if current.as_ref().map(|s| s.profile.as_str()) == Some(name.as_str()) {
                let colored_name = profile
                    .as_ref()
                    .and_then(|p| p.color.as_deref())
                    .map(|c| apply_color(&style(name).bold().to_string(), Some(c)))
                    .unwrap_or_else(|| style(name).bold().to_string());
                format!(
                    "{} {}  {}{}",
                    style("*").cyan(),
                    colored_name,
                    style("(active)").cyan(),
                    desc
                )
            } else {
                let colored_name = profile
                    .as_ref()
                    .and_then(|p| p.color.as_deref())
                    .map(|c| apply_color(name, Some(c)))
                    .unwrap_or_else(|| name.to_string());
                format!("  {}{}", colored_name, desc)
            }
        })
        .collect();

    let default = current
        .as_ref()
        .and_then(|c| names.iter().position(|n| *n == &c.profile))
        .unwrap_or(0);

    let selection = Select::new()
        .with_prompt(format!("Target: {} - select a profile", target))
        .items(&items)
        .default(default)
        .interact_opt()
        .map_err(|e| CcrlError::Io(e.into()))?;

    match selection {
        Some(idx) => cmd_set(custom_config, Some(target), names[idx]),
        None => Ok(()),
    }
}

fn cmd_export(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
    name: &str,
) -> Result<(), CcrlError> {
    let (_, _, profiles) = load_target_profiles(custom_config, explicit_target)?;
    let raw = profiles
        .get(name)
        .ok_or_else(|| CcrlError::ProfileNotFound(name.into()))?;
    let profile = resolve_profile(name, raw)?;
    println!(
        "export ANTHROPIC_BASE_URL='{}'",
        shell_single_quote(&profile.url)
    );
    println!(
        "export ANTHROPIC_AUTH_TOKEN='{}'",
        shell_single_quote(&profile.auth)
    );
    for (k, v) in &profile.env {
        let s = match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        println!("export {}='{}'", k, shell_single_quote(&s));
    }
    Ok(())
}

fn shell_single_quote(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

fn cmd_config_edit(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
) -> Result<(), CcrlError> {
    let target = selected_target(custom_config, explicit_target)?;
    ensure_target_implemented(target)?;
    let path = profiles_path(custom_config, target);
    let editor = std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    std::process::Command::new(editor).arg(&path).status()?;

    Ok(())
}

fn cmd_unset(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
) -> Result<(), CcrlError> {
    use std::fs;

    let target = selected_target(custom_config, explicit_target)?;
    ensure_target_implemented(target)?;

    let current = match current_state_for_target(target) {
        Some(current) => current,
        None => {
            println!("No active profile to unset");
            return Ok(());
        }
    };

    let profiles = load_profiles(&profiles_path(custom_config, target))?;
    let keys_to_remove = profiles
        .get(&current.profile)
        .map(profile_keys)
        .unwrap_or_else(|| {
            vec![
                "ANTHROPIC_BASE_URL".to_string(),
                "ANTHROPIC_AUTH_TOKEN".to_string(),
            ]
        });

    settings::remove_keys(&claude_settings_path(), &keys_to_remove)?;

    let state_file = state::state_path();
    if state_file.exists() {
        fs::remove_file(state_file)?;
    }

    println!(
        "Target '{}' profile '{}' {}",
        target,
        current.profile.bold(),
        "deactivated".green()
    );
    Ok(())
}

fn cmd_diff(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
    target_name: &str,
) -> Result<(), CcrlError> {
    use std::collections::HashSet;

    let (target, _, profiles) = load_target_profiles(custom_config, explicit_target)?;

    let target_raw = profiles
        .get(target_name)
        .ok_or_else(|| CcrlError::ProfileNotFound(target_name.into()))?;
    let next_profile = resolve_profile(target_name, target_raw)?;

    let current = current_state_for_target(target)
        .and_then(|state| profiles.get(&state.profile).map(|raw| (state.profile, raw)))
        .and_then(|(name, raw)| resolve_profile(&name, raw).ok());

    println!(
        "Switching to target '{}' profile '{}':\n",
        target,
        target_name.bold()
    );

    if let Some(curr) = &current {
        if curr.url != next_profile.url {
            println!("  {} ANTHROPIC_BASE_URL", "~".yellow());
            println!("    {} {}", "-".red(), curr.url);
            println!("    {} {}", "+".green(), next_profile.url);
        }

        if curr.auth != next_profile.auth {
            println!("  {} ANTHROPIC_AUTH_TOKEN", "~".yellow());
            println!("    {} <changed>", "~".yellow());
        }

        let curr_keys: HashSet<_> = curr.env.keys().collect();
        let target_keys: HashSet<_> = next_profile.env.keys().collect();

        for key in curr_keys.difference(&target_keys) {
            println!("  {} {}", "-".red(), key.red());
        }

        for key in target_keys.difference(&curr_keys) {
            println!("  {} {}", "+".green(), key.green());
        }

        for key in curr_keys.intersection(&target_keys) {
            if curr.env[*key] != next_profile.env[*key] {
                println!("  {} {}", "~".yellow(), key.yellow());
            }
        }
    } else {
        println!(
            "  {} ANTHROPIC_BASE_URL = {}",
            "+".green(),
            next_profile.url
        );
        println!("  {} ANTHROPIC_AUTH_TOKEN = <set>", "+".green());
        for key in next_profile.env.keys() {
            println!("  {} {}", "+".green(), key.green());
        }
    }

    Ok(())
}

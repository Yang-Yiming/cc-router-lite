mod codex;
mod config;
mod error;
mod settings;
mod state;
mod tui;

use std::collections::{HashMap, HashSet};
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use console::style;
use owo_colors::OwoColorize;
use serde_json::Value as JsonValue;

use crate::codex::OAUTH_PROFILE_NAME;
use crate::config::{
    global_config_path, load_global_config, load_profiles, parse_hex_color, resolve_profile,
    target_config_path, Profile, RawProfile, Target,
};
use crate::error::CcrlError;
use crate::state::CurrentState;
use crate::tui::TuiProfileItem;

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

fn claude_settings_path() -> PathBuf {
    let mut p = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    p.push(".claude");
    p.push("settings.json");
    p
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
) -> Result<(Target, HashMap<String, RawProfile>), CcrlError> {
    let target = selected_target(custom_config, explicit_target)?;
    let path = profiles_path(custom_config, target);
    let profiles = load_profiles(&path)?;
    Ok((target, profiles))
}

fn current_state_for_target(target: Target) -> Option<CurrentState> {
    state::read_current().filter(|state| state.target == target)
}

fn profile_keys(raw: &RawProfile) -> Vec<String> {
    let mut keys = vec![
        "ANTHROPIC_BASE_URL".to_string(),
        "ANTHROPIC_AUTH_TOKEN".to_string(),
    ];
    keys.extend(raw.env.keys().cloned());
    keys
}

fn shell_single_quote(value: &str) -> String {
    value.replace('\'', "'\"'\"'")
}

fn maybe_refresh_codex_oauth_snapshot() -> Result<bool, CcrlError> {
    codex::refresh_oauth_snapshot_if_needed(
        &codex::codex_auth_path(),
        &codex::oauth_snapshot_path(),
    )
}

fn codex_entries(profiles: &HashMap<String, RawProfile>) -> Vec<String> {
    let mut names: Vec<String> = profiles.keys().cloned().collect();
    names.sort();
    names.push(OAUTH_PROFILE_NAME.to_string());
    names
}

fn format_profile_item(
    name: &str,
    description: Option<&str>,
    color: Option<&str>,
    active: bool,
) -> String {
    let desc = description.map(|d| format!(" - {}", d)).unwrap_or_default();
    let label = if active {
        apply_color(&style(name).bold().to_string(), color)
    } else {
        apply_color(name, color)
    };

    if active {
        format!(
            "{} {}  {}{}",
            style("*").cyan(),
            label,
            style("(active)").cyan(),
            desc
        )
    } else {
        format!("  {}{}", label, desc)
    }
}

fn current_display_name(current: &CurrentState) -> &str {
    if current.is_oauth() {
        OAUTH_PROFILE_NAME
    } else {
        &current.profile
    }
}

fn load_profiles_for_tui(
    custom_config: &Option<PathBuf>,
    target: Target,
) -> Result<HashMap<String, RawProfile>, CcrlError> {
    match load_profiles(&profiles_path(custom_config, target)) {
        Ok(profiles) => Ok(profiles),
        Err(CcrlError::ConfigNotFound(_)) => Ok(HashMap::new()),
        Err(err) => Err(err),
    }
}

fn active_claude_profile_name(
    profiles: &HashMap<String, RawProfile>,
) -> Result<Option<String>, CcrlError> {
    let path = claude_settings_path();
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path)?;
    let root: JsonValue = serde_json::from_str(&content)?;
    let Some(env) = root.get("env").and_then(|value| value.as_object()) else {
        return Ok(None);
    };

    let base_url = env
        .get("ANTHROPIC_BASE_URL")
        .and_then(|value| value.as_str());
    let auth = env
        .get("ANTHROPIC_AUTH_TOKEN")
        .and_then(|value| value.as_str());

    for name in sorted_profile_names(profiles) {
        let profile = resolve_profile(&name, &profiles[&name])?;
        if base_url != Some(profile.url.as_str()) || auth != Some(profile.auth.as_str()) {
            continue;
        }

        let env_matches = profile
            .env
            .iter()
            .all(|(key, value)| env.get(key) == Some(value));
        if env_matches {
            return Ok(Some(name));
        }
    }

    Ok(None)
}

fn active_codex_profile_name(
    profiles: &HashMap<String, RawProfile>,
) -> Result<Option<String>, CcrlError> {
    if codex::current_auth_is_oauth(&codex::codex_auth_path())? {
        return Ok(Some(OAUTH_PROFILE_NAME.to_string()));
    }

    let Some(provider) = codex::current_model_provider(&codex::codex_config_path())? else {
        return Ok(None);
    };

    if profiles.contains_key(&provider) {
        Ok(Some(provider))
    } else {
        Ok(None)
    }
}

fn sorted_profile_names(profiles: &HashMap<String, RawProfile>) -> Vec<String> {
    let mut names: Vec<String> = profiles.keys().cloned().collect();
    names.sort();
    names
}

fn build_tui_items(
    target: Target,
    profiles: &HashMap<String, RawProfile>,
    active_profile: Option<&str>,
) -> Result<Vec<TuiProfileItem>, CcrlError> {
    let mut items = Vec::new();

    match target {
        Target::Claude => {
            for name in sorted_profile_names(profiles) {
                let profile = resolve_profile(&name, &profiles[&name])?;
                items.push(TuiProfileItem {
                    name,
                    description: profile.description,
                    color: profile.color,
                    active: active_profile == Some(profile.name.as_str()),
                });
            }
        }
        Target::Codex => {
            maybe_refresh_codex_oauth_snapshot()?;
            for name in codex_entries(profiles) {
                if name == OAUTH_PROFILE_NAME {
                    items.push(TuiProfileItem {
                        name,
                        description: Some("Restore saved OAuth auth".to_string()),
                        color: Some("magenta".to_string()),
                        active: active_profile == Some(OAUTH_PROFILE_NAME),
                    });
                } else {
                    let profile = resolve_profile(&name, &profiles[&name])?;
                    items.push(TuiProfileItem {
                        name,
                        description: profile.description,
                        color: profile.color,
                        active: active_profile == Some(profile.name.as_str()),
                    });
                }
            }
        }
    }

    Ok(items)
}

fn cmd_set(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
    name: &str,
) -> Result<(), CcrlError> {
    let (target, profiles) = load_target_profiles(custom_config, explicit_target)?;
    match target {
        Target::Claude => cmd_set_claude(target, &profiles, name),
        Target::Codex => cmd_set_codex(target, &profiles, name),
    }
}

fn cmd_set_claude(
    target: Target,
    profiles: &HashMap<String, RawProfile>,
    name: &str,
) -> Result<(), CcrlError> {
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
        mode: None,
    })?;
    println!(
        "Target '{}' profile '{}' {}",
        target,
        name.bold(),
        "activated".green()
    );
    Ok(())
}

fn cmd_set_codex(
    target: Target,
    profiles: &HashMap<String, RawProfile>,
    name: &str,
) -> Result<(), CcrlError> {
    maybe_refresh_codex_oauth_snapshot()?;

    if name == OAUTH_PROFILE_NAME {
        codex::restore_oauth_snapshot(&codex::codex_auth_path(), &codex::oauth_snapshot_path())?;
        codex::clear_model_provider(&codex::codex_config_path())?;
        state::write_current(&CurrentState::oauth(target))?;
        println!(
            "Target '{}' profile '{}' {}",
            target,
            OAUTH_PROFILE_NAME.bold(),
            "activated".green()
        );
        return Ok(());
    }

    let raw = profiles
        .get(name)
        .ok_or_else(|| CcrlError::ProfileNotFound(name.into()))?;
    let profile = resolve_profile(name, raw)?;

    codex::set_model_provider(
        &codex::codex_config_path(),
        name,
        &profile.url,
        &profile.wire_api,
        profile.requires_openai_auth,
    )?;
    codex::write_api_key_auth(&codex::codex_auth_path(), &profile.auth)?;
    state::write_current(&CurrentState {
        target,
        profile: name.to_string(),
        mode: None,
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
        Some(current) => println!("{}/{}", current.target, current_display_name(&current)),
        None => println!("No active profile"),
    }
    Ok(())
}

fn cmd_list(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
) -> Result<(), CcrlError> {
    let (target, profiles) = load_target_profiles(custom_config, explicit_target)?;
    let current = current_state_for_target(target);

    match target {
        Target::Claude => {
            let mut names: Vec<&String> = profiles.keys().collect();
            names.sort();
            for name in names {
                let profile = resolve_profile(name, &profiles[name])?;
                let active = current.as_ref().map(current_display_name) == Some(name.as_str());
                println!(
                    "{}",
                    format_profile_item(
                        name,
                        profile.description.as_deref(),
                        profile.color.as_deref(),
                        active,
                    )
                );
            }
        }
        Target::Codex => {
            maybe_refresh_codex_oauth_snapshot()?;
            for name in codex_entries(&profiles) {
                if name == OAUTH_PROFILE_NAME {
                    let active = current
                        .as_ref()
                        .map(CurrentState::is_oauth)
                        .unwrap_or(false);
                    println!(
                        "{}",
                        format_profile_item(
                            &name,
                            Some("Restore saved OAuth auth"),
                            Some("magenta"),
                            active
                        )
                    );
                } else {
                    let profile = resolve_profile(&name, &profiles[&name])?;
                    let active = current.as_ref().map(current_display_name) == Some(name.as_str());
                    println!(
                        "{}",
                        format_profile_item(
                            &name,
                            profile.description.as_deref(),
                            profile.color.as_deref(),
                            active,
                        )
                    );
                }
            }
        }
    }
    Ok(())
}

fn cmd_check(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
    name: Option<&str>,
) -> Result<(), CcrlError> {
    let (_, profiles) = load_target_profiles(custom_config, explicit_target)?;
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
    let (_, profiles) = load_target_profiles(custom_config, explicit_target)?;
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
    let target = if let Some(target) = explicit_target {
        target
    } else if let Some(current) = state::read_current() {
        current.target
    } else {
        selected_target(custom_config, None)?
    };

    let claude_profiles = load_profiles_for_tui(custom_config, Target::Claude)?;
    let codex_profiles = load_profiles_for_tui(custom_config, Target::Codex)?;
    let claude_active = active_claude_profile_name(&claude_profiles)?;
    let codex_active = active_codex_profile_name(&codex_profiles)?;

    let claude_items = build_tui_items(Target::Claude, &claude_profiles, claude_active.as_deref())?;
    let codex_items = build_tui_items(Target::Codex, &codex_profiles, codex_active.as_deref())?;

    if let Some(selection) = tui::run(target, claude_items, codex_items)? {
        cmd_set(custom_config, Some(selection.target), &selection.profile)?;
    }

    Ok(())
}

fn cmd_export(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
    name: &str,
) -> Result<(), CcrlError> {
    let (target, profiles) = load_target_profiles(custom_config, explicit_target)?;
    if target == Target::Codex && name == OAUTH_PROFILE_NAME {
        return Err(CcrlError::OAuthSnapshotMissing);
    }

    let raw = profiles
        .get(name)
        .ok_or_else(|| CcrlError::ProfileNotFound(name.into()))?;
    let profile = resolve_profile(name, raw)?;

    match target {
        Target::Claude => {
            println!(
                "export ANTHROPIC_BASE_URL='{}'",
                shell_single_quote(&profile.url)
            );
            println!(
                "export ANTHROPIC_AUTH_TOKEN='{}'",
                shell_single_quote(&profile.auth)
            );
        }
        Target::Codex => {
            println!(
                "export OPENAI_API_KEY='{}'",
                shell_single_quote(&profile.auth)
            );
        }
    }

    for (k, v) in &profile.env {
        let s = match v {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        println!("export {}='{}'", k, shell_single_quote(&s));
    }
    Ok(())
}

fn cmd_config_edit(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
) -> Result<(), CcrlError> {
    let target = selected_target(custom_config, explicit_target)?;
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
    let current = match current_state_for_target(target) {
        Some(current) => current,
        None => {
            println!("No active profile to unset");
            return Ok(());
        }
    };

    match target {
        Target::Claude => {
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
        }
        Target::Codex => {
            maybe_refresh_codex_oauth_snapshot()?;
            codex::clear_model_provider(&codex::codex_config_path())?;
            if codex::has_oauth_snapshot(&codex::oauth_snapshot_path()) {
                codex::restore_oauth_snapshot(
                    &codex::codex_auth_path(),
                    &codex::oauth_snapshot_path(),
                )?;
            }
        }
    }

    let state_file = state::state_path();
    if state_file.exists() {
        fs::remove_file(state_file)?;
    }

    println!(
        "Target '{}' profile '{}' {}",
        target,
        current_display_name(&current).bold(),
        "deactivated".green()
    );
    Ok(())
}

fn cmd_diff(
    custom_config: &Option<PathBuf>,
    explicit_target: Option<Target>,
    target_name: &str,
) -> Result<(), CcrlError> {
    let (target, profiles) = load_target_profiles(custom_config, explicit_target)?;

    match target {
        Target::Claude => cmd_diff_claude(target, &profiles, target_name),
        Target::Codex => cmd_diff_codex(&profiles, target_name),
    }
}

fn cmd_diff_claude(
    target: Target,
    profiles: &HashMap<String, RawProfile>,
    target_name: &str,
) -> Result<(), CcrlError> {
    let target_raw = profiles
        .get(target_name)
        .ok_or_else(|| CcrlError::ProfileNotFound(target_name.into()))?;
    let next_profile = resolve_profile(target_name, target_raw)?;

    let current = current_state_for_target(target)
        .filter(|state| !state.is_oauth())
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

fn cmd_diff_codex(
    profiles: &HashMap<String, RawProfile>,
    target_name: &str,
) -> Result<(), CcrlError> {
    maybe_refresh_codex_oauth_snapshot()?;

    let current = current_state_for_target(Target::Codex);
    let current_provider = codex::current_model_provider(&codex::codex_config_path())?;
    let current_oauth = codex::current_auth_is_oauth(&codex::codex_auth_path())?;

    println!(
        "Switching to target '{}' profile '{}':\n",
        Target::Codex,
        target_name.bold()
    );

    if target_name == OAUTH_PROFILE_NAME {
        if let Some(provider) = current_provider {
            println!("  {} model_provider", "~".yellow());
            println!("    {} {}", "-".red(), provider);
            println!("    {} <unset>", "+".green());
        }
        if !current_oauth {
            println!("  {} auth_mode", "~".yellow());
            println!("    {} api-key", "-".red());
            println!("    {} OAuth", "+".green());
        }
        return Ok(());
    }

    let target_raw = profiles
        .get(target_name)
        .ok_or_else(|| CcrlError::ProfileNotFound(target_name.into()))?;
    let next_profile = resolve_profile(target_name, target_raw)?;

    if current_provider.as_deref() != Some(target_name) {
        println!("  {} model_provider", "~".yellow());
        if let Some(provider) = current_provider {
            println!("    {} {}", "-".red(), provider);
        } else {
            println!("    {} <unset>", "-".red());
        }
        println!("    {} {}", "+".green(), target_name);
    }

    if current_oauth {
        println!("  {} auth_mode", "~".yellow());
        println!("    {} OAuth", "-".red());
        println!("    {} api-key", "+".green());
    }

    let current_profile = current
        .filter(|state| !state.is_oauth())
        .and_then(|state| profiles.get(&state.profile).map(|raw| (state.profile, raw)))
        .and_then(|(name, raw)| resolve_profile(&name, raw).ok());

    if let Some(curr) = current_profile {
        compare_codex_provider_fields(&curr, &next_profile);
    } else {
        println!("  {} base_url = {}", "+".green(), next_profile.url);
        println!("  {} wire_api = {}", "+".green(), next_profile.wire_api);
        println!(
            "  {} requires_openai_auth = {}",
            "+".green(),
            next_profile.requires_openai_auth
        );
    }

    Ok(())
}

fn compare_codex_provider_fields(current: &Profile, next: &Profile) {
    if current.url != next.url {
        println!("  {} base_url", "~".yellow());
        println!("    {} {}", "-".red(), current.url);
        println!("    {} {}", "+".green(), next.url);
    }

    if current.wire_api != next.wire_api {
        println!("  {} wire_api", "~".yellow());
        println!("    {} {}", "-".red(), current.wire_api);
        println!("    {} {}", "+".green(), next.wire_api);
    }

    if current.requires_openai_auth != next.requires_openai_auth {
        println!("  {} requires_openai_auth", "~".yellow());
        println!("    {} {}", "-".red(), current.requires_openai_auth);
        println!("    {} {}", "+".green(), next.requires_openai_auth);
    }
}

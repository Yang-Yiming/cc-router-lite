mod codex;
mod config;
mod error;
mod settings;
mod state;

use std::collections::{HashMap, HashSet};
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::process;

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use console::style;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{self, ClearType},
};
use owo_colors::OwoColorize;

use crate::codex::OAUTH_PROFILE_NAME;
use crate::config::{
    global_config_path, load_global_config, load_profiles, parse_hex_color, resolve_profile,
    target_config_path, Profile, RawProfile, Target,
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

fn load_target_profiles_optional(
    custom_config: &Option<PathBuf>,
    target: Target,
) -> Result<HashMap<String, RawProfile>, CcrlError> {
    let path = profiles_path(custom_config, target);
    match load_profiles(&path) {
        Ok(profiles) => Ok(profiles),
        Err(CcrlError::ConfigNotFound(_)) => Ok(HashMap::new()),
        Err(e) => Err(e),
    }
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

struct TuiTargetView {
    target: Target,
    names: Vec<String>,
    profiles: HashMap<String, RawProfile>,
}

impl TuiTargetView {
    fn new(target: Target, profiles: HashMap<String, RawProfile>) -> Self {
        let mut names: Vec<String> = profiles.keys().cloned().collect();
        names.sort();
        if target == Target::Codex {
            names.push(OAUTH_PROFILE_NAME.to_string());
        }
        Self {
            target,
            names,
            profiles,
        }
    }

    fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    fn active_name(&self, current: Option<&CurrentState>) -> Option<String> {
        current.and_then(|state| {
            if state.target != self.target {
                None
            } else if state.is_oauth() {
                Some(OAUTH_PROFILE_NAME.to_string())
            } else {
                Some(state.profile.clone())
            }
        })
    }
}

struct TuiApp {
    views: [TuiTargetView; 2],
    active_target: Target,
    selected: [usize; 2],
}

impl TuiApp {
    fn new(
        claude: HashMap<String, RawProfile>,
        codex: HashMap<String, RawProfile>,
        active_target: Target,
        current: Option<CurrentState>,
    ) -> Self {
        let views = [
            TuiTargetView::new(Target::Claude, claude),
            TuiTargetView::new(Target::Codex, codex),
        ];
        let mut selected = [0usize; 2];
        for (idx, view) in views.iter().enumerate() {
            if let Some(active) = view.active_name(current.as_ref()) {
                if let Some(pos) = view.names.iter().position(|name| name == &active) {
                    selected[idx] = pos;
                }
            }
        }
        Self {
            views,
            active_target,
            selected,
        }
    }

    fn view(&self, target: Target) -> &TuiTargetView {
        match target {
            Target::Claude => &self.views[0],
            Target::Codex => &self.views[1],
        }
    }

    fn active_index(&self) -> usize {
        match self.active_target {
            Target::Claude => 0,
            Target::Codex => 1,
        }
    }

    fn move_selection(&mut self, delta: i32) {
        let idx = self.active_index();
        let view = self.view(self.active_target);
        if view.is_empty() {
            return;
        }
        let len = view.names.len() as i32;
        let current = self.selected[idx] as i32;
        let next = (current + delta).rem_euclid(len);
        self.selected[idx] = next as usize;
    }

    fn current_name(&self) -> Option<&str> {
        let view = self.view(self.active_target);
        if view.is_empty() {
            None
        } else {
            Some(view.names[self.selected[self.active_index()]].as_str())
        }
    }
}

fn render_tui(app: &TuiApp, current: Option<&CurrentState>) -> Result<(), CcrlError> {
    let mut stdout = io::stdout();
    execute!(
        stdout,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0)
    )?;

    let claude_active = app.active_target == Target::Claude;
    let codex_active = app.active_target == Target::Codex;
    let tabs = format!(
        "{} {}",
        if claude_active {
            style("Claude").bold().green().to_string()
        } else {
            style("Claude").dim().to_string()
        },
        if codex_active {
            style("Codex").bold().green().to_string()
        } else {
            style("Codex").dim().to_string()
        }
    );
    println!("{tabs}");
    println!(
        "{}",
        style("Use Tab to switch target, Enter to activate, q to quit").cyan()
    );
    println!();

    let view = app.view(app.active_target);
    if view.is_empty() {
        println!("{}", style("No profiles configured").yellow());
        stdout.flush()?;
        return Ok(());
    }

    let current_name = view.active_name(current);
    for (idx, name) in view.names.iter().enumerate() {
        let prefix = if idx == app.selected[app.active_index()] {
            ">"
        } else {
            " "
        };
        if name == OAUTH_PROFILE_NAME {
            let active = current_name.as_deref() == Some(OAUTH_PROFILE_NAME);
            let label = if active {
                style(name).bold().magenta().to_string()
            } else {
                style(name).magenta().to_string()
            };
            println!(
                "{} {}{}",
                prefix,
                label,
                if active { "  (active)" } else { "" }
            );
        } else {
            let profile = resolve_profile(name, &view.profiles[name])?;
            let active = current_name.as_deref() == Some(name.as_str());
            let label = if active {
                apply_color(&style(name).bold().to_string(), profile.color.as_deref())
            } else {
                apply_color(name, profile.color.as_deref())
            };
            let desc = profile
                .description
                .as_deref()
                .map(|d| format!(" - {}", d))
                .unwrap_or_default();
            println!(
                "{} {}{}{}",
                prefix,
                label,
                if active { "  (active)" } else { "" },
                desc
            );
        }
    }

    stdout.flush()?;
    Ok(())
}

fn cleanup_tui(stdout: &mut io::Stdout) -> Result<(), CcrlError> {
    execute!(stdout, terminal::LeaveAlternateScreen, cursor::Show)?;
    terminal::disable_raw_mode()?;
    Ok(())
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
    let start_target = if let Some(target) = explicit_target {
        target
    } else if let Some(current) = state::read_current() {
        current.target
    } else {
        selected_target(custom_config, None)?
    };

    let claude_profiles = load_target_profiles_optional(custom_config, Target::Claude)?;
    let codex_profiles = load_target_profiles_optional(custom_config, Target::Codex)?;
    let current = state::read_current();
    let current_for_tui = current.clone();
    let mut app = TuiApp::new(claude_profiles, codex_profiles, start_target, current);

    if app.active_target == Target::Codex {
        maybe_refresh_codex_oauth_snapshot()?;
    }

    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, terminal::EnterAlternateScreen, cursor::Hide)?;

    let mut selected: Option<(Target, String)> = None;
    let result = (|| -> Result<(), CcrlError> {
        loop {
            render_tui(&app, current_for_tui.as_ref())?;
            let event = event::read().map_err(CcrlError::from)?;
            let key = match event {
                Event::Key(key) => key,
                _ => continue,
            };

            match key {
                KeyEvent {
                    code: KeyCode::Char('q'),
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Esc, ..
                } => break,
                KeyEvent {
                    code: KeyCode::Char('c'),
                    modifiers,
                    ..
                } if modifiers.contains(event::KeyModifiers::CONTROL) => break,
                KeyEvent {
                    code: KeyCode::Tab, ..
                }
                | KeyEvent {
                    code: KeyCode::Right,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::BackTab,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Left,
                    ..
                } => {
                    app.active_target = match app.active_target {
                        Target::Claude => Target::Codex,
                        Target::Codex => Target::Claude,
                    };
                    if app.active_target == Target::Codex {
                        maybe_refresh_codex_oauth_snapshot()?;
                    }
                }
                KeyEvent {
                    code: KeyCode::Up, ..
                }
                | KeyEvent {
                    code: KeyCode::Char('k'),
                    ..
                } => app.move_selection(-1),
                KeyEvent {
                    code: KeyCode::Down,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Char('j'),
                    ..
                } => app.move_selection(1),
                KeyEvent {
                    code: KeyCode::Enter,
                    ..
                } => {
                    if let Some(name) = app.current_name() {
                        selected = Some((app.active_target, name.to_string()));
                        break;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    })();

    cleanup_tui(&mut stdout)?;
    result?;
    if let Some((target, name)) = selected {
        cmd_set(custom_config, Some(target), &name)
    } else {
        Ok(())
    }
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

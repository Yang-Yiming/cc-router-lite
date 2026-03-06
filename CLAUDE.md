# Claude Code Router Lite

A lightweight Rust CLI tool (`ccrl`) for switching between multiple Claude Code API profiles. Injects env variables into `~/.claude/settings.json` or exports them to the current shell.

# Documents

Always READ:
- DOCS/REQUIREMENTS.md : the original requirements
- DOCS/PLAN.md : implementation plan and progress
- DOCS/ARCHITECTURE.md: design architecture

Always update them when you do edits.

(if PLAN.md is not clear enough, you can use `git log`, the commit messages are detailed)

# Key Paths

- Config: `~/.config/ccr-lite/config.toml` (profile definitions)
- Settings: `~/.claude/settings.json` (hardcoded, no `settings_file` config)
- State: `~/.config/ccr-lite/.current` (active profile name)

# Commands

- `ccrl` — interactive profile selector (arrow keys + Enter)
- `ccrl set <name>` — inject profile into settings.json
- `ccrl now` — show active profile
- `ccrl list` — list all profiles
- `ccrl <name>` — output shell export statements (use with `eval`)

# Code Structure

```
src/
  main.rs       — clap CLI + command dispatch
  config.rs     — config.toml parsing, EnvOrLiteral, Profile
  settings.rs   — settings.json env injection
  state.rs      — .current read/write
  error.rs      — CcrlError enum
```

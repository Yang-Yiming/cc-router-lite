# ccrl — Claude Code Router Lite

A lightweight CLI for switching between multiple Claude Code API profiles.

## Features

- Switch profiles by injecting env vars into `~/.claude/settings.json`
- Export env vars to the current shell session via `eval`
- Track the active profile across sessions
- Support `$ENV_VAR` references in config values

## Install

```sh
cargo install --path .
```

## Config

Create `~/.config/ccr-lite/config.toml`:

```toml
[work]
url = "https://api.anthropic.com"
auth = "$ANTHROPIC_API_KEY_WORK"

[personal]
url = "https://api.anthropic.com"
auth = "sk-ant-xxxx"

[personal.env]
ANTHROPIC_SMALL_FAST_MODEL = "claude-haiku-4-5-20251001"
```

## Usage

```sh
# Inject a profile into ~/.claude/settings.json (persistent)
ccrl set work

# Show the active profile
ccrl now

# List all profiles
ccrl list

# Export env vars to the current shell (temporary)
eval "$(ccrl personal)"
```

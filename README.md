# ccrl — Codex/Claude Code Router Lite

A lightweight CLI for switching between multiple API profiles behind a target-aware interface.

This project is inspired by [Claude Code Router](https://github.com/musistudio/claude-code-router), a powerful routing solution with many features. Unlike it, this project only focuses on being extremely lightweight, operating only by injecting environment variables into `~/.claude/settings.json`.

Current status:
- `claude` target is implemented
- `codex` target uses the new interface but is not implemented yet

## Features

- Switch profiles by injecting env vars into `~/.claude/settings.json`
- Export env vars to the current shell session via `eval`
- Track the active profile across sessions
- Support `$ENV_VAR` references in config values
- Validate profiles (env var resolution) without network calls
- Check API connectivity for all profiles

>[!warning]
>env vars(containing api-keys) are injected into `~/.claude/settings.json` directly, make sure this file not uploaded to git repos.

## Install

```sh
cargo install --path .
```

Homebrew - MacOS(Darwin)
```sh
brew tap yang-yiming/tap
brew install ccrl
```

## Config

Create `~/.config/ccr-lite/config.toml`:

Example:
```toml
default_target = "claude"
```

Create `~/.config/ccr-lite/claude.toml`:

```toml
[ds]
url = "https://api.deepseek.com/anthropic"
auth = "$DEEPSEEK_API_KEY"
color = "green"
description = "Deepseek API"
[ds.env]
ANTHROPIC_MODEL="deepseek-chat"
ANTHROPIC_SMALL_FAST_MODEL="deepseek-chat"

[kimi]
url = "https://api.moonshot.cn/anthropic/"
auth = "sk-xxxx"
color = "yellow"
description = "Kimi API"
```

**Optional fields:**
- `color` — Display color in list/interactive mode. Supported: `red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, `black`
- `description` — Short description shown in profile lists

## Usage

```sh
# Inject a Claude profile into ~/.claude/settings.json (persistent)
ccrl --target claude set ds

# Show the active target/profile
ccrl now

# List Claude profiles
ccrl --target claude list

# Validate all profiles (env var resolution, no network)
ccrl --target claude validate

# Check API connectivity for all profiles
ccrl --target claude check

# Export env vars to the current shell (temporary)
eval "$(ccrl --target claude ds)"
```
>[!note]
>Environment variable settings (e.g., `eval "$(ccrl ds)"`) have lower priority than `~/.claude/settings.json` in Claude Code. If a provider is already configured there, the environment >variable will be ignored.

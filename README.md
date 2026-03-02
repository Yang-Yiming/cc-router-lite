# ccrl — Claude Code Router Lite

A lightweight CLI for switching between multiple Claude Code API profiles.

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
# Deepseek
[ds]
url = "https://api.deepseek.com/anthropic"
auth = "$DEEPSEEK_API_KEY"
[ds.env]
ANTHROPIC_MODEL="deepseek-chat"
ANTHROPIC_SMALL_FAST_MODEL="deepseek-chat"

[kimi]
url = "https://api.moonshot.cn/anthropic/"
auth = "sk-xxxx"
```

## Usage

```sh
# Inject a profile into ~/.claude/settings.json (persistent)
ccrl set ds

# Show the active profile
ccrl now

# List all profiles
ccrl list

# Validate all profiles (env var resolution, no network)
ccrl validate

# Check API connectivity for all profiles
ccrl check

# Export env vars to the current shell (temporary)
eval "$(ccrl personal)"
# or
ccrl ds
```

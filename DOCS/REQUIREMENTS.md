# Requirements

## Overview

`ccrl` (Codex/Claude Code Router Lite) 是一个轻量级 Rust CLI 工具，用于在多个 profile 之间切换，并把对应的环境变量注入到目标编辑器/CLI 的配置中，或者直接导出到当前 shell。

这个版本把 profile 按 target 分开管理：
- `claude` target 用于 Claude Code
- `codex` target 用于 Codex

TUI 与 CLI 都围绕 `target + profile` 进行操作。

## Config Files

目录：`~/.config/ccr-lite/`

### 全局配置

`config.toml`

```toml
default_target = "claude"
```

- `default_target` 用于 `ccrl` 无 `--target` 时的默认 target
- 不考虑向后兼容，配置保持最小化

### Claude profiles

`claude.toml`

```toml
[fox]
url = "https://fox.example.com/v1"
auth = "sk-xxxx"

[fox.env]
ANTHROPIC_SMALL_FAST_MODEL = "Haiku"

[dog]
url = "https://dog.example.com/v1"
auth = "$DOG_AUTH"
color = "green"
description = "fallback profile"
```

### Codex profiles

`codex.toml`

```toml
[main]
url = "https://api.example.com/v1"
auth = "$CODEX_API_KEY"
wire_api = "responses"
requires_openai_auth = true

[main.env]
OPENAI_MODEL = "gpt-5.4"
```

## Profile Rules

- 每个顶层 table 是一个 profile
- `url` 和 `auth` 支持 `$` 前缀，表示从系统环境变量读取实际值
- `[name.env]` 下的 kv 对会作为额外环境变量注入
- 可选 `color` 字段：支持命名颜色（`red`, `green`, `yellow`, `blue`, `magenta`, `cyan`, `white`, `black`）或 hex 格式（`#RRGGBB` / `#RGB`）
- 可选 `description` 字段：用于列表和 TUI 显示
- `codex.toml` 可选 `wire_api` 字段，默认 `responses`
- `codex.toml` 可选 `requires_openai_auth` 字段，默认 `true`

## State File

路径：`~/.config/ccr-lite/.current`

状态文件记录当前 target 和 profile，例如：

```toml
target = "codex"
profile = "main"
mode = "oauth"
```

- `.current` 是运行状态，不是静态配置
- 目标是让 `now` 命令和 TUI 都能恢复当前所在 target
- `mode` 仅用于 synthetic profile，例如 Codex 的 `OAuth`

## Codex Managed Files

- `~/.codex/config.toml`：`ccrl` 负责设置顶层 `model_provider`，并同步 `[model_providers.<name>]`
- `~/.codex/auth.json`：API key profile 会写为 API key 模式；`OAuth` 会恢复保存的 OAuth auth
- `~/.config/ccr-lite/codex-oauth-auth.json`：由 `ccrl` 管理的 OAuth auth snapshot

## Commands

### Global Option: `--target <target>`

- 可选值：`claude`, `codex`
- 不写时使用 `config.toml` 中的 `default_target`
- 同一套命令在不同 target 下执行不同的配置注入/导出

### `ccrl set <name>`

将指定 profile 的配置注入到目标配置中，并记录当前激活状态：
- `target=claude` 时，注入 Claude 对应的 settings
- `target=codex` 时，注入 Codex 对应的 settings
- 额外环境变量来自 profile 的 `[name.env]`

### `ccrl now`

输出当前激活的 `target/profile` 组合，或在没有激活状态时提示为空。

### `ccrl <name>` (shell export)

输出当前 target 下该 profile 的 `export KEY="value"` 语句到 stdout，用户通过 `eval "$(ccrl <name>)"` 在当前 shell 中生效。不修改 settings，不记录状态。

### `ccrl list`

列出当前 target 下所有可用 profile，标记 active 状态。

### `ccrl` (no args, interactive)

进入 TUI：
- 当前版本先按选定 target 显示对应 profiles
- `codex` target 会额外显示一个 synthetic 选项 `OAuth`
- `Enter` 激活选中的 profile

后续增强：
- 顶部显示 `Claude | Codex` 两个 tab
- `Tab` 切换 target

## Constraints

- Rust 编写，单二进制文件
- 修改目标 settings 时必须保留已有字段
- settings 文件可能没有 `env` 字段，需要自动创建
- 支持 `--config` 全局选项指定非默认全局配置文件路径

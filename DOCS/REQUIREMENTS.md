# Requirements

## Overview

`ccrl` (Claude Code Router Lite) — 一个轻量级 Rust CLI 工具，用于在多个 Claude Code API profile 之间快速切换。通过修改 `~/.claude/settings.json` 中的环境变量或直接输出 shell export 语句来实现。

## Config File

路径: `~/.config/ccr-lite/config.toml`

```toml
[fox]
url = "https://fox.example.com/v1"          # ANTHROPIC_BASE_URL
auth = "sk-xxxx"                            # ANTHROPIC_AUTH_TOKEN

[fox.env]
ANTHROPIC_SMALL_FAST_MODEL = "Haiku"        # 额外注入的环境变量

[dog]
url = "https://dog.example.com/v1"
auth = "$DOG_AUTH"                           # $ 前缀 = 从环境变量读取
```

- 每个顶层 table 是一个 profile
- `auth`（和 `url`）支持 `$` 前缀，表示从系统环境变量读取实际值
- `[name.env]` 下的 kv 对会作为额外环境变量注入

## Commands

### `ccrl set <name>`
将指定 profile 的配置注入到 `~/.claude/settings.json` 的 `"env"` 对象中：
- `url` → `ANTHROPIC_BASE_URL`
- `auth` → `ANTHROPIC_AUTH_TOKEN`
- `[name.env]` 中的所有 kv 直接注入
- 记录当前激活的 profile

### `ccrl now`
输出当前激活的 profile 名称。

### `ccrl <name>` (shell export)
输出 `export KEY="value"` 语句到 stdout，用户通过 `eval "$(ccrl fox)"` 在当前 shell 中生效。不修改 settings.json，不记录状态。

### `ccrl list` (建议新增)
列出所有可用 profile，标记当前激活的。

## Constraints

- Rust 编写，单二进制文件
- 修改 settings.json 时必须保留所有已有字段（model、plugins 等）
- settings.json 可能没有 `"env"` 字段，需要自动创建
- 支持 `--config` 全局选项指定非默认配置文件路径

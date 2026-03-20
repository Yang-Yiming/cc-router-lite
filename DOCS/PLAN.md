# Implementation Plan

## Status: Architecture Design Phase

## Progress

- [x] 需求分析 (prompt.md)
- [x] 架构设计 (ARCHITECTURE.md)
- [x] 项目脚手架 (Cargo.toml, src/)
- [x] error.rs — 错误类型
- [x] config.rs — 配置解析
- [x] state.rs — .current 状态追踪
- [x] settings.rs — settings.json 注入
- [x] main.rs — CLI + 命令分发
- [x] 集成测试
- [x] cargo clippy + fmt

## Implementation Order

1. **Cargo.toml** — 项目初始化，声明依赖
2. **error.rs** — CcrlError enum，其他模块都依赖它
3. **config.rs** — RawProfile, EnvOrLiteral, Profile, load_config, resolve_profile
4. **state.rs** — write_current, read_current
5. **settings.rs** — inject_profile
6. **main.rs** — Cli/Commands 定义，cmd_set/cmd_now/cmd_list/cmd_export

## Completed Features

- [x] **Feature 1: `ccrl check`** — Connectivity check command
- [x] **Feature 2: Notes/Description field** — Optional `description` field in profiles, shown in `ccrl list` output.
- [x] **Feature 3: Shell completions** — `ccrl completions <shell>` generates completions via `clap_complete`. that tests each profile's API endpoint (`/v1/models`) and reports status with timing. Usage: `ccrl check` or `ccrl check <profile-name>`.
- [x] **Feature 5: Interactive Profile Selector** — `ccrl` with no args shows `dialoguer::Select` inline picker (arrow keys + Enter). TTY check falls back to help for non-interactive shells. Escape/Ctrl-C exits cleanly.

## Completed Fixes

- [x] **Stale env key cleanup on profile switch** — `inject_profile()` now removes old profile's env keys before injecting new ones, using `.current` state to identify the previous profile. Old profile missing or `.current` absent gracefully skips removal.

## Design Changes from Original Requirements

- 去掉 `settings_file` 配置项，settings.json 路径硬编码为 `~/.claude/settings.json`
- `url` 和 `auth` 统一支持 `$` 环境变量前缀
- 新增 `ccrl list` 命令
- shell export 值用单引号包裹

## Completed Features (continued)

- [x] **Feature 6: Per-profile color** — `color` field in config supports named colors and hex (`#RRGGBB` / `#RGB`). `parse_hex_color()` utility in `config.rs` parses hex strings without new deps; `apply_color()` in `main.rs` uses `owo-colors` `.truecolor(r,g,b)` for RGB output.

## Future Features

Each feature below is self-contained and can be implemented in a separate session.

---

### Feature 1: `ccrl check` — Connectivity Check

**New dep**: `ureq = "2"` in Cargo.toml (lightweight sync HTTP)

**`main.rs`**: add `Check { name: Option<String> }` to `Commands` enum; add `cmd_check()`

**Logic**: for each profile (or named one), resolve profile, then GET `{url}/v1/models` with header `x-api-key: {auth}`. Print result with timing.

**Output example**:
```
[✓] work-anthropic    200 OK (142ms)
[✗] personal-bedrock  connection refused
[!] openrouter        401 unauthorized
```

---

### Feature 2: Notes/Description Field

**`config.rs`**: add `pub description: Option<String>` to `RawProfile`

**`main.rs` `cmd_list()`**: if description present, print it after profile name

**Output example**:
```
* work-anthropic  (active)  — work AWS Bedrock
  openrouter                — cheap fallback
```

**Config example**:
```toml
[work-anthropic]
url = "https://..."
auth = "$WORK_KEY"
description = "work AWS Bedrock"
```

---

### Feature 3: Shell Completion (`ccrl completions`)

**New dep**: `clap_complete = "4"` in Cargo.toml

**`main.rs`**: add `Completions { shell: clap_complete::Shell }` to `Commands`; add `cmd_completions()` calling `clap_complete::generate()` to stdout

**Usage**: `eval "$(ccrl completions zsh)"` in `.zshrc`

Completes: subcommand names + profile names for `set`/`check`/`validate` args

---

### Feature 4: `ccrl validate` — Profile Validation ✅

**No new deps needed.**

**`main.rs`**: add `Validate` to `Commands`; add `cmd_validate()`

**Logic**: load config, for each profile call `resolve_profile()`. Print `[✓] name` or `[✗] name  <error>`.

**Output example**:
```
[✓] work-anthropic
[✗] personal-bedrock  env var BEDROCK_KEY not set
```

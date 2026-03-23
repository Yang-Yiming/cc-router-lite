# Architecture

## Project Structure

```
cc-router-lite/
├── Cargo.toml
├── CLAUDE.md
├── prompt.md
├── DOCS/
│   ├── REQUIREMENTS.md
│   ├── PLAN.md
│   └── ARCHITECTURE.md
└── src/
    ├── codex.rs      # ~/.codex/config.toml + auth.json management
    ├── main.rs       # CLI 定义 (clap) + 命令分发
    ├── config.rs     # 全局配置 + target profile 解析、~ 展开
    ├── settings.rs   # target settings 写入与 env 注入
    ├── state.rs      # .current 活跃 target/profile 追踪
    └── error.rs      # 统一错误类型 CcrlError
```

## Dependencies

| Crate | 用途 |
|-------|------|
| `clap` (4, derive) | CLI 参数解析 |
| `serde` (1, derive) | 序列化/反序列化 |
| `toml` (0.8) | config.toml 解析 |
| `serde_json` (1) | settings.json 操作 |
| `dirs` (5) | 跨平台 home 目录解析 |
| `thiserror` (2) | 错误类型派生 |

| `dialoguer` (0.12) | 交互式 profile 选择器 |

无 async 依赖 — 所有操作都是本地文件读写，同步即可。

## Data Structures

### `error.rs` — 统一错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum CcrlError {
    #[error("Config file not found: {0}")]
    ConfigNotFound(String),

    #[error("Profile '{0}' not found in config")]
    ProfileNotFound(String),

    #[error("Environment variable '{0}' not set")]
    EnvVarNotSet(String),

    #[error("Invalid config: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("Invalid settings.json: {0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("{0}")]
    Io(#[from] std::io::Error),
}
```

覆盖所有失败场景，`main()` 中统一捕获并输出到 stderr。

### `config.rs` — 配置与 Profile

```rust
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Claude,
    Codex,
}

#[derive(Debug, Deserialize)]
pub struct GlobalConfig {
    // parsed and validated against Target
    pub default_target: Target,
}

/// config.toml 中每个 profile 的原始结构
#[derive(Debug, Deserialize)]
pub struct RawProfile {
    pub url: String,
    pub auth: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub wire_api: Option<String>,
    pub requires_openai_auth: Option<bool>,
}

/// 值解析：字面量 vs 环境变量引用
pub enum EnvOrLiteral<'a> {
    Literal(&'a str),
    EnvVar(&'a str),   // $ 后的变量名
}

/// 解析后的 profile（所有值已 resolve）
pub struct Profile {
    pub name: String,
    pub url: String,
    pub auth: String,
    pub env: HashMap<String, String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub wire_api: String,
    pub requires_openai_auth: bool,
}
```

**解析策略**：
- `config.toml` 只包含全局设置，例如 `default_target`
- `claude.toml` 和 `codex.toml` 分别保存各自 target 的 profile tables
- `config.rs` 提供 `load_global_config()` 和 `load_profiles(target)` 两层加载逻辑
- `load_profiles(target)` 内部把 target 映射到固定文件名，例如 `claude.toml` / `codex.toml`

```rust
pub fn load_global_config(path: &Path) -> Result<GlobalConfig, CcrlError> {
    let content = fs::read_to_string(path)?;
    Ok(toml::from_str(&content)?)
}

pub fn load_profiles(path: &Path) -> Result<HashMap<String, RawProfile>, CcrlError> {
    let content = fs::read_to_string(path)?;
    Ok(toml::from_str(&content)?)
}
```

`EnvOrLiteral` 的解析逻辑：
- `$` 开头 → `EnvVar`，通过 `std::env::var()` 获取实际值
- 否则 → `Literal`，直接使用
- `url` 和 `auth` 都走这个逻辑，保持一致性
- `wire_api` 默认 `responses`
- `requires_openai_auth` 默认 `true`

## CLI Design (`main.rs`)

使用 clap derive + `external_subcommand` 同时支持 `ccrl set fox` 和 `ccrl fox`，并通过 `--target` 选择 Claude 或 Codex：

```rust
#[derive(Parser)]
#[command(name = "ccrl", about = "Claude Code Router Lite")]
struct Cli {
    #[arg(long, global = true)]
    target: Option<Target>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Inject profile into target settings
    Set { name: String },
    /// Show active profile
    Now,
    /// List all profiles
    List,
    /// Shell export mode (ccrl <name>)
    #[command(external_subcommand)]
    Export(Vec<String>),
}
```

分发逻辑：
- `None` (no args, TTY) → 交互式 target/profile 选择器
- `None` (no args, non-TTY) → 输出 help
- `Set { name }` → 注入当前 target 的 settings + 写 .current
- `Now` → 读 .current 并输出 target/profile
- `List` → 读当前 target 的 profiles，列出并标记 active
- `Export(args)` → 取 `args[0]` 作为 profile 名，在当前 target 下输出 export 语句
- `Claude` 走 `settings.rs`
- `Codex` 走 `codex.rs`，同步 `~/.codex/config.toml` 和 `~/.codex/auth.json`

## Command Flows

### `ccrl` (no args, interactive)

```
1. 检测 stdin/stdout 是否 TTY
2. 非 TTY → 输出 help
3. TTY → load_global_config(), read_current()
4. 当前版本根据 `--target`、`.current` 或 `default_target` 确定 target
5. 根据当前 target 加载对应 profiles
6. 构建 display items，首项是 "Switch To Claude/Codex"
7. prompt 显示 `Claude | Codex` tab 风格标签，突出当前 target
8. `dialoguer::Select` 交互选择
9. Escape/Ctrl-C → 静默退出
10. 选择切换项时切换 target 并重绘列表
11. 选择 profile 后 → 委托 `cmd_set()` 激活 profile
```

### `ccrl set <name>`

```
1. 读取 global config，确定当前 target
2. load_profiles(~/.config/ccr-lite/{target}.toml)
3. resolve_profile(name) — 查找 profile，resolve url/auth 的 $ 引用
4. `Claude`: 写 `~/.claude/settings.json`
5. `Codex`: 若当前 auth 是 OAuth，则先刷新 OAuth snapshot
6. `Codex`: 更新 `~/.codex/config.toml` 的 `model_provider` 和 `[model_providers.<name>]`
7. `Codex`: 写 `~/.codex/auth.json` 为 API key 模式
8. 写 target + profile 到 ~/.config/ccr-lite/.current
9. println!("Profile '{name}' activated")
```

### `ccrl now`

```
1. 读取 ~/.config/ccr-lite/.current
2. 文件存在且非空 → 输出 target/profile；synthetic profile 可带 mode
3. 否则 → "No active profile"
```

### `ccrl <name>` (shell export)

```
1. 读取当前 target
2. load_profiles + resolve_profile
3. 输出到 stdout:
   export ANTHROPIC_BASE_URL='<url>'
   export ANTHROPIC_AUTH_TOKEN='<auth>'
   export KEY='value'  (for each in profile.env)
4. 不写 .current（临时操作）
```

用户使用方式: `eval "$(ccrl fox)"`
值用单引号包裹，避免 shell 特殊字符问题。

### `ccrl list`

```
1. 读取当前 target
2. load_profiles(target)
3. read_current() 获取当前 active target/profile
4. 遍历 profiles，输出:
   * fox  (active)
     dog
```

## Module Details

### `state.rs` — 活跃 Profile 追踪

状态文件: `~/.config/ccr-lite/.current`，TOML 格式，记录当前 target、profile 和可选 mode。

```rust
pub struct CurrentState {
    pub target: Target,
    pub profile: String,
    pub mode: Option<CurrentMode>,
}

pub fn write_current(state: &CurrentState) -> Result<(), CcrlError>;
pub fn read_current() -> Option<CurrentState>;
```

- `write_current`: 确保父目录存在，写入 target/profile/mode
- `read_current`: 读取文件并反序列化；文件不存在或为空返回 None
- 选择文件而非环境变量：环境变量不跨 shell session 持久化

### `codex.rs` — Codex Provider/Auth 管理

职责：
1. 读写 `~/.codex/config.toml`
2. 设置或清除顶层 `model_provider`
3. 同步 `[model_providers.<name>]`
4. 读写 `~/.codex/auth.json`
5. 在检测到 OAuth auth 时刷新 `~/.config/ccr-lite/codex-oauth-auth.json`
6. 提供 synthetic `OAuth` profile 的恢复能力

### `settings.rs` — settings.json 注入

使用 `serde_json::Value` 操作 settings.json，保留所有已有字段不被破坏。

```rust
pub fn inject_profile(
    settings_path: &Path,
    profile: &Profile,
) -> Result<(), CcrlError>;
```

核心逻辑：
1. 读取文件 → `serde_json::Value`（文件不存在则创建 `{}`）
2. 确保 `root["env"]` 是 Object（不存在则插入空 `{}`）
3. 插入 `ANTHROPIC_BASE_URL`、`ANTHROPIC_AUTH_TOKEN`、以及 `profile.env` 的所有 kv
4. `to_string_pretty()` 写回（2 空格缩进）

关键点：不用 typed struct 反序列化 settings.json，因为其字段不固定且我们只关心 `env`。

## Color Utilities (`config.rs`)

`pub(crate) fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)>` — parses `#RRGGBB` or `#RGB` hex strings into `(r, g, b)` tuples. Used by both `validate_color()` in `config.rs` and `apply_color()` in `main.rs`. No new dependencies — uses stdlib `u8::from_str_radix`. The `apply_color()` function in `main.rs` routes hex colors to `owo-colors`'s `.truecolor(r, g, b)` for 24-bit ANSI output.

## Design Decisions

| 决策 | 理由 |
|------|------|
| `config.toml` 只保留全局配置 | 避免把 target 元信息和 profile 定义混在一起 |
| `claude.toml` / `codex.toml` 分文件存储 | target 语义清晰，TUI tabs 与配置文件一一对应 |
| `serde_json::Value` 操作 settings.json | 保留所有未知字段，只修改 `env` |
| `.current` TOML 状态文件 | 需要同时记录 target、profile 与 optional mode |
| `external_subcommand` | 干净处理 `ccrl <name>` 与 `ccrl set/now/list` 共存 |
| `thiserror` | 零成本错误类型派生，良好的用户错误信息 |
| shell export 用单引号 | 避免值中特殊字符被 shell 解释 |
| `url` 和 `auth` 统一支持 `$` 前缀 | 一致性，低成本 |
| `--target` 作为全局选项 | 保持 CLI 简洁，避免嵌套子命令膨胀 |

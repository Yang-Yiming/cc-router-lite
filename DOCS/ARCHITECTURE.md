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
    ├── main.rs       # CLI 定义 (clap) + 命令分发
    ├── config.rs     # config.toml 解析、数据结构、~ 展开
    ├── settings.rs   # settings.json 读写与 env 注入
    ├── state.rs      # .current 活跃 profile 追踪
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

/// config.toml 中每个 profile 的原始结构
#[derive(Debug, Deserialize)]
pub struct RawProfile {
    pub url: String,
    pub auth: String,
    #[serde(default)]
    pub env: HashMap<String, String>,
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
}
```

**解析策略**: 去掉 `settings_file` 配置项，settings.json 路径硬编码为 `~/.claude/settings.json`。这样 config.toml 只包含 profile tables，可以直接反序列化为 `HashMap<String, RawProfile>`：

```rust
pub fn load_config(path: &Path) -> Result<HashMap<String, RawProfile>, CcrlError> {
    let content = fs::read_to_string(path)?;
    let profiles: HashMap<String, RawProfile> = toml::from_str(&content)?;
    Ok(profiles)
}
```

`EnvOrLiteral` 的解析逻辑：
- `$` 开头 → `EnvVar`，通过 `std::env::var()` 获取实际值
- 否则 → `Literal`，直接使用
- `url` 和 `auth` 都走这个逻辑，保持一致性

## CLI Design (`main.rs`)

使用 clap derive + `external_subcommand` 同时支持 `ccrl set fox` 和 `ccrl fox`：

```rust
#[derive(Parser)]
#[command(name = "ccrl", about = "Claude Code Router Lite")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Inject profile into settings.json
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
- `None` (no args, TTY) → 交互式 profile 选择器 (`dialoguer::Select`)
- `None` (no args, non-TTY) → 输出 help
- `Set { name }` → 注入 settings.json + 写 .current
- `Now` → 读 .current 并输出
- `List` → 读 config，列出所有 profile，标记 active
- `Export(args)` → 取 `args[0]` 作为 profile 名，输出 export 语句

## Command Flows

### `ccrl` (no args, interactive)

```
1. 检测 stdin/stdout 是否 TTY
2. 非 TTY → 输出 help
3. TTY → load_config, read_current()
4. 构建 display items: "name (active) — description"
5. dialoguer::Select 交互选择
6. Escape/Ctrl-C → 静默退出
7. 选择后 → 委托 cmd_set() 激活 profile
```

### `ccrl set <name>`

```
1. load_config(~/.config/ccr-lite/config.toml)
2. resolve_profile(name) — 查找 profile，resolve url/auth 的 $ 引用
3. 读取 ~/.claude/settings.json → serde_json::Value
4. 确保 root["env"] 存在（不存在则创建空 object）
5. 注入 ANTHROPIC_BASE_URL, ANTHROPIC_AUTH_TOKEN, [name.env] 的所有 kv
6. serde_json::to_string_pretty() 写回
7. 写 profile 名到 ~/.config/ccr-lite/.current
8. println!("Profile '{name}' activated")
```

### `ccrl now`

```
1. 读取 ~/.config/ccr-lite/.current
2. 文件存在且非空 → 输出 profile 名
3. 否则 → "No active profile"
```

### `ccrl <name>` (shell export)

```
1. load_config + resolve_profile
2. 输出到 stdout:
   export ANTHROPIC_BASE_URL='<url>'
   export ANTHROPIC_AUTH_TOKEN='<auth>'
   export KEY='value'  (for each in profile.env)
3. 不写 .current（临时操作）
```

用户使用方式: `eval "$(ccrl fox)"`
值用单引号包裹，避免 shell 特殊字符问题。

### `ccrl list`

```
1. load_config
2. read_current() 获取当前 active profile
3. 遍历 profiles，输出:
   * fox  (active)
     dog
```

## Module Details

### `state.rs` — 活跃 Profile 追踪

状态文件: `~/.config/ccr-lite/.current`，纯文本，内容仅为 profile 名称。

```rust
pub fn write_current(name: &str) -> Result<(), CcrlError>;
pub fn read_current() -> Option<String>;
```

- `write_current`: 确保父目录存在，写入 profile 名
- `read_current`: 读取文件，trim 后返回；文件不存在或为空返回 None
- 选择文件而非环境变量：环境变量不跨 shell session 持久化

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
| 去掉 `settings_file` 配置项 | 简化 config 解析，路径硬编码 `~/.claude/settings.json` |
| `HashMap<String, RawProfile>` 直接反序列化 | config.toml 纯 profile tables，无需手动解析 |
| `serde_json::Value` 操作 settings.json | 保留所有未知字段，只修改 `env` |
| `.current` 纯文本文件 | 最简单的持久化状态方案 |
| `external_subcommand` | 干净处理 `ccrl <name>` 与 `ccrl set/now/list` 共存 |
| `thiserror` | 零成本错误类型派生，良好的用户错误信息 |
| shell export 用单引号 | 避免值中特殊字符被 shell 解释 |
| `url` 和 `auth` 统一支持 `$` 前缀 | 一致性，低成本 |

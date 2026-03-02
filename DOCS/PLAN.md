# Implementation Plan

## Status: Architecture Design Phase

## Progress

- [x] 需求分析 (prompt.md)
- [x] 架构设计 (ARCHITECTURE.md)
- [ ] 项目脚手架 (Cargo.toml, src/)
- [ ] error.rs — 错误类型
- [ ] config.rs — 配置解析
- [ ] state.rs — .current 状态追踪
- [ ] settings.rs — settings.json 注入
- [ ] main.rs — CLI + 命令分发
- [ ] 集成测试
- [ ] cargo clippy + fmt

## Implementation Order

1. **Cargo.toml** — 项目初始化，声明依赖
2. **error.rs** — CcrlError enum，其他模块都依赖它
3. **config.rs** — RawProfile, EnvOrLiteral, Profile, load_config, resolve_profile
4. **state.rs** — write_current, read_current
5. **settings.rs** — inject_profile
6. **main.rs** — Cli/Commands 定义，cmd_set/cmd_now/cmd_list/cmd_export

## Completed Fixes

- [x] **Stale env key cleanup on profile switch** — `inject_profile()` now removes old profile's env keys before injecting new ones, using `.current` state to identify the previous profile. Old profile missing or `.current` absent gracefully skips removal.

## Design Changes from Original Requirements

- 去掉 `settings_file` 配置项，settings.json 路径硬编码为 `~/.claude/settings.json`
- `url` 和 `auth` 统一支持 `$` 环境变量前缀
- 新增 `ccrl list` 命令
- shell export 值用单引号包裹

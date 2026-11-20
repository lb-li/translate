# Lumen Settings and Motion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在现有 GPUI 浮窗内实现可持久化的自定义热键、开机自启动和可开关的界面动效，并发布 v0.3.4。

**Architecture:** `core-storage` 保存平台无关的 JSON 设置；`app-ui` 将 GPUI 按键转换为全局热键，通过线程安全控制器动态重注册，并用 HKCU Run 启动项同步自启动。设置以浮层覆盖翻译区，动画只更新必要的局部状态。

**Tech Stack:** Rust 2021、GPUI 0.3.12 / gpui-kit 0.9.1、global-hotkey、windows 0.62、serde_json。

## Global Constraints

- 默认热键必须是 `Alt + Q`，新热键至少包含 Ctrl、Alt 或 Win 之一。
- 设置路径为 `%LOCALAPPDATA%\Lumen\settings.json`。
- 自启动只写 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\LumenTranslate`，不要求管理员权限。
- 热键冲突必须回滚；设置失败必须内联提示。
- 保留现有翻译、缓存、收藏、原文/译文朗读和中英双向翻译。
- 非 Windows 构建保留桩实现，核心测试不访问真实全局热键或注册表。

---

### Task 1: 持久化设置模型

**Files:**
- Create: `crates/core-storage/src/settings.rs`
- Modify: `crates/core-storage/src/lib.rs`
- Modify: `crates/core-storage/Cargo.toml`
- Modify: `Cargo.toml`

**Interfaces:**
- Produces: `AppSettings { hotkey, launch_at_startup, animations_enabled }`、`HotkeySettings`、`AppSettings::load()`、`save()`、`default_path()`、`HotkeySettings::display()`。

- [ ] 增加 `HotkeySettings::default()` 返回 Alt+Q 的测试。
- [ ] 增加 `Ctrl+Shift+Q` 显示与 JSON 往返测试。
- [ ] 增加缺少新增字段时使用默认值的反序列化测试。
- [ ] 使用 serde 默认值、原子临时文件替换和 `%LOCALAPPDATA%` 路径实现。
- [ ] 运行 `cargo test -p core-storage`，确认全部通过。

### Task 2: 动态热键控制器

**Files:**
- Create: `crates/app-ui/src/hotkey.rs`
- Modify: `crates/app-ui/src/tray.rs`
- Modify: `crates/app-ui/src/main.rs`

**Interfaces:**
- Consumes: Task 1 的 `HotkeySettings`。
- Produces: `from_keystroke(&Keystroke) -> Option<HotkeySettings>`、`to_hotkey(&HotkeySettings) -> Result<HotKey>`、`HotkeyController::new()`、`update()`、`current_id()`。
- UI commands: `ApplyHotkey(HotkeySettings)`、`SetAutostart(bool)`、`SetAnimationsEnabled(bool)`。

- [ ] 实现字母、数字、F1–F12、Space、Tab、Enter、方向键和编辑键映射。
- [ ] 拒绝纯修饰键、Esc 和无 Ctrl/Alt/Win 的组合。
- [ ] 在互斥锁内注销旧热键并注册新热键，失败时重新注册旧热键。
- [ ] 托盘事件线程通过 `current_id()` 判断当前热键，不缓存初始 ID。
- [ ] 非 Windows 控制器只更新内存状态，保证应用类型检查可运行。

### Task 3: 当前用户开机自启动

**Files:**
- Create: `crates/app-ui/src/startup.rs`
- Modify: `crates/app-ui/Cargo.toml`
- Modify: `crates/app-ui/src/main.rs`

**Interfaces:**
- Produces: `set_launch_at_startup(enabled: bool) -> Result<()>`。
- Windows Run value: `LumenTranslate` = 带引号的 `current_exe()` 路径。

- [ ] 添加 `Win32_System_Registry` feature。
- [ ] 开启时写 `REG_SZ`，关闭时删除值，关闭已打开的键。
- [ ] 启动时按保存设置同步一次；失败仅记录或显示设置错误。
- [ ] 非 Windows 返回明确的不支持错误，不阻塞核心构建。

### Task 4: 设置浮层与动效

**Files:**
- Modify: `crates/app-ui/src/popover.rs`
- Modify: `crates/app-ui/src/icons.rs`
- Modify: `crates/app-ui/src/main.rs`
- Modify: `DESIGN.md`
- Modify: `README.md`

**Interfaces:**
- View state: `settings: AppSettings`、`show_settings`、`recording_hotkey`、`settings_error`、`streaming_frame`。
- View methods: `set_settings()`、`show_settings_error()`、`schedule_streaming_animation()`。
- Settings overlay: 热键录制、开机启动、界面动效和错误提示。

- [ ] 顶栏增加带显式 `text_color` 的齿轮图标和设置浮层。
- [ ] 录制状态优先处理按键；Esc 取消录制，普通设置页 Esc 返回翻译。
- [ ] 自启动和动效开关通过 UI command 应用，成功后才更新持久化状态。
- [ ] 打开动画改为约 120ms easeOutBack；关闭动效时直接最终状态。
- [ ] 译文为空且流式加载时显示三条错峰骨架，首个 Delta 或完成/错误后停止定时器。
- [ ] 更新设计规范、快捷键和设置文件说明。

### Task 5: 验证与发布

**Files:**
- Modify: `Cargo.toml`、`Cargo.lock`
- Modify: `.github/workflows/build-windows.yml`（仅在验证需要时）
- Modify: `README.md`

**Interfaces:**
- Release tag: `v0.3.4`。

- [ ] 运行 `cargo fmt --all -- --check`。
- [ ] 运行核心与新增设置测试，全部通过。
- [ ] 运行应用 `cargo check --locked` 和严格 Clippy。
- [ ] 检查 `git diff --check`，确认没有 Token 或临时凭据。
- [ ] 提交全部已批准改动并推送 `main` 与 `v0.3.4`，确认 Windows CI 成功及 Release 含 `lumen-translate.exe`。

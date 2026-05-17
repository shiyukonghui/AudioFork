# 最小化到系统托盘 Spec

## Why
当前用户点击窗口最小化按钮时，窗口会最小化到任务栏。对于音频路由器这类后台持续运行的应用，用户期望点击最小化时窗口隐藏到系统托盘，仅显示托盘图标，以便在后台持续运行音频路由而不占用任务栏空间。

## What Changes
- 添加 `tray-icon` crate 依赖（跨平台系统托盘支持）
- 在 `src/gui/mod.rs` 中添加系统托盘初始化逻辑
- 添加 `src/gui/tray.rs` 模块处理托盘图标和菜单
- 拦截窗口关闭事件，改为隐藏窗口而非退出程序
- 托盘图标右键菜单提供"显示窗口"和"退出"选项
- 双击托盘图标恢复显示窗口

## Impact
- Affected specs: phase4-gui（GUI 模块结构）
- Affected code: `Cargo.toml`（新增依赖）、`src/gui/mod.rs`（托盘初始化）、新增 `src/gui/tray.rs`

## ADDED Requirements

### Requirement: 系统托盘图标
系统 SHALL 在 GUI 启动时创建系统托盘图标，显示应用程序图标和状态。

#### Scenario: GUI 启动时���建托盘图标
- **WHEN** 用户启动 GUI 模式
- **THEN** 系统托盘区域显示应用程序图标，鼠标悬停显示"音频路由器"提示

#### Scenario: 托盘图标状态指示
- **WHEN** 音频引擎处于运行状态
- **THEN** 托盘图标显示绿色指示（或动态变化）
- **WHEN** 音频引擎处于停止状态
- **THEN** 托盘图标显示灰色指示

### Requirement: 最小化隐藏到托盘
当用户点击窗口最小化按钮或关闭按钮时，系统 SHALL 隐藏主窗口而非退出程序，仅保留托盘图标。

#### Scenario: 点击最小化按钮
- **WHEN** 用户点击窗口标题栏的最小化按钮
- **THEN** 主窗口完全隐藏（不显示在任务栏），托盘图标保持可见

#### Scenario: 点击关闭按钮
- **WHEN** 用户点击窗口标题栏的关闭按钮（X）
- **THEN** 主窗口隐藏到托盘，程序继续运行，托盘图标保持可见

#### Scenario: 窗口隐藏后引擎继续运行
- **WHEN** 窗口已隐藏到托盘且音频引擎正在运行
- **THEN** 音频路由功能继续正常工作，不受窗口隐藏影响

### Requirement: 托盘图标交互
系统 SHALL 支持通过托盘图标恢复显示窗口和退出程序。

#### Scenario: 双击托盘图标恢复窗口
- **WHEN** 用户双击系统托盘图标
- **THEN** 主窗口恢复显示并聚焦

#### Scenario: 单击托盘图标显示菜单
- **WHEN** 用户右键单击系统托盘图标
- **THEN** 显示上下文菜单，包含"显示窗口"、"退出"选项

#### Scenario: 菜单选择显示窗口
- **WHEN** 用户在托盘菜单中选择"显示窗口"
- **THEN** 主窗口恢复显示并聚焦

#### Scenario: 菜单选择退出
- **WHEN** 用户在托盘菜单中选择"退出"
- **THEN** 程序正常退出（若引擎运行中则先停止引擎）

### Requirement: 托盘菜单状态同步
托盘菜单 SHALL 根据当前窗口状态动态调整选项。

#### Scenario: 窗口已隐藏时菜单显示"显示窗口"
- **WHEN** 主窗口当前处于隐藏状态
- **THEN** 托盘菜单显示"显示窗口"选项（而非"隐藏窗口"）

#### Scenario: 窗口可见时菜单显示"隐藏窗口"
- **WHEN** 主窗口当前处于可见状态
- **THEN** 托盘菜单显示"隐藏窗口"选项

### Requirement: 跨平台托盘实现
系统 SHALL 使用 `tray-icon` crate 实现跨平台系统托盘功能，Windows 平台使用 `windows` crate 控制窗口可见性。

#### Scenario: Windows 平台窗口隐藏
- **WHEN** 在 Windows 平台上隐藏窗口
- **THEN** 使用 `ShowWindow(hwnd, SW_HIDE)` API 隐藏窗口

#### Scenario: Windows 平台窗口恢复
- **WHEN** 在 Windows 平台上恢复显示窗口
- **THEN** 使用 `ShowWindow(hwnd, SW_SHOW)` 或 `SW_SHOWDEFAULT` API 显示窗口

## MODIFIED Requirements

### Requirement: GUI 模块结构（Phase 4 扩展）
Phase 4 规定的 `src/gui/` 模块结构新增 `tray.rs` 子模块：
- `mod.rs` — GUI 入口，`eframe::App` trait 实现，托盘初始化
- `toolbar.rs` — 顶部工具栏
- `devices.rs` — 左侧设备管理面板
- `params.rs` — 右侧参数配置面板
- `status.rs` — 底部状态栏
- `logs.rs` — 日志/事件区域
- `theme.rs` — 浅色/深色主题
- `tray.rs` — 系统托盘图标和菜单管理（新增）

### Requirement: Cargo.toml 依赖（新增）
`Cargo.toml` 新增以下依赖：
- `tray-icon` — 跨平台系统托盘图标库

## REMOVED Requirements
无。
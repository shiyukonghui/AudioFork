# Phase 4: GUI 与打磨 Spec

## Why
Phase 1-3 已实现完整的 CLI 音频路由引擎（设备枚举、扇出管道、重采样、漂移补偿、热插拔、限幅器）。Phase 4 需要基于 `egui` 构建图形界面，通过消息通道与核心引擎解耦交互，实现 panic 隔离，完成跨平台打包，让非技术用户也能使用本工具。

## What Changes
- 添加 `egui`、`eframe`、`rfd`、`sysinfo` 依赖
- 新建 `src/gui/` 模块目录（7 个文件）：入口、工具栏、设备面板、参数面板、状态栏、日志区、主题
- 新建 `src/message.rs` — 消息通道枚举（GuiToEngine / EngineToGui）
- 改建 `src/main.rs` — 替换 "GUI 模式将在第四阶段实现" 为实际 GUI 启动逻辑
- GUI 线程通过 `crossbeam-channel` 与引擎线程通信，消息**不进入**音频回调
- 基于 egui 即时模式渲染，30 FPS 刷新

## Impact
- Affected specs: 无（Phase 4 是所有前序阶段的 GUI 前端叠加）
- Affected code: `Cargo.toml`, `src/main.rs`, 新建 `src/message.rs`, `src/gui/*.rs`（7个文件）
- Phase 1-3 引擎代码无需修改

## ADDED Requirements

### Requirement: egui/eframe GUI 框架
系统 SHALL 使用 `egui` + `eframe` 构建跨平台原生窗口 GUI，以 30 FPS 刷新。

#### Scenario: GUI 启动
- **WHEN** 用户执行 `audio_router --gui`
- **THEN** 在 < 1 秒内显示主窗口（默认 900×700 像素），自动加载配置文件

#### Scenario: GUI 刷新率
- **WHEN** GUI 窗口处于活动状态
- **THEN** 每 33ms（30 FPS）重绘一次界面，引擎运行中亦保持此刷新率

### Requirement: GUI 模块结构
系统 SHALL 将 GUI 代码组织为独立模块目录 `src/gui/`，包含以下子模块：
- `mod.rs` — GUI 入口，`eframe::App` trait 实现
- `toolbar.rs` — 顶部工具栏
- `devices.rs` — 左侧设备管理面板
- `params.rs` — 右侧参数配置面板
- `status.rs` — 底部状态栏
- `logs.rs` — 日志/事件区域
- `theme.rs` — 浅色/深色主题

#### Scenario: 模块独立性
- **WHEN** 遍历 `src/gui/` 目录
- **THEN** 每个子模块职责单一，通过 `mod.rs` 统一聚合

### Requirement: 消息通道
系统 SHALL 通过 `crossbeam-channel` 在 GUI 线程与引擎线程之间传递控制指令和状态更新，消息**绝不**进入音频回调。

#### Scenario: GUI 发送控制指令
- **WHEN** 用户点击启动按钮
- **THEN** GUI 线程通过 `GuiToEngine::Start(config)` 发送消息到引擎线程

#### Scenario: 引擎推送状态
- **WHEN** 引擎检测到输出设备热插拔变化
- **THEN** 引擎线程通过 `EngineToGui::DeviceListUpdated(...)` 推送消息，GUI 刷新设备列表

### Requirement: 主窗口布局
系统 SHALL 实现需求说明书 2.9.2 规定的五区布局。

#### Scenario: 布局呈现
- **WHEN** GUI 窗口初始化
- **THEN** 从上到下、从左到右依次显示：顶部工具栏、左侧设备面板、右侧参数面板、底部状态栏、底部可折叠日志区

### Requirement: 顶部工具栏
系统 SHALL 在顶部工具栏提供以下控件：
- 引擎状态指示灯（绿色=运行 / 灰色=停止 / 红色=错误）
- 启动/停止按钮
- 配置导入按钮（调用 `rfd::FileDialog`）
- 配置导出按钮
- 帮助链接

#### Scenario: 启动后按钮锁定
- **WHEN** 引擎处于运行状态
- **THEN** "启动"按钮变灰，"停止"按钮可用；参数配置区和设备选择下拉框锁定

### Requirement: 设备管理面板（左侧）
系统 SHALL 在左侧面板显示：
- 输入设备下拉选择框（显示采样率、声道数、格式）
- 输出设备列表（复选框 + 名称 + 采样率 + 声道 + 状态标签 + 设备类型图标）
- "刷新设备"按钮

#### Scenario: 蓝牙设备标注
- **WHEN** 输出设备列表中包含蓝牙设备
- **THEN** 该设备行显示 ⚠ 图标并标注"高延迟"

#### Scenario: 设备状态刷新
- **WHEN** 用户点击"刷新设备"按钮或引擎推送 `DeviceListUpdated`
- **THEN** 重新枚举设备并更新列表，不中断当前播放

### Requirement: 参数配置面板（右侧）
系统 SHALL 在右侧面板提供：
- 缓冲区帧数滑块（32～4096，显示对应延迟 ms）
- 最大延迟限制输入框（ms）
- 重采样算法下拉菜单（Sinc/Cubic/None）
- 漂移补偿复选框
- 独占模式复选框（仅 Windows 平台可见）
- 输入丢失行为单选按钮组

#### Scenario: 滑块实时反馈
- **WHEN** 用户拖动缓冲区帧数滑块
- **THEN** 旁边的延迟 ms 标签实时更新（`buffer_frames * 1000 / sample_rate`）

### Requirement: 底部状态栏
系统 SHALL 在底部状态栏显示每个活跃输出设备的实时指标：
- 设备名称
- 欠载计数、溢出计数
- 当前延迟估计（ms）
- 漂移调整量（delta）
- 缓冲区水位百分比

#### Scenario: 状态栏定时刷新
- **WHEN** 引擎每 500ms 通过 `EngineToGui::OutputStatus` 推送
- **THEN** GUI 更新各输出设备的指标显示

### Requirement: 日志/事件区域
系统 SHALL 在底部提供可折叠的日志区域，显示最近 100 条运行日志（带时间戳），支持复制和清空。

#### Scenario: 日志滚动
- **WHEN** 新日志条目通过 `EngineToGui::Log` 推送
- **THEN** 自动追加到日志区域末尾，超出 100 条时移除最旧条目

### Requirement: 可访问性与主题
系统 SHALL 支持：
- Tab 键焦点导航（egui 默认支持）
- 自动检测系统主题（深色/浅色），启动时匹配
- 运行时手动切换主题

#### Scenario: 系统主题跟随
- **WHEN** Windows 系统主题为深色模式
- **THEN** GUI 启动时自动使用 `egui::Visuals::dark()`

### Requirement: Panic 隔离
系统 SHALL 使用 `std::panic::catch_unwind` 包裹 GUI 更新逻辑，并使用 `std::panic::set_hook` 设置自定义 panic hook。

#### Scenario: GUI panic 不中断引擎
- **WHEN** GUI 渲染逻辑发生 panic（如绘制错误）
- **THEN** catch_unwind 捕获 panic，弹出 `rfd::MessageDialog` 通知用户，引擎继续运行；最坏情况下进程退出但音频已淡出静音

### Requirement: 配置文件互通
系统 SHALL 支持在 GUI 中：
- 启动时自动加载 `audio_router.toml`
- "保存配置"按钮将当前界面参数写入文件
- File 菜单导入/导出配置（`--preset` 语义）

#### Scenario: 配置导出
- **WHEN** 用户点击"导出配置"
- **THEN** `rfd::FileDialog` 弹出保存对话框，将当前参数序列化为 TOML 并写入用户选择的路径

### Requirement: 启动停止交互逻辑
系统 SHALL 遵循以下交互约束：
- **启动前**：参数和设备选择可修改
- **启动后**：参数锁定，仅允许停止或动态设备启停
- **停止**：引擎平滑停止 → 通知 GUI 恢复编辑状态
- **窗口关闭**：若引擎运行中，先发送停止指令再退出

#### Scenario: 运行时设备启停
- **WHEN** 引擎运行中，用户取消勾选某输出设备
- **THEN** GUI 发送 `GuiToEngine::DisableDevice(name)` → 引擎停用对应槽位 → 推送状态更新

### Requirement: 跨平台依赖
系统 SHALL 添加以下依赖到 `Cargo.toml`：
- `egui` + `eframe`（GUI 框架，默认使用 glow 后端）
- `rfd`（原生文件对话框）
- `sysinfo`（CPU/内存监控，可选功能）

#### Scenario: 依赖已就绪
- **WHEN** `cargo check` 执行
- **THEN** 所有新依赖正确下载，编译通过

## MODIFIED Requirements

### Requirement: main.rs GUI 模式处理（Phase 4 版本）
替换 Phase 3 中的 "GUI 模式将在第四阶段实现" 占位逻辑为：
- `--gui` 生效时：初始化 egui/eframe 窗口 → 加载配置 → 创建消息通道 → 启动 GUI 事件循环
- `--gui` 与 `--monitor` 互斥（cli.rs 已有 `conflicts_with` 约束，保持不变）
- 非 GUI 模式（CLI 模式）：保持 Phase 3 逻辑不变

#### Scenario: GUI 模式启动流程
- **WHEN** `audio_router --gui` 执行
- **THEN** 不再打印"GUI 模式将在第四阶段实现"并退出，而是启动 egui 窗口并进入 GUI 事件循环

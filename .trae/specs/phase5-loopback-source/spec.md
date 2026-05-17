# Phase 5: 音源类型拓展（Loopback 系统音频回采）Spec

## Why
AudioFork 当前只支持「物理输入设备（麦克风/Line-in）→ 多个输出设备」的扇出路由。用户需要将电脑正在播放的系统声音（如音乐、视频、游戏音频）同步分发到多个输出设备（如有线音箱 + 蓝牙耳机同时出声）。本阶段引入 `SourceType` 音源抽象，使音源可在「物理输入设备」和「系统音频回采（Loopback）」之间切换，下游 SPSC 扇出管道完全复用。

## What Changes
- 新建 `src/source.rs` — 音源类型枚举与平台检测逻辑
- 改建 `src/config.rs` — `AudioRouterConfig` 新增 `source_type`、`loopback_device` 字段
- 改建 `src/cli.rs` — 新增 `--source-type`、`--loopback-device` CLI 参数
- 改建 `src/message.rs` — `EngineConfig` 新增 `source_type`、`loopback_device` 字段
- 改建 `src/audio/capture.rs` — `CaptureStream` 新增 `from_loopback()` 构造方法
- 改建 `src/device.rs` — 新增 `enumerate_loopback_devices()` 和 `select_loopback_device()` 函数
- 改建 `src/main.rs` — `run()` 根据 `source_type` 分支选择音源，其余管道逻辑不变
- 改建 `src/gui/devices.rs` — 设备面板新增音源类型选择（Radio 切换）+ 回采设备下拉框
- 改建 `src/gui/mod.rs` — `AudioRouterApp` 创建和配置导出时包含新字段
- 改建 `src/gui/params.rs` — `ParamsPanelState` 与配置加载/构建包含新字段

## Impact
- Affected specs: Phase 1-4（引擎核心无变动，仅新增音源入口分支）
- Affected code: `src/source.rs`（新建），`src/config.rs`，`src/cli.rs`，`src/message.rs`，`src/audio/capture.rs`，`src/device.rs`，`src/main.rs`，`src/gui/devices.rs`，`src/gui/mod.rs`，`src/gui/params.rs`
- 下游管道（pipeline.rs、channel_map.rs、resample.rs、limiter.rs、drift.rs）**完全不变**

## ADDED Requirements

### Requirement: 音源类型抽象
系统 SHALL 提供 `SourceType` 枚举，包含 `InputDevice`（物理输入设备）和 `Loopback`（系统音频回采）两种类型，并支持在运行时通过配置切换。

#### Scenario: 默认音源类型
- **WHEN** 用户未指定 `--source-type` 且配置文件中未设置
- **THEN** 系统默认使用 `InputDevice`（物理输入设备），行为与 Phase 4 完全一致

#### Scenario: 切换到 Loopback 音源
- **WHEN** 用户指定 `--source-type loopback` 或在 GUI 中选择「系统音频回采」
- **THEN** 系统尝试以 Loopback 模式采集系统音频输出，若平台不支持则返回明确错误提示

### Requirement: 平台 Loopback 支持
系统 SHALL 在 Windows 平台通过 WASAPI Loopback 模式实现系统音频回采；在非 Windows 平台返回明确的平台不支持错误，并引导用户使用虚拟声卡方案。

#### Scenario: Windows WASAPI Loopback
- **WHEN** 用户在 Windows 平台选择 Loopback 音源
- **THEN** 系统使用 WASAPI 以 Loopback 模式打开指定（或默认）输出设备的捕获流，将系统播放的音频数据送入扇出管道

#### Scenario: 非 Windows 平台 Loopback
- **WHEN** 用户在 macOS 或 Linux 平台选择 Loopback 音源
- **THEN** 系统返回 `NotSupported` 错误，日志/错误消息中引导用户安装虚拟声卡（macOS: BlackHole；Linux: PulseAudio monitor source / PipeWire）

### Requirement: CLI 新增参数
系统 SHALL 在 CLI 中新增以下参数：
- `--source-type <input|loopback>` — 音源类型，默认 `input`
- `--loopback-device <名称>` — Loopback 模式下回采的目标输出设备名称，不指定则使用系统默认输出设备

#### Scenario: CLI Loopback 启动
- **WHEN** 用户执行 `audio_router --source-type loopback --loopback-device "扬声器"`
- **THEN** 系统以 Loopback 模式采集「扬声器」的系统音频，扇出到配置的输出设备列表

### Requirement: 配置新增字段
系统 SHALL 在 `AudioRouterConfig` 中新增：
- `source_type: String` — `"input"` 或 `"loopback"`，默认 `"input"`
- `loopback_device: Option<String>` — Loopback 回采目标输出设备名

#### Scenario: 配置文件指定 Loopback
- **WHEN** `audio_router.toml` 包含 `source_type = "loopback"` 和 `loopback_device = "扬声器"`
- **THEN** 系统启动时自动使用 Loopback 模式，无需 CLI 参数

### Requirement: CaptureStream 新增 Loopback 构造方法
系统 SHALL 为 `CaptureStream` 新增 `from_loopback()` 静态方法，接受输出设备引用和流配置，返回 Loopback 捕获流。物理输入设备的构造逻辑改名重构为 `from_input_device()`，原有 `new()` 保留为别名。

#### Scenario: Loopback 流创建
- **WHEN** 引擎调用 `CaptureStream::from_loopback(device, config, callback)`
- **THEN** Windows 上通过 WASAPI loopback 打开输出设备的音频回采流；非 Windows 返回 `AudioRouterError::NotSupported`

#### Scenario: 物理输入流向后兼容
- **WHEN** 引擎调用 `CaptureStream::new(device, config, callback)` 或 `CaptureStream::from_input_device(device, config, callback)`
- **THEN** 行为与 Phase 4 完全一致，不受新增方法影响

### Requirement: 设备枚举新增 Loopback 设备查询
系统 SHALL 新增 `enumerate_loopback_devices()` 函数，返回支持 Loopback 捕获的输出设备列表（Windows 上为所有活跃输出设备；非 Windows 返回空列表或错误提示）。

#### Scenario: 枚举 Loopback 设备
- **WHEN** GUI 设备面板或 CLI 枚举调用 `enumerate_loopback_devices()`
- **THEN** Windows 上返回所有支持输出配置的设备列表；非 Windows 返回空列表并附带平台提示

### Requirement: GUI 音源类型选择
系统 SHALL 在设备管理面板（左侧）的「输入设备」区域上方新增音源类型 Radio 选择：
- 「物理输入设备（麦克风）」— 默认选中
- 「系统音频回采（Loopback）」— 非 Windows 平台显示 ⚠ 标记

选中不同音源类型时，下方的设备下拉框切换为对应的设备列表（输入设备 / 回采输出设备）。

#### Scenario: GUI 中切换音源
- **WHEN** 用户在设备面板中点击「系统音频回采」Radio 按钮
- **THEN** 输入设备下拉框隐藏，显示回采设备下拉框（列出可用于回采的输出设备）

#### Scenario: 非 Windows 平台 Loopback 提示
- **WHEN** 用户在 macOS/Linux 的 GUI 中选择「系统音频回采」
- **THEN** 回采设备下拉框旁显示 ⚠ 图标，hover 提示「当前平台不支持直接 Loopback，请安装虚拟声卡」

### Requirement: 引擎启动链分支化
系统 SHALL 在 `main.rs` 的 `run()` 函数中，根据 `ResolvedConfig.source_type` 分支选择音源采集方式：
- `"input"` → 调用 `device::select_input_device()` + `CaptureStream::from_input_device()`（现有逻辑）
- `"loopback"` → 调用 `device::select_loopback_device()` + `CaptureStream::from_loopback()`（新增逻辑）

下游扇出管道创建逻辑（步骤 7-15）完全不变，不受音源类型影响。

#### Scenario: Loopback 引擎运行
- **WHEN** 配置为 Loopback 模式且引擎启动成功
- **THEN** 启动信息中显示「音源: Loopback（回采设备: XXX）」，其余输出格式与物理输入模式一致

## MODIFIED Requirements

### Requirement: EngineConfig 结构体（Phase 5 版本）
在 Phase 4 版本基础上新增两个字段：
- `source_type: String` — `"input"` 或 `"loopback"`，默认 `"input"`
- `loopback_device: Option<String>` — Loopback 回采目标输出设备名

#### Scenario: EngineConfig 序列化
- **WHEN** GUI 构建 `EngineConfig` 并发送到引擎
- **THEN** 新字段随现有字段一同传递，引擎正确解析并使用

## REMOVED Requirements
无。

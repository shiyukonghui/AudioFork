# Phase 1: 音频基础设施 Spec

## Why
项目当前仅有一个 `cargo init` 骨架，需要搭建 Rust 工程基础、实现设备枚举与单路直通播放，验证 `cpal` 在 Windows/macOS 目标平台上的可用性。

## What Changes
- 配置 `Cargo.toml` 依赖：`cpal`、`clap`、`toml`、`serde`、`log`、`tracing`、`tracing-subscriber`、`tracing-appender`
- 添加 `wasapi-exclusive` Cargo feature（条件编译引入 `wasapi-rs`）
- 建立 `src/` 模块划分：`main.rs`、`cli.rs`、`config.rs`、`device.rs`、`audio/mod.rs`、`audio/capture.rs`、`audio/playback.rs`、`error.rs`
- 实现设备枚举（输入/输出），标记设备类型（有线/蓝牙/网络/未知）
- 实现完整 CLI 参数骨架（覆盖需求说明书 2.1 所有参数）
- 实现同参数单路直通播放（输入采样率 == 输出采样率 && 声道数相同）
- 建立统一错误类型 `AudioRouterError`、错误严重级别 `ErrorSeverity`、恢复状态枚举 `RecoveryState`
- 实现 TOML 配置文件加载，命令行参数覆盖配置

## Impact
- Affected specs: 无（全新项目）
- Affected code: `Cargo.toml`、`src/` 下全部模块

## ADDED Requirements

### Requirement: 工程依赖与模块结构
系统 SHALL 在 `Cargo.toml` 中声明 Phase 1 所需依赖，并按规划建立 `src/` 模块目录结构。

#### Scenario: 依赖声明正确
- **WHEN** 执行 `cargo check`
- **THEN** 所有依赖解析成功，无编译错误

#### Scenario: wasapi-exclusive feature 条件编译
- **WHEN** 非 Windows 平台编译
- **THEN** 该 feature 不可用，相关代码路径不可见

### Requirement: 设备枚举与选择
系统 SHALL 枚举所有输入/输出音频设备，返回设备名称、默认采样率范围、声道数范围、支持的格式列表，并标记设备类型。

#### Scenario: 枚举所有输出设备
- **WHEN** 调用 enumerate_output_devices()
- **THEN** 返回所有有效输出设备列表，蓝牙/网络设备被标注类型

#### Scenario: 按名称模糊匹配输入设备
- **WHEN** 指定 `--input-device "Mic"`
- **THEN** 选中名称包含 "Mic" 的输入设备

#### Scenario: 默认输入设备
- **WHEN** 未指定 `--input-device`
- **THEN** 使用系统默认输入设备

#### Scenario: 默认输出设备
- **WHEN** 未指定 `--output-device`
- **THEN** 使用全部有效输出设备

### Requirement: CLI 参数骨架
系统 SHALL 支持需求说明书 2.1 中全部命令行参数，包括互斥校验和配置文件覆盖。

#### Scenario: --gui 与 --monitor 互斥
- **WHEN** 同时指定 `--gui` 和 `--monitor`
- **THEN** 拒绝启动并提示错误

#### Scenario: 配置文件加载
- **WHEN** 指定 `--config <PATH>`
- **THEN** 加载 TOML 配置文件，命令行参数覆盖配置文件值

#### Scenario: GUI 模式下参数过滤
- **WHEN** `--gui` 生效
- **THEN** 仅解析 `--config` 和 `--gui`，其余参数忽略并输出 warn 日志

### Requirement: 同参数单路直通
系统 SHALL 在输入/输出采样率和声道数相同时，实现一路音频直通播放。

#### Scenario: 同参数直通成功
- **WHEN** 输入设备 48kHz/立体声，输出设备 48kHz/立体声
- **THEN** 音频从输入直接拷贝至输出，延迟 < 20ms（共享模式）

#### Scenario: 不同参数拒绝启动
- **WHEN** 输入 48kHz，输出 44.1kHz
- **THEN** 拒绝启动并提示"需要重采样能力，将在第三阶段支持"

#### Scenario: Ctrl+C 优雅停止
- **WHEN** 用户按 Ctrl+C
- **THEN** 2 秒内干净退出，无残留线程

### Requirement: 统一错误类型
系统 SHALL 定义 `AudioRouterError` 枚举、`ErrorSeverity` 枚举和 `RecoveryState` 枚举，所有模块使用统一 `Result<T, AudioRouterError>`。

#### Scenario: 设备未找到错误
- **WHEN** 指定不存在的设备名
- **THEN** 返回 `AudioRouterError::DeviceNotFound` 并包含清晰错误消息

#### Scenario: 配置错误
- **WHEN** 配置文件格式错误
- **THEN** 返回 `AudioRouterError::ConfigError` 并包含具体原因

### Requirement: 配置文件结构
系统 SHALL 支持 TOML 配置文件，包含 `gui_enabled` 字段控制默认启动模式。

#### Scenario: 配置文件包含 gui_enabled
- **WHEN** 配置文件设置 `gui_enabled = true`
- **THEN** 默认启动 GUI 模式

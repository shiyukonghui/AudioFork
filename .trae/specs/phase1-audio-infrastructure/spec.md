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

#### Scenario:
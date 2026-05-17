# AudioFork / 音频路由器

[English](#english) | [中文](#chinese)

---

<a name="chinese"></a>

## 中文说明

### 项目简介

AudioFork 是一款高性能的音频路由工具，支持将音频从一个输入源分发到多个输出设备。适用于直播、录音、多设备监听等场景。

### 主要特性

- **多输出扇出路由**：将单个音频输入同时路由到多个输出设备
- **Loopback 回采模式**：捕获系统播放的音频（如音乐播放器、视频等）
- **智能重采样**：支持不同采样率的输出设备（Sinc/Cubic算法）
- **砖墙限幅器**：防止削波失真，保护音频质量
- **时钟漂移补偿**：自动补偿不同设备间的时钟偏差
- **热插拔监听**：动态检测设备的连接与断开
- **声道映射**：自动处理不同声道数的输入输出转换
- **平滑淡入淡出**：启动和停止时无爆音
- **GUI 图形界面**：基于 egui 的跨平台原生界面
- **CLI 命令行模式**：支持无界面运行和自动化集成

### 构建方式

```bash
# 开发模式构建
cargo build

# Release 模式构建（最高优化）
cargo build --release
```

### 使用方式

#### GUI 模式（默认）

直接运行可执行文件即可启动图形界面：

```bash
audio_router.exe
```

或通过命令行指定：

```bash
audio_router.exe --gui
```

#### CLI 命令行模式

```bash
# 基本用法：指定输出设备
audio_router.exe --output-device "扬声器" --output-device "耳机"

# Loopback 模式：捕获系统音频
audio_router.exe --source-type loopback --loopback-device "扬声器" --output-device "耳机"

# 使用配置文件
audio_router.exe --config audio_router.toml

# 启用监控输出（JSON格式）
audio_router.exe --monitor --output-device "扬声器"
```

### 命令行参数

| 参数 | 说明 | 默认值 |
|------|------|--------|
| `--input-device` | 输入设备名称 | 系统默认 |
| `--output-device` | 输出设备名称（可多次指定） | 无 |
| `--sample-rate` | 采样率 (Hz) | 设备默认 |
| `--buffer-frames` | 缓冲区帧数 | 256 |
| `--max-latency-ms` | 最大允许延迟 (ms) | 30 |
| `--resampler` | 重采样算法 (sinc/cubic/none) | sinc |
| `--source-type` | 音源类型 (input/loopback) | input |
| `--loopback-device` | Loopback 回采目标设备 | 系统默认输出 |
| `--no-drift-compensation` | 禁用漂移补偿 | false |
| `--no-limiter` | 禁用限幅器 | false |
| `--monitor` | 启用 JSON 监控输出 | false |
| `--gui` | 启动图形界面 | false |
| `--config` | 配置文件路径 | audio_router.toml |

### 配置文件示例

```toml
# audio_router.toml
input_device = "麦克风"
output_devices = ["扬声器", "耳机"]
sample_rate = 48000
buffer_frames = 256
resampler = "sinc"
source_type = "input"
```

### 技术架构

- **音频后端**：cpal（跨平台音频库）
- **GUI 框架**：egui + eframe
- **重采样**：rubato（高质量重采样库）
- **缓冲区**：ringbuf（SPSC 环形缓冲区）
- **日志系统**：tracing + tracing-subscriber

### 系统要求

- Windows 10/11（主要支持平台）
- Rust 1.70+（编译需要）

### 许可证

本项目采用 Apache License 2.0 许可证，详见 [LICENSE](LICENSE) 文件。

---

<a name="english"></a>

## English Documentation

### Overview

AudioFork is a high-performance audio routing tool that distributes audio from a single input source to multiple output devices. Ideal for live streaming, recording, and multi-device monitoring scenarios.

### Key Features

- **Multi-output fan-out routing**: Route single audio input to multiple output devices simultaneously
- **Loopback capture mode**: Capture system audio output (music players, videos, etc.)
- **Smart resampling**: Support output devices with different sample rates (Sinc/Cubic algorithms)
- **Brickwall limiter**: Prevent clipping distortion, protect audio quality
- **Clock drift compensation**: Automatically compensate for clock drift between devices
- **Hot-plug monitoring**: Dynamically detect device connection and disconnection
- **Channel mapping**: Automatic conversion between different channel counts
- **Smooth fade-in/fade-out**: No audio pops during start/stop
- **GUI interface**: Cross-platform native UI based on egui
- **CLI mode**: Support headless operation and automation integration

### Build

```bash
# Development build
cargo build

# Release build (maximum optimization)
cargo build --release
```

### Usage

#### GUI Mode (Default)

Run the executable directly to launch the GUI:

```bash
audio_router.exe
```

Or specify via command line:

```bash
audio_router.exe --gui
```

#### CLI Mode

```bash
# Basic usage: specify output devices
audio_router.exe --output-device "Speaker" --output-device "Headphones"

# Loopback mode: capture system audio
audio_router.exe --source-type loopback --loopback-device "Speaker" --output-device "Headphones"

# Use configuration file
audio_router.exe --config audio_router.toml

# Enable monitoring output (JSON format)
audio_router.exe --monitor --output-device "Speaker"
```

### Command Line Arguments

| Argument | Description | Default |
|----------|-------------|---------|
| `--input-device` | Input device name | System default |
| `--output-device` | Output device name (can be specified multiple times) | None |
| `--sample-rate` | Sample rate (Hz) | Device default |
| `--buffer-frames` | Buffer frames | 256 |
| `--max-latency-ms` | Maximum allowed latency (ms) | 30 |
| `--resampler` | Resampling algorithm (sinc/cubic/none) | sinc |
| `--source-type` | Source type (input/loopback) | input |
| `--loopback-device` | Loopback capture target device | System default output |
| `--no-drift-compensation` | Disable drift compensation | false |
| `--no-limiter` | Disable limiter | false |
| `--monitor` | Enable JSON monitoring output | false |
| `--gui` | Launch GUI | false |
| `--config` | Configuration file path | audio_router.toml |

### Configuration File Example

```toml
# audio_router.toml
input_device = "Microphone"
output_devices = ["Speaker", "Headphones"]
sample_rate = 48000
buffer_frames = 256
resampler = "sinc"
source_type = "input"
```

### Technical Architecture

- **Audio backend**: cpal (cross-platform audio library)
- **GUI framework**: egui + eframe
- **Resampling**: rubato (high-quality resampling library)
- **Buffer**: ringbuf (SPSC ring buffer)
- **Logging**: tracing + tracing-subscriber

### System Requirements

- Windows 10/11 (primary supported platform)
- Rust 1.70+ (for compilation)

### License

This project is licensed under the Apache License 2.0. See the [LICENSE](LICENSE) file for details.
# Tasks: Phase 1 音频基础设施

## 任务概览

每个子代理严格只负责一个文件的开发，按依赖顺序执行。

---

- [x] Task 1: 配置 `Cargo.toml` — 声明 Phase 1 所有依赖与 wasapi-exclusive feature
  - 文件：`Cargo.toml`
  - 内容：
    - 添加依赖：`cpal`、`clap`（derive feature）、`toml`、`serde`（derive feature）、`log`、`tracing`、`tracing-subscriber`、`tracing-appender`
    - 添加 `[features]` 块：`wasapi-exclusive = ["dep:wasapi-rs"]`
    - `wasapi-rs` 作为可选依赖（`optional = true`）
  - 验证：`cargo check` 通过

- [x] Task 2: 创建 `error.rs` — 统一错误类型定义
  - 文件：`src/error.rs`
  - 内容：
    - `AudioRouterError` 枚举：`DeviceNotFound(String)`、`StreamError(String)`、`ConfigError(String)`、`Fatal(String)`、`ChannelError(String)`
    - 实现 `std::fmt::Display` 和 `std::error::Error`
    - `ErrorSeverity` 枚举：`Recoverable`、`Degraded`、`Fatal`
    - `RecoveryState` 枚举：`Normal`、`ReconnectBackoff { attempt: u32, next_interval: Duration }`、`FallbackToDefault { attempt: u32 }`
    - 模块级 `pub type Result<T> = std::result::Result<T, AudioRouterError>;`
    - 中文注释
  - 验证：`cargo check --lib` 通过

- [x] Task 3: 创建 `config.rs` — 配置文件结构体与 TOML 读写
  - 文件：`src/config.rs`
  - 内容：
    - `AudioRouterConfig` 结构体，包含字段：
      - `input_device: Option<String>`
      - `output_devices: Vec<String>`
      - `sample_rate: Option<u32>`
      - `channels: Option<u16>`
      - `buffer_frames: Option<u32>`
      - `max_latency_ms: Option<u32>`（默认 30）
      - `resampler: String`（默认 `"sinc"`）
      - `no_drift_compensation: bool`
      - `exit_on_input_loss: bool`
      - `input_fallback_to_default: bool`（默认 true）
      - `log_file: Option<String>`
      - `monitor: bool`
      - `wasapi_exclusive: bool`
      - `gui_enabled: bool`
    - `AudioRouterConfig::default()` 实现
    - `load_config(path: &Path) -> Result<AudioRouterConfig>` — 从 TOML 文件加载
    - `save_config(config: &AudioRouterConfig, path: &Path) -> Result<()>` — 保存为 TOML
    - 中文注释
  - 验证：`cargo check --lib` 通过

- [x] Task 4: 创建 `cli.rs` — CLI 参数定义与解析
  - 文件：`src/cli.rs`
  - 内容：
    - 使用 `clap`（derive 模式）定义 `CliArgs` 结构体
    - 覆盖所有参数（见开发规划 1.3 节）：
      - `--input-device`、`--output-device`（可多次）、`--sample-rate`、`--channels`、`--buffer-frames`、`--max-latency-ms`、`--resampler`、`--no-drift-compensation`、`--exit-on-input-loss`、`--input-fallback-to-default`、`--log-file`、`--monitor`、`--wasapi-exclusive`、`--gui`、`--config`、`--preset`
    - `--gui` 与 `--monitor` 互斥校验（clap 内置 `conflicts_with`）
    - `--wasapi-exclusive` 用 `#[cfg(feature = "wasapi-exclusive")]` 条件编译
    - `CliArgs::merge_with_config(&self, config: &AudioRouterConfig) -> ResolvedConfig` — 合并 CLI 与配置文件
    - `ResolvedConfig` 结构体（最终解析后的配置，所有 `Option` 已消解）
    - GUI 模式下参数过滤逻辑（仅保留 `--config` 和 `--gui`，其余 warn）
    - 中文注释
  - 验证：`cargo check --lib` 通过

- [x] Task 5: 创建 `device.rs` — 设备枚举与选择
  - 文件：`src/device.rs`
  - 内容：
    - `DeviceType` 枚举：`Wired`、`Bluetooth`、`Network`、`Unknown`
    - `DeviceInfo` 结构体：name、sample_rates、channels、formats、device_type
    - `enumerate_input_devices() -> Result<Vec<DeviceInfo>>`
    - `enumerate_output_devices() -> Result<Vec<DeviceInfo>>`
    - `select_input_device(name: Option<&str>) -> Result<(cpal::Device, DeviceInfo)>`
      - 若 `None`，返回系统默认
      - 若 `Some`，按名称模糊匹配（含指定字符串即可）
    - `select_output_devices(names: &[String]) -> Result<Vec<(cpal::Device, DeviceInfo)>>`
      - 若列表为空，返回所有有效输出设备
    - 蓝牙/网络设备检测（通过名称关键字判断）
    - 过滤纯输入设备（输出枚举时）
    - 中文注释
  - 验证：`cargo check --lib` 通过

- [x] Task 6: 创建 `audio/mod.rs` — 音频模块入口
  - 文件：`src/audio/mod.rs`
  - 内容：
    - 声明子模块 `pub mod capture;` 和 `pub mod playback;`
    - 导出 `capture::CaptureStream` 和 `playback::PlaybackStream`
    - 中文注释
  - 验证：`cargo check --lib` 通过

- [x] Task 7: 创建 `audio/capture.rs` — 输入流封装
  - 文件：`src/audio/capture.rs`
  - 内容：
    - `CaptureStream` 结构体，封装 `cpal::Stream`
    - `CaptureStream::new(device: &cpal::Device, config: &cpal::StreamConfig, on_data: F) -> Result<Self>`
      - `F: FnMut(&[f32]) + Send + 'static`
    - `CaptureStream::pause()` / `CaptureStream::resume()`
    - `Drop` 实现确保流正确释放
    - 中文注释
  - 验证：`cargo check --lib` 通过

- [x] Task 8: 创建 `audio/playback.rs` — 输出流封装
  - 文件：`src/audio/playback.rs`
  - 内容：
    - `PlaybackStream` 结构体，封装 `cpal::Stream` 和就绪标志 `AtomicBool`
    - `PlaybackStream::new(device: &cpal::Device, config: &cpal::StreamConfig, on_data: F) -> Result<Self>`
      - `F: FnMut(&mut [f32]) + Send + 'static`
      - 创建后在回调首次触发时设置就绪标志
    - `is_ready(&self) -> bool` — 查询就绪标志
    - `wait_ready(&self, timeout: Duration) -> bool` — 阻塞等待就绪（轮询 + sleep）
    - `PlaybackStream::pause()` / `PlaybackStream::resume()`
    - `Drop` 实现确保流正确释放
    - 中文注释
  - 验证：`cargo check --lib` 通过

- [x] Task 9: 创建 `main.rs` — 主入口与 Phase 1 流程编排
  - 文件：`src/main.rs`
  - 内容：
    - 声明模块：`mod cli; mod config; mod device; mod audio; mod error;`
    - 初始化 tracing 日志（控制台 + 可选文件日志）
    - 解析 CLI → 加载配置 → 合并 → 参数校验
    - 选择输入设备
    - 选择输出设备（Phase 1 只取第一个输出设备做单路直通）
    - 校验输入/输出参数是否匹配（采样率、声道数），不匹配则拒绝启动
    - 直通方案：使用 `Arc<Mutex<VecDeque<f32>>>` 共享缓冲区，输入回调写入，输出回调读取拷贝
    - 创建输出流（先启动，等待就绪，最多 5 秒超时），然后创建输入流
    - Ctrl+C 信号处理（优雅退出）
    - 中文注释
  - 验证：`cargo build` 通过（待实际音频设备测试）
  - **依赖**：Task 1-8 全部完成

---

## 任务依赖关系

```
Task 1 (Cargo.toml)
 └─► Task 2 (error.rs) ─────────────────────────────────────┐
      └─► Task 3 (config.rs) ──► Task 4 (cli.rs) ──────────┤
           └─► Task 5 (device.rs) ─────────────────────────┤
                └─► Task 6 (audio/mod.rs) ─────────────────┤
                     ├─► Task 7 (audio/capture.rs) ────────┤
                     └─► Task 8 (audio/playback.rs) ───────┤
                          └─► Task 9 (main.rs) ◄───────────┘
```

- Task 1 无依赖，最先执行
- Task 2 依赖 Task 1（需要 Cargo.toml 就绪才能 cargo check）
- Task 3 依赖 Task 2（config 需要 error 类型）
- Task 4 依赖 Task 3（CLI 合并需要 config 类型）
- Task 5 依赖 Task 2（device 需要 error 类型）
- Task 6 依赖 Task 1（需要模块路径就绪）
- Task 7、Task 8 依赖 Task 5、Task 6
- Task 9 依赖所有前序任务

并行可执行：
- Task 3 和 Task 5 可并行（都只依赖 Task 2）
- Task 7 和 Task 8 可并行（都依赖 Task 5、Task 6）

# Tasks: Phase 3 高级鲁棒性

## 任务概览

每个子代理严格只负责**一个文件**的开发或改建，按依赖顺序执行。可并行任务会在依赖关系中标注。

---

- [x] Task 1: 添加 Phase 3 依赖到 `Cargo.toml`
  - 文件：`Cargo.toml`
  - 内容：
    - 在 `[dependencies]` 中添加：
      - `rubato = "2"` — 重采样库
      - `crossbeam-channel = "0.5"` — 跨线程消息通道（用于监控线程和热插拔事件传递）
    - 保留现有依赖（`cpal`, `clap`, `toml`, `serde`, `log`, `tracing*`, `ringbuf`, `wasapi`）
  - 验证：`cargo check` 通过

- [x] Task 2: 创建 `src/resample.rs` — 重采样器封装
  - 文件：`src/resample.rs`（新建）
  - 内容：
    - **枚举 `ResamplerType`**：`Sinc` / `Cubic` / `None`
    - **枚举 `ResampleProcessor`**：使用 rubato v2 API（`Async<f32>` 替代旧的 `SincFixedIn`，`Fft<f32>` 替代旧的 `FftFixedIn`）。用 enum variant + 内部辅助结构体 `SincState` / `CubicState` 区分。
      - `Sinc(SincState)` — 异步 sinc 高质量重采样
      - `Cubic(CubicState)` — 同步 FFT 快速重采样
      - `PassThrough` — 无重采样时直接拷贝
    - **方法**：
      - `new(algorithm: ResamplerType, input_rate: f64, output_rate: f64, channels: usize, chunk_size: usize) -> Result<Self, String>` — 根据 algorithm 创建对应的重采样器。Sinc 使用 `Async::new_sinc(...)`，Cubic 使用 `Fft::new(...)`，同采样率或 None 返回 `PassThrough`。
      - `process(&mut self, input: &[f32], output: &mut [f32]) -> Result<(usize, usize), String>` — 输入交错帧，输出交错帧，返回 (输入消耗帧数, 输出产生帧数)。Sinc/Cubic 调用 `process_into_buffer`（通过 `SequentialSliceOfVecs` 适配器），PassThrough 直接拷贝。
      - `input_frames_next(&self) -> usize` — 返回下一次 process 需要的输入帧数。
      - `output_frames_next(&self) -> usize` — 返回下一次 process 能产出的输出帧数。
      - `output_delay(&self) -> usize` — 返回输出延迟帧数。
      - `reset(&mut self)` — 重置重采样器内部状态。
      - `is_passthrough(&self) -> bool` — 是否为直通（无重采样）。
      - `input_sample_rate(&self) -> f64` / `output_sample_rate(&self) -> f64` — 返回输入/输出采样率。
      - `set_input_sample_rate(&mut self, rate: f64)` / `set_output_sample_rate(&mut self, rate: f64)` — 动态更新采样率（仅 Sinc 异步模式支持）。
    - **ResampleProcessor 满足 `Send` trait 边界**（`Async<f32>` 和 `Fft<f32>` 均实现 Send）。
    - **rubato v2 使用注意事项**：
      - 使用 rubato 重导出的 `audioadapter_buffers::direct::SequentialSliceOfVecs` 包装 `Vec<Vec<f32>>`
      - 使用 `Indexing` 结构体处理 partial_len
    - **中文注释**
    - **含 8 个单元测试**（直通、Sinc 创建、Cubic 创建、output_delay、reset、PassThrough 辅助方法、ResamplerType 比较、采样率 setter）
  - 验证：`cargo check` 通过（0 errors；本项目为 binary crate，使用 `cargo check` 而非 `--lib`）
  - **依赖**：Task 1（rubato 依赖）

- [x] Task 3: 创建 `src/limiter.rs` — 砖墙限幅器
  - 文件：`src/limiter.rs`（新建）
  - 内容：
    - **结构体 `BrickwallLimiter`**：
      - `threshold: f32` — 阈值（0dBFS = 1.0，使用线性值）
      - `release_coeff: f32` — 释放系数（从 release 时间 ms 换算：`exp(-1.0 / (sample_rate * release_ms / 1000.0))`）
      - `envelope: Vec<f32>` — 每个声道的峰值包络状态
      - `attack_coeff: f32` — 攻击系数（0 攻击 = `attack_coeff = 1.0`，即瞬间响应）
      - `enabled: bool` — 是否启用
    - **方法**：
      - `new(threshold: f32, attack_ms: f32, release_ms: f32, channels: usize, sample_rate: f64, enabled: bool) -> Self` — 初始化，attack=0 时 attack_coeff=1.0
      - `process(&mut self, buffer: &mut [f32], channels: u16)` — 逐声道处理：对每个样本计算峰值包络 → 计算增益衰减 → 应用到样本
        - 若 `!enabled`，直接返回不处理
        - 峰值检测：`peak = sample.abs()`
        - 包络更新：若 `peak > envelope[ch]` 则 `envelope[ch] = attack_coeff * peak + (1-attack_coeff) * envelope[ch]`；否则 `envelope[ch] = release_coeff * envelope[ch] + (1-release_coeff) * 0.0`
        - 增益计算：`gain = threshold / envelope[ch].max(threshold)` 或直接 `gain = threshold / envelope[ch]`（钳位后 max(1.0) 即不做放大只做衰减）
        - 实际做法：`gain = 1.0.min(threshold / envelope[ch].max(1e-10))` 然后 `sample *= gain`
      - `set_enabled(&mut self, enabled: bool)` — 运行时切换限幅器开关
      - `reset(&mut self)` — 重置包络状态为 0
    - **Send + 'static 要求**（因会进入 `Arc<Mutex<BrickwallLimiter>>` 在音频回调中使用）
    - **中文注释**
  - 验证：`cargo check --lib` 通过
  - **依赖**：无（独立模块）

- [x] Task 4: 创建 `src/drift.rs` — 时钟漂移补偿
  - 文件：`src/drift.rs`（新建）
  - 内容：
    - **结构体 `DriftCompensator`**：
      - `enabled: bool`
      - `ema_alpha: f64` — EMA 平滑系数（从时间常数 3s 换算：`alpha = 1.0 - exp(-dt / tau)`，其中 dt ≈ 0.1s，tau = 3.0s）
      - `ema_level: f64` — 当前 EMA 水位（百分比 0~1）
      - `target_level: f64` — 目标水位（= 0.5，一半容量）
      - `kp: f64` — PI 控制器比例增益
      - `ki: f64` — PI 控制器积分增益
      - `integral: f64` — 积分累加器
      - `delta: AtomicI32` — 帧数微调量（-1/0/+1），供音频回调读取
    - **方法**：
      - `new(enabled: bool, sample_rate: f64, buffer_frames: usize) -> Self` — 计算 alpha、kp、ki 等参数
      - `update(&mut self, water_level: f64)` — 被控制线程每 100ms 调用：
        1. 更新 EMA：`ema_level = alpha * water_level + (1 - alpha) * ema_level`
        2. 计算误差：`error = (target_level - ema_level) * 100.0`（放大到百分比便于 PI 调参）
        3. 积分项：`integral = integral * 0.99 + ki * error`（弱泄漏防积分饱和）
        4. 输出：`raw = kp * error + integral`
        5. 量化 delta：`raw > 0.5 → delta = 1`，`raw < -0.5 → delta = -1`，否则 `delta = 0`
        6. 若 `!enabled`，delta 固定为 0
        7. `AtomicI32::store(delta, Release)`
      - `delta(&self) -> i32` — 读取当前 delta 值（`AtomicI32::load(Acquire)`）
      - `water_level_pct(&self) -> f64` — 返回当前 EMA 水位的百分比值（0~100）
      - `set_enabled(&mut self, enabled: bool)` — 运行时切换漂移补偿开关
    - **PI 调参建议**（在模块文档注释中说明）：
      - kp ≈ 0.02, ki ≈ 0.001 作为初始值，需根据实际运行调优
    - **中文注释**
  - 验证：`cargo check --lib` 通过
  - **依赖**：无（独立模块）

- [x] Task 5: 创建 `src/recovery.rs` — 输入设备丢失恢复状态机
  - 文件：`src/recovery.rs`（新建）
  - 内容：
    - 使用 `error.rs` 中已有的 `RecoveryState` 枚举
    - **结构体 `InputRecoveryManager`**：
      - `state: RecoveryState`
      - `exit_on_loss: bool` — 是否丢失后直接退出
      - `fallback_to_default: bool` — 是否在 30s 后降级默认设备
      - `input_lost: Arc<AtomicBool>` — 供音频回调读取的输入丢失标志
      - `lost_time: Option<std::time::Instant>` — 输入丢失发生的时刻
      - `original_device_name: Option<String>` — 丢失的原始设备名
    - **方法**：
      - `new(exit_on_loss: bool, fallback_to_default: bool, original_device_name: Option<String>) -> Self` — 创建管理器，状态初始为 Normal
      - `on_input_lost(&mut self)` — 输入丢失时调用：
        - 若 exit_on_loss → 返回需要退出的信号（通过 Result 或单独方法）
        - 否则 → 设置 `input_lost.store(true, Release)`，记录 lost_time，状态切换至 `ReconnectBackoff { attempt: 0, next_interval: Duration::from_millis(100) }`
      - `on_input_recovered(&mut self)` — 输入恢复时调用：设置 `input_lost.store(false, Release)`，状态切换至 Normal
      - `tick(&mut self) -> RecoveryAction` — 被控制线程定时调用（每 100ms）：
        - Normal → 返回 `RecoveryAction::None`
        - ReconnectBackoff → 检查是否到达重连间隔：
          - 若累计时间 < 30s：返回 `RecoveryAction::TryReconnectOriginal`，更新 attempt 和 next_interval
          - 若累计时间 >= 30s：若 fallback_to_default → 切换至 FallbackToDefault { attempt: 0 }，返回 `RecoveryAction::TryFallbackToDefault`；否则 → 保持 ReconnectBackoff（间隔固定 5s）
        - FallbackToDefault → 每次间隔 5s 返回 `RecoveryAction::TryFallbackToDefault`
      - `should_exit(&self) -> bool` — 返回是否应退出（exit_on_loss 为 true 时）
      - `input_lost_flag(&self) -> Arc<AtomicBool>` — 返回 input_lost 的 Arc，供音频回调检测
    - **枚举 `RecoveryAction`**：
      - `None` — 无操作
      - `TryReconnectOriginal` — 尝试重连原设备
      - `TryFallbackToDefault` — 尝试连接默认设备
    - **中文注释**
  - 验证：`cargo check --lib` 通过
  - **依赖**：依赖 `src/error.rs`（RecoveryState 已在 Phase 1 定义）

- [x] Task 6: 创建 `src/hotplug.rs` — 设备热插拔监听
  - 文件：`src/hotplug.rs`（新建）
  - 内容：
    - **Trait `DeviceNotifier`**：
      ```rust
      pub trait DeviceNotifier: Send {
          fn start(&mut self, on_add: Box<dyn Fn(DeviceInfo) + Send + 'static>, on_remove: Box<dyn Fn(String) + Send + 'static>);
          fn stop(&mut self);
      }
      ```
    - **结构体 `HotplugEvent`**：
      ```rust
      pub enum HotplugEvent {
          DeviceAdded(DeviceInfo),
          DeviceRemoved(String),
      }
      ```
    - **`hotplug_channel()` 工厂函数**：
      - 返回 `(crossbeam_channel::Receiver<HotplugEvent>, Box<dyn DeviceNotifier>)`
      - （简化方案：不创建实际 trait 对象，而是通过 channel 将事件从监听线程发送到主线程）
    - **平台特定实现（简化版，使用 channel）**：
      - 启动一个后台线程，周期性（每 2 秒）调用 `device::enumerate_output_devices()` 对比当前活跃设备列表
      - 若检测到新设备 → channel 发送 `HotplugEvent::DeviceAdded(info)`
      - 若检测到设备移除 → channel 发送 `HotplugEvent::DeviceRemoved(name)`
    - **命名方案**：使用 polling 方式比原生 WASAPI/CoreAudio 通知更简单可靠，避免跨库混用问题
    - 提供 `start_hotplug_monitor(tx: crossbeam_channel::Sender<HotplugEvent>, poll_interval: Duration) -> JoinHandle<()>` 函数
    - **中文注释**
  - 验证：`cargo check --lib` 通过
  - **依赖**：依赖 Task 1（crossbeam-channel）, 依赖 `src/device.rs`

- [x] Task 7: 添加 `--no-limiter` CLI 参数和配置字段
  - 文件：`src/cli.rs` 和 `src/config.rs`
  - 内容：
    - **cli.rs**：
      - 在 `CliArgs` 添加 `#[arg(long = "no-limiter", default_value_t = false)] pub no_limiter: bool` 字段
      - 在 `ResolvedConfig` 添加 `pub no_limiter: bool` 字段
      - 在 `merge_with_config` 中添加 no_limiter 的合并逻辑（CLI true → 覆盖 config）
    - **config.rs**：
      - 在 `AudioRouterConfig` 添加 `pub no_limiter: bool` 字段，默认值 false
    - **中文注释**
  - 验证：`cargo check --lib` 通过
  - **依赖**：无（独立修改）

- [x] Task 8: 改建 `src/pipeline.rs` — 集成重采样、漂移补偿、限幅器
  - 文件：`src/pipeline.rs`（改建）
  - 内容：
    - **(A) 修改 `create_output_callback` 函数签名**：
      增加参数：
      - `resampler: Arc<Mutex<crate::resample::ResampleProcessor>>` — 重采样器
      - `limiter: Arc<Mutex<crate::limiter::BrickwallLimiter>>` — 限幅器
      - `delta_var: Arc<AtomicI32>` — 漂移补偿的 delta 值
      - `input_lost: Arc<AtomicBool>` — 输入丢失标志
      - `no_drift_compensation: bool` — 是否禁用漂移补偿（影响直通判断）
    - **(B) 修改回调逻辑**：
      1. 判断直通模式（Phase 3 条件）：`input_sample_rate == output_sample_rate && input_channels == output_channels && no_drift_compensation`
      2. 若无漂移补偿 → 直通模式：数据从 SPSC 直接拷贝至输出缓冲区
      3. 若有漂移补偿（即使采样率和声道相同）→ 转换模式最小路径：
         - 从 SPSC 取 `N_out + delta` 帧
         - 跳过声道映射（声道相同）
         - 跳过重采样（采样率相同）
         - 只做 delta 帧数微调
      4. 若采样率或声道不同 → 完整转换路径：
         - 加载 `input_lost` 标志，若为 true → 输出静音 + 淡出
         - 计算 `N_in_target`，从 SPSC 取数据
         - `ChannelMapper::map()` → 声道映射
         - `BrickwallLimiter::process()` → 限幅（在重采样前，如规划所述：声道映射之后、重采样之前）
         - `ResampleProcessor::process()` → 重采样
         - 应用 delta 调整（`output[..N_out+delta]` 或类似逻辑）
         - 写入输出缓冲区
      5. 欠载处理：保持 Phase 2 逻辑，但增加重采样器 reset
    - **(C) 保留现有所有代码**：SlotArray、Slot、Fader、COSINE_WINDOW、create_input_callback 均保持不变
    - **中文注释**
  - 验证：`cargo check --lib` 通过
  - **依赖**：Task 2 (resample.rs), Task 3 (limiter.rs), Task 4 (drift.rs)

- [x] Task 9: 改建 `src/main.rs` — Phase 3 主流程
  - 文件：`src/main.rs`（改建）
  - 内容：
    - **(A) 模块声明**：添加 `mod resample; mod drift; mod limiter; mod hotplug; mod recovery;`
    
    - **(B) 移除 Phase 2 采样率同质化限制**：删除步骤 8 的采样率兼容性校验循环，改为每个输出设备使用其实际支持的采样率（选择最接近输入采样率的配置），通过 resampler 处理采样率差异
    
    - **(C) 输出设备采样率选择**：对每个输出设备，从 `DeviceInfo.sample_rates` 中选最佳采样率：
      - 优先选与输入采样率相同的
      - 其次选与输入最接近的
      - 若支持列表为空，使用输入采样率
    
    - **(D) 创建重采样器和限幅器**：在创建每个 OutputSlot 时：
      - 若采样率不同 → 创建 `ResampleProcessor`（根据 `resolved.resampler` 选择算法）
      - 若 `--resampler none` 且采样率不同 → 拒绝启动
      - 创建 `BrickwallLimiter`（默认启用，`--no-limiter` 禁用）
      - 创建 `DriftCompensator`（若 `!resolved.no_drift_compensation`）
    
    - **(E) 修改 OutputSlot 结构体**：增加字段：
      - `resampler: Option<Arc<Mutex<ResampleProcessor>>>`
      - `limiter: Arc<Mutex<BrickwallLimiter>>`
      - `delta: Arc<AtomicI32>`
      - `drift: Option<Arc<Mutex<DriftCompensator>>>`
      - `input_lost: Arc<AtomicBool>`
    
    - **(F) Ctrl+C 信号替换 Enter**：使用 `ctrlc` 的 `set_handler`（或自行使用 `std::sync::Condvar` + SIGINT handler）替换 `std::io::stdin().read_line()`
      - 注：不添加 ctrlc 依赖，直接用 `std::io::stdin().read_line()` 同时保留 Enter 和 Ctrl+C（Ctrl+C 会触发默认的 SIGINT，产生错误返回，等价于读行结束）
    
    - **(G) 漂移补偿控制线程**：
      - 若漂移补偿启用 → 启动独立线程，每 100ms 轮询各输出的环形缓冲区水位
      - 每个输出：`water_level = consumer.occupied_len() / buffer_capacity`
      - 调用 `DriftCompensator::update(water_level)`
      - delta 通过 `AtomicI32` 传递给输出回调
    
    - **(H) 热插拔监控线程**：
      - 调用 `hotplug::start_hotplug_monitor()` 启动设备变更检测
      - 主循环中同时监听 `hotplug_rx` 和停止信号
      - `DeviceAdded` → 分配新槽位，创建 SPSC/输出流/重采样器/限幅器，启动播放
      - `DeviceRemoved` → 停用对应槽位，记录日志
    
    - **(I) 输入丢失恢复**：
      - 创建 `InputRecoveryManager`
      - 在 cpal 的输入错误回调中调用 `recovery.on_input_lost()`
      - 主循环中调用 `recovery.tick()` 处理重连逻辑
    
    - **(J) 监控 JSON 输出**：
      - 若 `resolved.monitor` → 启动独立线程，每 5 秒采集统计并输出 JSON 行至 stdout
      - JSON 格式：`{"ts":...,"outputs":[{"name":"...","underruns":...,"overflows":...,"latency_ms":...,"delta":...,"water_level_pct":...}]}`
      - 采集数据从 `OutputSlot` 的统计计数器和 `DriftCompensator` 获取
    
    - **(K) WASAPI 独占模式**：
      - 若 `wasapi-exclusive` feature 启用且 `resolved.wasapi_exclusive` → 对支持独占的输出设备使用 `wasapi-rs` 创建独占流
      - 独占模式创建失败 → 降级为共享模式并 warn
    
    - **保留现有**：日志初始化、配置加载、设备枚举、管道创建、停止流程均保留
    
    - **中文注释**
  - 验证：`cargo build --release` 通过
  - **依赖**：Task 1-8 全部完成，Task 5,6,7,8 完成后才可执行

---

## 任务依赖关系

```
Task 1 (Cargo.toml)
 ├─► Task 2 (resample.rs)
 ├─► Task 6 (hotplug.rs) ──┐
 ├─► Task 3 (limiter.rs)   ├──┐
 ├─► Task 4 (drift.rs)     │  │
 ├─► Task 5 (recovery.rs)  │  │
 └─► Task 7 (cli+config)   │  │
                           │  │
 Task 2 + 3 + 4 ──► Task 8 (pipeline.rs) ──► Task 9 (main.rs)
 Task 5 + 6 + 7 + 8 ──────────────────────────┘
```

**可并行执行的任务组合：**
- Task 2, Task 3, Task 4, Task 5, Task 7 可在 Task 1 完成后**同时执行**
- Task 6 依赖 `crossbeam-channel`（Task 1）+ `device.rs`（已存在），可与 2/3/4/5/7 并行

**串行执行的任务：**
- Task 8 必须在 Task 2, 3, 4 完成后执行
- Task 9 必须在 Task 5, 6, 7, 8 全部完成后执行

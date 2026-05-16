# Tasks: Phase 2 核心分发管道

## 任务概览

每个子代理严格只负责一个文件的开发，按依赖顺序执行。

---

- [x] Task 1: 添加 `ringbuf` 依赖到 `Cargo.toml`
  - 文件：`Cargo.toml`
  - 内容：
    - 在 `[dependencies]` 中添加 `ringbuf = "0.5"`
    - 不修改其他内容
  - 验证：`cargo check` 通过，ringbuf 下载成功

- [x] Task 2: 创建 `src/channel_map.rs` — 声道映射器（ITU-R BS.775）
  - 文件：`src/channel_map.rs`
  - 内容：
    - `ChannelMapper` 结构体，字段 `input_channels: u16` 和 `output_channels: u16`
    - `ChannelMapper::new(input_channels: u16, output_channels: u16) -> Self`
    - `is_passthrough(&self) -> bool` — 声道数相同时返回 true
    - `map(&self, input: &[f32], output: &mut [f32])` — 执行声道映射
    - 映射策略（`map` 内部）：
      - 1→2: `output[0] = input[0]; output[1] = input[0]`
      - 2→1: `output[0] = 0.5*input[0] + 0.5*input[1]`
      - 6→2 (5.1→立体声，假设 L/C/R/Ls/Rs/LFE 通道序): `output[0] = 0.5*input[0] + 0.35*input[1] + 0.35*input[3]`, `output[1] = 0.5*input[2] + 0.35*input[1] + 0.35*input[4]`
      - 通用 fallback: 按逐通道拷贝，多余丢弃，不足填 0
    - 具体映射时，map 方法循环处理每帧：每次取 input_channels 个输入、产出 output_channels 个输出
    - 模块文档注释中引用 `ITU-R BS.775` 标准
    - 中文注释
  - 验证：`cargo check --lib` 通过

- [x] Task 3: 创建 `src/pipeline.rs` — SPSC 槽位管理与扇出管道
  - 文件：`src/pipeline.rs`
  - 内容：
    
    **(A) 常量与数据结构**
    - `const MAX_OUTPUTS: usize = 32` — 最大输出槽位数
    - `Slot` 结构体：`producer: Option<ringbuf::Producer<f32>>` + `active: AtomicBool`
    - `SlotArray` 结构体：`slots: [Slot; MAX_OUTPUTS]` — 直接使用数组而非 `[MaybeUninit]`（Phase 2 用 `Option<Producer>` 处理未初始化）

    **(B) SlotArray 方法**
    - `new() -> Self`：创建 32 个空槽位（所有 `active` 为 false，`producer` 为 None）
    - `allocate_slot(&self, producer: ringbuf::Producer<f32>) -> Option<usize>`：找空闲槽位 → 填入 Producer → `store(true, Release)` → 返回索引
    - `deactivate_slot(&self, index: usize)`：`store(false, Release)`，保留 Producer 不释放
    - `iter_active(&self) -> impl Iterator<Item = (usize, &ringbuf::Producer<f32>)>`：`Acquire` 读取 active 标志，仅返回活跃槽位的 `(index, Producer引用)`

    **(C) 余弦窗表**
    - 静态 `static COSINE_WINDOW: [f32; 256]` 使用 `std::sync::LazyLock`（或 `once_cell::sync::Lazy`，但我们没有 once_cell 依赖... 用 `std::sync::OnceLock` 或直接 `const` 初始化）
    - 用 const fn 初始化：`const COSINE_WINDOW: [f32; 256] = { ... 预计算值 ... }` 或使用 `std::sync::LazyLock`
    - 函数 `cosine_fade_gain(phase: f32) -> f32`：phase ∈ [0, 1]，返回 cos 窗增益（`0.5 - 0.5 * cos(π * phase)`），线性插值查 256 点表

    **(D) Fader 结构体**
    - `Fader` 结构体：`fade_out_samples: AtomicI32` — 负数=fade_out 剩余帧数，正数=fade_in 剩余帧数，0=正常
    - `Fader::new(fade_len_frames: usize) -> Self` — 设置初始淡出长度为 0（正常）
    - `start_fade_out(&self, len: usize)` — 原子设置 fade_out_samples = -len
    - `start_fade_in(&self, len: usize)` — 原子设置 fade_in_samples = len
    - `process(&self, buffer: &mut [f32], channels: u16)` — 对 buffer 中的每帧应用当前淡入淡出增益（逐帧递减/递增 phase）

    **(E) 生产者侧（扇出写入）— 函数签名**：
    ```rust
    pub fn create_input_callback(
        slot_array: Arc<SlotArray>,
        overflow_counters: Arc<[AtomicU64; MAX_OUTPUTS]>,
    ) -> impl FnMut(&[f32]) + Send + 'static
    ```
    逻辑：
    1. 遍历 `slot_array.iter_active()`
    2. 对每个活跃 Producer，检查 `producer.remaining()` >= data.len()
    3. 足够 → `producer.push_slice(data)`
    4. 不足 → 递增对应 `overflow_counters[index]`，继续下一槽位
    5. **回调内零堆分配、零锁、仅 Acquire 原子读 + SPSC 写入**

    **(F) 消费者侧（输出回调）— 函数签名**：
    ```rust
    pub fn create_output_callback(
        consumer: ringbuf::Consumer<f32>,
        fader: Arc<Fader>,
        channel_mapper: Arc<crate::channel_map::ChannelMapper>,
        input_channels: u16,
        output_channels: u16,
        input_sample_rate: u32,
        output_sample_rate: u32,
        underrun_counter: Arc<AtomicU64>,
    ) -> impl FnMut(&mut [f32]) + Send + 'static
    ```
    逻辑：
    1. 判断直通模式（same sample_rate && same channels）vs 转换模式
    2. 计算 `N_in_target = ceil(N_out * input_sample_rate / output_sample_rate)`（但 Phase 2 条件是相同采样率才有输出 = ratio=1，实际输入采样率和输出采样率可能不同... 暂不处理重采样，保持 ratio=1。不同采样率在 Phase 2 暂不接受。处理不同声道数即可。）
    3. 检查 consumer 可读样本数：
       - 足够 → 取出样本，声道映射，写入输出
       - 不足 → 触发 fader.start_fade_out() → 填充静音 → 递增 underrun_counter
    4. 恢复时触发 fader.start_fade_in()
    5. **回调 wrapper**: 在写 buffer 后调用 `fader.process(buffer, output_channels)`

    **(G) 统计结构体**
    - `OutputStats` 结构体：`device_name: String`, `underrun_count: u64`, `overflow_count: u64`
    - 提供 `snapshot(&self, ...) -> ...` 方法用于获取当前统计快照

    - 中文注释
  - 验证：`cargo check --lib` 通过
  - **依赖**：Task 1（ringbuf）、Task 2（ChannelMapper）

- [x] Task 4: 改造 `src/main.rs` — Phase 2 多输出扇出流程
  - 文件：`src/main.rs`
  - 替换当前 Phase 1 直通实现为 Phase 2 扇出管道
  - 保留现有的：模块声明、tracing 初始化、CLI 解析、配置加载合并、GUI 模式处理、设备枚举与打印
  - 新增/改造流程：
    **(A) 模块声明**：添加 `mod pipeline; mod channel_map;`
    
    **(B) 参数校验 — max-latency-ms**：
    ```rust
    let buffer_frames = resolved.buffer_frames;
    let sample_rate = resolved.sample_rate.unwrap_or(48000);
    let delay_ms = (buffer_frames as f64) * 1000.0 / (sample_rate as f64);
    if delay_ms > resolved.max_latency_ms as f64 {
        // 拒绝启动，给出清晰错误
    }
    ```
    
    **(C) 创建 SlotArray + overflow_counters**：
    ```rust
    let slot_array = Arc::new(pipeline::SlotArray::new());
    let overflow_counters = Arc::new(std::array::from_fn(|_| AtomicU64::new(0)));
    ```
    
    **(D) 为每个输出设备分配槽位并创建输出流**：
    - 遍历 `output_devices` 列表
    - 每个设备：创建 SPSC 队列（ringbuf capacity = buffer_frames * channels * 4）
    - 分配槽位（`slot_array.allocate_slot(producer)`）
    - 创建 ChannelMapper
    - 创建 Fader（fade_len = min(5ms等效帧数, callback_duration/2)）
    - 创建输出回调（`pipeline::create_output_callback(...)`）
    - 创建 PlaybackStream
    - 记录设备→槽位映射（Vec<StreamInfo> 含 playback/fader/index）
    
    **(E) 等待所有输出流就绪**：
    - 主线程依次轮询（或并行等待）所有 PlaybackStream 的 `ready` 标志
    - 每个设备 5 秒超时
    - 超时设备 → `slot_array.deactivate_slot(index)` + `tracing::error!` 日志
    - 全部超时 → `Fatal` 退出
    
    **(F) 创建输入回调并启动输入流**：
    ```rust
    let on_input = pipeline::create_input_callback(Arc::clone(&slot_array), Arc::clone(&overflow_counters));
    let capture = CaptureStream::new(&device_in, &input_config, on_input)?;
    ```
    
    **(G) 打印启动成功信息**：列出所有已启动的输出设备及其模式（直通/转换）
    
    **(H) 等待 Enter 停止 + 停止流程**：
    - 停止输入流（drop capture）
    - 对每个活跃设备启动淡出（`fader.start_fade_out(...)`）
    - 轮询等待所有 fade_complete（2 秒超时）
    - 超时设备强制 drop
    - drop 所有 playback
    - 打印每个设备的最终统计（溢出/欠载次数）

    - 结构体辅助：
      ```rust
      struct OutputSlot {
          playback: PlaybackStream,
          index: usize,
          fader: Arc<pipeline::Fader>,
          underrun_counter: Arc<AtomicU64>,
          device_name: String,
      }
      ```

    - 中文注释
  - 验证：`cargo build --release` 通过，运行不崩溃
  - **依赖**：Task 1, 2, 3 全部完成

---

## 任务依赖关系

```
Task 1 (Cargo.toml: ringbuf)
 ├─► Task 2 (channel_map.rs) ──► Task 3 (pipeline.rs) ──► Task 4 (main.rs)
 └─► Task 3 (pipeline.rs) ──────────────────────────────────┘
```

- Task 1 无依赖，最先执行
- Task 2 依赖 Task 1（需要 Cargo.toml 就有 ringbuf 才能 cargo check）
- Task 3 依赖 Task 1（ringbuf）和 Task 2（ChannelMapper 类型）
- Task 4 依赖 Task 1-3 全部

并行可执行：
- 无（Task 3 需要 Task 2 的类型定义，因此只能串行 Task2→Task3）

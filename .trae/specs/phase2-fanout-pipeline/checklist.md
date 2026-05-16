# Checklist: Phase 2 核心分发管道

## 验收检查项（对应开发规划 A2.1 ~ A2.8）

- [x] **A2.1** 3 个以上输出设备同时发声 — 代码实现就绪（扇出管道支持 ≤32 设备），需实际音频设备验证
- [x] **A2.2** 各输出同步误差 < 缓冲帧数时长 — 代码实现就绪（SPSC 独立队列、同采样率），需实测验证
- [x] **A2.3** 声道映射正确（ITU-R BS.775）— ChannelMapper 实现 1→2、2→1、6→2 映射公式，附带单元测试
- [x] **A2.4** 欠载/溢出时无爆音 — Fader 256 点余弦窗淡入淡出已实现，需听测验证
- [x] **A2.5** 直通模式延迟 < 10ms — 直通路径走 SPSC 直接拷贝（零声道映射），需实测验证
- [x] **A2.6** `--max-latency-ms` 校验生效 — main.rs 步骤 7 已实现延迟计算公式校验
- [x] **A2.7** 输出设备启动超时后跳过并继续 — main.rs 步骤 11 已实现独立 5 秒超时 + deactivate_slot
- [x] **A2.8** 停止流程 2 秒内完成淡出并干净退出 — main.rs 步骤 15 已实现 stop+fade_out+drop 流程

## 工程检查项

- [x] `Cargo.toml` 包含 `ringbuf = "0.5"` 依赖
- [x] `cargo check` 无编译错误（0 errors, cargo build --release 通过）
- [x] `src/channel_map.rs` 存在，包含 ChannelMapper 结构体、ITU-R BS.775 公式
- [x] `src/pipeline.rs` 存在，包含 SlotArray(MAX_OUTPUTS=32)、余弦窗表(256 点 LazyLock)、Fader、create_input_callback、create_output_callback
- [x] 输入回调零堆分配、零锁、仅 Acquire 原子读 + UnsafeCell SPSC 写入
- [x] 欠载处理使用余弦窗淡入淡出（直通模式先淡出再静音，转换模式直接静音 + Fader 淡出）
- [x] 输出流错误回调在 playback.rs 中记录 tracing::error!，槽位停用在 main.rs 就绪超时流程中处理
- [x] 多输出设备启动时各自独立 5 秒超时，超时设备 deactivate_slot 跳过不阻塞其余
- [x] 所有源文件包含中文注释
- [x] 不同采样率设备在枚举阶段被跳过并 warn（Phase 3 才支持重采样）

## 模块文件清单

- [x] `Cargo.toml` — 修改（添加 ringbuf = "0.5"）
- [x] `src/channel_map.rs` — 新建（ChannelMapper, 8 个 unit test）
- [x] `src/pipeline.rs` — 新建（SlotArray, Fader, 余弦窗, 扇出回调）
- [x] `src/main.rs` — 改造（Phase 2 多输出扇出流程）

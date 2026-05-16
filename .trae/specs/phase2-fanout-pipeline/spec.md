# Phase 2: 核心分发管道 Spec

## Why
Phase 1 仅实现单路 `Mutex<VecDeque>` 直通，Phase 2 需要升级为基于 `ringbuf` SPSC 无锁队列的扇出（fan-out）架构，实现一路输入同时分发至多个输出设备，并处理声道映射与欠载/溢出。

## What Changes
- 添加 `ringbuf` 依赖
- 新建 `src/pipeline.rs` — SPSC 槽位管理、扇出写入、输出消费、余弦淡入淡出
- 新建 `src/channel_map.rs` — 声道映射器（ITU-R BS.775）
- 改造 `src/main.rs` — 多输出设备扇出、完整启动/停止流程、`--max-latency-ms` 校验
- 输出流错误回调触发槽位停用

## Impact
- Affected specs: Phase 1 单路直通被替换为扇出管道
- Affected code: `Cargo.toml`, `src/main.rs`, `src/pipeline.rs`(新), `src/channel_map.rs`(新)

## ADDED Requirements

### Requirement: ringbuf SPSC 无锁队列集成
系统 SHALL 使用 `ringbuf` 库为每个输出设备创建独立的 `Producer`/`Consumer` 对，替换 Phase 1 的 `Arc<Mutex<VecDeque<f32>>>` 共享缓冲区。

#### Scenario: 生产者写入无阻塞
- **WHEN** 输入回调向 SPSC Producer 写入音频数据
- **THEN** 仅需原子检查和写入，不涉及堆分配或锁

### Requirement: 槽位管理
系统 SHALL 实现固定大小槽位数组（MAX_OUTPUTS=32），使用原子操作管理活跃状态。

#### Scenario: 分配槽位
- **WHEN** 调用 `allocate_slot(producer)` 且有空闲槽位
- **THEN** 先填充 Producer，再 `store(true, Release)` 设置 active，返回槽位索引

#### Scenario: 停用槽位
- **WHEN** 设备出错或用户手动停用
- **THEN** `store(false, Release)` 保留 Producer 不释放（延迟释放策略）

#### Scenario: 遍历活跃槽位
- **WHEN** 输入回调调用 `iter_active()`
- **THEN** `Acquire` 读取 active 标志，仅返回活跃槽位的 Producer 引用

### Requirement: 扇出输入回调
系统 SHALL 在输入回调中遍历所有活跃槽位，将每帧音频数据写入所有 SPSC 队列。

#### Scenario: 数据写入所有活跃输出
- **WHEN** 输入有 3 个活跃输出设备
- **THEN** 每帧数据写入 3 个 SPSC 队列

#### Scenario: 溢出处理
- **WHEN** 某槽位的 SPSC 队列剩余空间不足
- **THEN** 丢弃整帧，递增 overflow_count（AtomicU64），继续处理下一槽位

### Requirement: 输出回调（每设备独立）
系统 SHALL 为每个活跃输出设备创建独立的 cpal 输出流和回调线程。

#### Scenario: 直通模式
- **WHEN** 输入采样率 == 输出采样率 && 声道数相同
- **THEN** 数据从 SPSC Consumer 直接拷贝至输出缓冲区

#### Scenario: 转换模式（声道映射）
- **WHEN** 输入声道数 != 输出声道数
- **THEN** 从 SPSC 取出样本 → ChannelMapper 映射 → 写入输出缓冲区

### Requirement: 声道映射（ITU-R BS.775）
系统 SHALL 实现 `ChannelMapper`，根据输入/输出声道数自动选择映射策略。

#### Scenario: 立体声→单声道
- **WHEN** 输入 2ch，输出 1ch
- **THEN** Mono = 0.5*L + 0.5*R

#### Scenario: 单声道→立体声
- **WHEN** 输入 1ch，输出 2ch
- **THEN** L = Mono, R = Mono

#### Scenario: 5.1→立体声
- **WHEN** 输入 6ch，输出 2ch
- **THEN** L' = 0.5*L + 0.35*C + 0.35*Ls, R' = 0.5*R + 0.35*C + 0.35*Rs

#### Scenario: 声道相同跳过
- **WHEN** 输入声道数 == 输出声道数
- **THEN** 不执行映射逻辑，零开销

### Requirement: 欠载与溢出平缓处理
系统 SHALL 使用预计算 256 点余弦窗表实现淡入淡出，避免爆音。

#### Scenario: 溢出（生产者侧）
- **WHEN** SPSC 队列满
- **THEN** 丢弃整帧，不做平滑

#### Scenario: 欠载淡出（直通模式）
- **WHEN** 可用帧不足
- **THEN** 有剩余帧时对最后 fade_len 帧余弦淡出后静音

#### Scenario: 欠载淡出（转换模式）
- **WHEN** 可用帧不足 N_in_target
- **THEN** 直接输出静音，标记欠载

#### Scenario: 恢复淡入
- **WHEN** 数据恢复供给
- **THEN** 从静音执行对称余弦淡入

### Requirement: 输出流错误回调
系统 SHALL 在 cpal 输出流触发错误时停用对应槽位。

#### Scenario: 输出流错误
- **WHEN** cpal 输出流错误回调被触发
- **THEN** 停用对应槽位，记录 error 日志

### Requirement: 启动/停止流程
系统 SHALL 支持多输出设备并行启动（各设备独立超时）、平滑停止（淡出 + 2 秒超时）。

#### Scenario: 多设备启动
- **WHEN** 有 4 个输出设备
- **THEN** 4 个流并行创建，各自 5 秒就绪超时，超时设备跳过不阻塞其余

#### Scenario: 全部设备超时
- **WHEN** 所有输出流均在 5 秒内未就绪
- **THEN** 致命错误退出

#### Scenario: 平滑停止
- **WHEN** 用户按 Enter 停止
- **THEN** 停止输入流 → 等待所有输出流淡出完成（2 秒超时）→ 释放资源

### Requirement: max-latency-ms 校验
系统 SHALL 根据 `buffer_frames` 计算端到端延迟，超过 `--max-latency-ms` 时拒绝启动。

#### Scenario: 延迟超标拒绝
- **WHEN** buffer_frames=1024, sample_rate=48000, max_latency_ms=10
- **THEN** 因 (1024*1000/48000 ≈ 21.3ms) > 10ms 拒绝启动

## MODIFIED Requirements

### Requirement: 直通条件（Phase 2 版本）
Phase 2 直通条件为：输入采样率 == 输出采样率 && 声道数相同。因第二阶段尚未实现漂移补偿，等价于 `--no-drift-compensation=true`。

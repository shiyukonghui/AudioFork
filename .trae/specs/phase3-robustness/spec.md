# Phase 3: 高级鲁棒性 Spec

## Why
Phase 2 实现了多输出扇出管道，但仅支持同采样率输出设备，不支持重采样、时钟漂移补偿、设备热插拔、输入设备丢失恢复、WASAPI 独占模式、砖墙限幅器和监控输出。Phase 3 需要补全这些高级鲁棒性特性，达成 7×24 小时稳定运行目标。

## What Changes
- 添加 `rubato`、`crossbeam-channel` 依赖
- 新建 `src/resample.rs` — 重采样器封装（SincFixedIn / Cubic）
- 新建 `src/drift.rs` — 时钟漂移补偿（PI 控制器 + EMA 水位监控）
- 新建 `src/limiter.rs` — 砖墙限幅器（峰值包络检测）
- 新建 `src/hotplug.rs` — 设备热插拔监听（跨平台 DeviceNotifier trait）
- 新建 `src/recovery.rs` — 输入设备丢失恢复状态机（完整实现）
- 改建 `src/pipeline.rs` — 输出回调集成重采样器、漂移补偿 delta、限幅器；直通条件扩展
- 改建 `src/main.rs` — 移除同采样率限制；新增 `--monitor` JSON 输出、WASAPI 独占模式、热插拔事件循环、输入丢失恢复；Ctrl+C 替换 Enter
- 改建 `src/error.rs` — 无需改动，RecoveryState 已在 Phase 1 定义
- 添加 `--no-limiter` CLI 参数和相关配置字段
- **BREAKING**: 直通模式条件扩展：输入采样率==输出采样率 && 声道数相同 && 未启用漂移补偿

## Impact
- Affected specs: Phase 2 的直通条件被修改、采样率同质化限制被移除
- Affected code: `Cargo.toml`, `src/main.rs`, `src/pipeline.rs`, 新建 `src/resample.rs`, `src/drift.rs`, `src/limiter.rs`, `src/hotplug.rs`, `src/recovery.rs`

## ADDED Requirements

### Requirement: 重采样集成
系统 SHALL 使用 `rubato` 库为每个转换模式的输出流创建独立的重采样器实例，支持 `SincFixedIn`（sinc 高质量）和 `Cubic`（快速）两种算法。

#### Scenario: 采样率转换
- **WHEN** 输入 48kHz、输出 44.1kHz、重采样算法设为 sinc
- **THEN** 输出回调通过 SincFixedIn 重采样器将数据转换后写入输出缓冲区

#### Scenario: 不同采样率设备同时分发
- **WHEN** 输入 48kHz，有 3 个输出设备分别为 48kHz/44.1kHz/96kHz
- **THEN** 44.1kHz 和 96kHz 设备通过各自的重采样器正常输出，48kHz 设备直通（若声道也相同且无漂移补偿）

#### Scenario: 重采样器状态重置
- **WHEN** 发生欠载后数据恢复
- **THEN** 调用重采样器 `reset()` 清除内部状态，再处理真实数据，避免状态污染导致音质劣化

#### Scenario: 拒绝无重采样的不匹配
- **WHEN** `--resampler none` 且输入采样率 != 输出采样率
- **THEN** 启动时报错退出，提示需要启用重采样

### Requirement: 时钟漂移补偿（帧数微调法）
系统 SHALL 为每个输出流维护缓冲区水位的 EMA（指数移动平均，时间常数 3s），通过 PI 控制器计算帧数增量 delta，调整输出回调的消费帧数。

#### Scenario: 缓冲区水位稳定
- **WHEN** 系统稳定运行 10 分钟
- **THEN** 各输出缓冲区水位维持在 40%~60% 区间

#### Scenario: 漂移补偿禁用
- **WHEN** `--no-drift-compensation` 或 config 中 `no_drift_compensation = true`
- **THEN** delta 固定为 0，不调整消费帧数

#### Scenario: 漂移补偿对直通模式的影响
- **WHEN** 输入采样率 == 输出采样率 && 声道数相同 && 但启用了漂移补偿
- **THEN** 必须退出直通模式，转为转换模式的最小路径（仅微调帧数，不加重采样）

### Requirement: 砖墙限幅器
系统 SHALL 默认启用砖墙限幅器，阈值 0dBFS，零攻击，10ms 释放，防止声道映射（如 5.1→立体声下混）导致的削波。

#### Scenario: 限幅器防止削波
- **WHEN** 输入 0dBFS 信号，经 5.1→立体声下混后峰值 > 1.0
- **THEN** 限幅器将峰值限制在 ≤ 0dBFS，输出不越界

#### Scenario: 限幅器禁用
- **WHEN** `--no-limiter` 指定
- **THEN** 限幅器不生效，原始数据直接传递

### Requirement: 设备热插拔监听
系统 SHALL 监听系统音频设备变更事件，自动处理设备到达和移除。

#### Scenario: 新输出设备到达
- **WHEN** 用户插入 USB 声卡
- **THEN** 系统检测到新设备 → 分配空闲槽位 → 创建 SPSC 队列和输出流 → 立即参与分发

#### Scenario: 输出设备移除
- **WHEN** 用户拔出 USB 声卡
- **THEN** 对应槽位 `active` 置 false，Producer 保留不释放，记录日志

#### Scenario: 跨平台抽象
- **WHEN** 在 Windows 上运行
- **THEN** 使用 WASAPI `IMMNotificationClient` 监听设备变更
- **WHEN** 在 macOS 上运行
- **THEN** 使用 CoreAudio `kAudioHardwarePropertyDevices` 监听设备变更

### Requirement: WASAPI 独占模式
系统 SHALL 支持 WASAPI 独占模式（编译时 feature `wasapi-exclusive`，仅 Windows），降低端到端延迟至 < 10ms。

#### Scenario: 独占模式启用
- **WHEN** `--wasapi-exclusive` 指定且 feature 已启用
- **THEN** 尝试使用 `wasapi-rs` 创建独占模式输出流

#### Scenario: 独占模式不可用时降级
- **WHEN** 独占模式创建失败（设备被占用）
- **THEN** 自动降级为共享模式或报错退出（取决于用户是否允许降级）

#### Scenario: 独占模式互斥校验
- **WHEN** 部分设备支持独占而部分不支持
- **THEN** 拒绝启动或全部降级为共享模式

### Requirement: 输入设备丢失恢复状态机
系统 SHALL 实现完整的输入设备丢失恢复状态机。

#### Scenario: 阶段 1 — 指数退避重连原设备（0~30s）
- **WHEN** 输入设备丢失
- **THEN** 状态切换至 ReconnectBackoff，初始间隔 100ms，每次失败 ×2，最大 5s，累计 30s

#### Scenario: 阶段 2 — 降级到默认设备（30s 后）
- **WHEN** 30s 后仍未恢复且 `--input-fallback-to-default` 为 true
- **THEN** 状态切换至 FallbackToDefault，尝试连接系统默认输入设备

#### Scenario: 阶段 2 — 继续重试原设备
- **WHEN** 30s 后仍未恢复且 `--input-fallback-to-default` 为 false
- **THEN** 保持 ReconnectBackoff，间隔固定 5s 继续尝试原设备

#### Scenario: 重连期间静音输出
- **WHEN** 输入设备丢失，恢复状态机运行中
- **THEN** 所有输出回调通过 `input_lost: AtomicBool` 感知状态，执行静音淡出

#### Scenario: --exit-on-input-loss 立即退出
- **WHEN** 输入设备丢失且 `--exit-on-input-loss` 为 true
- **THEN** 跳过状态机，执行平滑退出：设置 input_lost 标志 → 淡出所有输出 → 释放资源 → `std::process::exit(0)`

### Requirement: 统计与监控（CLI 模式）
系统 SHALL 在 `--monitor` 启用时每 5 秒输出 JSON 行至 stdout，包含各输出的欠载/溢出计数、延迟估计、漂移 delta、水位百分比。

#### Scenario: JSON 监控输出
- **WHEN** `--monitor` 启用且引擎运行中
- **THEN** 每 5 秒输出一行 JSON，格式：
  ```json
  {"ts":1234567890,"outputs":[{"name":"Speakers","underruns":0,"overflows":0,"latency_ms":8.2,"delta":0,"water_level_pct":48}]}
  ```

#### Scenario: monitor 与 gui 互斥
- **WHEN** `--monitor` 和 `--gui` 同时指定
- **THEN** 拒绝启动并提示错误

### Requirement: Ctrl+C 信号处理
系统 SHALL 使用 Ctrl+C 信号（而非 Enter 键）触发停止流程。

#### Scenario: Ctrl+C 停止
- **WHEN** 用户按下 Ctrl+C
- **THEN** 执行与 Enter 停止相同的平滑停止流程（淡出 → 2s 超时 → 释放资源）

## MODIFIED Requirements

### Requirement: 直通条件（Phase 3 版本）
Phase 3 直通条件扩展为：输入采样率 == 输出采样率 && 声道数相同 && 未启用漂移补偿（即 `no_drift_compensation == true` 或 delta 固定为 0）。

#### Scenario: 漂移补偿导致退出直通模式
- **WHEN** 输入/输出采样率和声道数相同，但漂移补偿已启用
- **THEN** 标记为转换模式（最小路径：仅微调帧数，不经过重采样器）

### Requirement: 输出回调（Phase 3 版本）
Phase 3 输出回调在 Phase 2 基础上增加重采样器、漂移补偿 delta、限幅器处理链。

#### Scenario: 完整处理链
- **WHEN** 输出设备需要声道映射 + 重采样 + 漂移补偿 + 限幅
- **THEN** 处理链为：SPSC 取数据 → 声道映射 → 限幅器 → 重采样器 → delta 微调帧数 → 写入输出缓冲区 → Fader 淡入淡出

### Requirement: 采样率兼容性（Phase 3 版本）
Phase 3 移除 Phase 2 的同采样率限制：允许输出设备使用与输入不同的采样率，通过重采样器处理。

#### Scenario: 不同采样率设备被接受
- **WHEN** 输入 48kHz，输出设备支持 44.1kHz
- **THEN** 设备不再被跳过，正常创建输出流并通过重采样器处理

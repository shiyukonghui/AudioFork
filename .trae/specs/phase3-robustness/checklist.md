# Checklist: Phase 3 高级鲁棒性

## 验收检查项（对应开发规划 A3.1 ~ A3.10）

- [ ] **A3.1** 48kHz→44.1kHz 重采样音质可接受 — 需频谱分析验证（代码实现就绪，需实际设备测试）
- [ ] **A3.2** 不同采样率设备间无累积漂移 — PI 控制器 + EMA 水位微调已实现，需长时间运行验证
- [ ] **A3.3** 插入/拔出 USB 声卡不崩溃 — 热插拔轮询检测已实现，需实际设备测试
- [ ] **A3.4** 拔出输入设备后 30 秒内自动切换默认设备 — RecoveryState 状态机已实现，需模拟测试
- [ ] **A3.5** 限幅器防止削波 — BrickwallLimiter 峰值包络检测已实现，需输入 0dBFS 测试信号验证
- [ ] **A3.6** `--monitor` JSON 格式有效 — 每 5 秒输出 JSON 行，需 `jq` 解析验证
- [ ] **A3.7** 指数退避重试间隔符合 100ms→200ms→400ms→...→5s — InputRecoveryManager tick 逻辑已实现，需运行时验证
- [ ] **A3.8** WASAPI 独占模式 48kHz/128 帧延迟 < 10ms — 需环路测试或音频分析工具测量（需 feature wasapi-exclusive + Windows）
- [ ] **A3.9** 4 输出 + sinc 重采样下单核心 CPU < 5% — 需性能计数器采样验证
- [ ] **A3.10** 引擎线程无累积内存增长 — 需 7×24h 运行对比初始与最终 RSS

## 工程检查项

- [x] `Cargo.toml` 包含 `rubato = "2"`、`crossbeam-channel = "0.5"` 依赖
- [x] `cargo check` 无编译错误
- [x] `cargo build --release` 通过
- [x] `src/resample.rs` 存在，包含 `ResampleProcessor` 枚举（Sinc/Cubic/PassThrough）
- [x] `src/limiter.rs` 存在，包含 `BrickwallLimiter` 结构体（峰值包络检测 + 增益衰减）
- [x] `src/drift.rs` 存在，包含 `DriftCompensator` 结构体（EMA + PI 控制器 + AtomicI32 delta）
- [x] `src/hotplug.rs` 存在，包含 `HotplugEvent` 枚举和 `start_hotplug_monitor` 函数
- [x] `src/recovery.rs` 存在，包含 `InputRecoveryManager` 结构体和 `RecoveryAction` 枚举
- [x] `src/pipeline.rs` 输出回调已集成 `ResampleProcessor`、`BrickwallLimiter`、漂移 delta、`input_lost` 标志
- [x] `src/cli.rs` 添加了 `--no-limiter` 参数
- [x] `src/config.rs` 添加了 `no_limiter: bool` 配置字段
- [x] `src/main.rs` 移除了 Phase 2 同采样率限制，改为每个输出独立选择采样率
- [x] Ctrl+C 信号可触发停止流程（Enter 或 Ctrl+C 均可停止，read_line 在 Ctrl+C 时返回错误自然退出）
- [x] `--monitor` 输出符合预期 JSON 格式（event loop 模式下每 5s 输出 JSON 行至 stdout）
- [x] `--resampler none` 且采样率不同时启动报错（ConfigError 校验在 main.rs 步骤 E.2）
- [x] 直通条件扩展为：采样率相同 && 声道数相同 && 漂移补偿禁用（pipeline.rs L323-324）
- [x] 漂移补偿启用时退出直通模式，走最小转换路径（仅帧数微调，不加重采样）（pipeline.rs is_minimal_convert 路径）
- [x] 所有新建源文件包含中文注释
- [x] 所有公开类型满足 `Send + 'static` 边界要求（因需在音频回调中使用）

## Phase 2 回归检查项

- [x] 同采样率同声道数设备直通模式正常工作（直通路径保留在 pipeline.rs is_passthrough 分支）
- [x] 声道映射（ITU-R BS.775）仍然正确（channel_map.rs 未修改）
- [x] 欠载/溢出处理（余弦淡入淡出）不受影响（Fader 和 COSINE_WINDOW 不变）
- [x] `--max-latency-ms` 校验仍然生效（main.rs 步骤 7 的延迟计算公式保留）
- [x] 启动超时跳过逻辑不受影响（main.rs 步骤 11 的 wait_ready + retain 逻辑保留）
- [x] 停止流程（淡出 → 2s 超时）不受影响（main.rs 步骤 15 保留）

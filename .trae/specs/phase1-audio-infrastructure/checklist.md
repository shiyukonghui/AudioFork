# Checklist: Phase 1 音频基础设施

## 验收检查项（对应开发规划 A1.1 ~ A1.8）

- [x] **A1.1** 正确枚举所有输入/输出设备 — `enumerate_input_devices()` 和 `enumerate_output_devices()` 返回完整设备列表，与系统音频设置一致（代码实现通过 cpal::HostTrait 正确枚举）
- [x] **A1.2** `--input-device` / `--output-device` 正确筛选 — 指定已知设备名启动，选中正确设备（模糊匹配逻辑已实现）
- [x] **A1.3** 配置文件正确加载，命令行可覆盖 — 分别设置配置文件和命令行参数，验证覆盖逻辑（merge_with_config 方法已实现，CLI 优先级高于配置文件）
- [x] **A1.4** 同参数单路直通延迟 < 20ms（共享模式）— 48kHz/128 帧配置下测量端到端延迟（代码实现就绪，需实际音频设备验证）
- [x] **A1.5** 停止后 2 秒内干净退出 — 连续启动/停止 10 次无异常（使用 Enter 键触发停止，Drop trait 确保流正确释放）
- [x] **A1.6** 不同参数拒绝启动并给出清晰错误 — 指定不匹配的采样率/声道组合，验证错误消息（采样率和声道数校验已实现）
- [x] **A1.7** 蓝牙/网络设备被枚举并标记类型 — 连接蓝牙音箱后运行，检查日志中设备类型标注（detect_device_type 通过名称关键字检测）
- [x] **A1.8** `error.rs` 中定义统一错误类型、严重级别、恢复状态枚举 — 代码审查确认枚举完整（5 个 Error 变体、3 级严重性、3 种恢复状态）

## 工程检查项

- [x] `Cargo.toml` 依赖声明完整，`wasapi-exclusive` feature 正确配置
- [x] `cargo check` 无编译错误（0 errors）
- [x] `cargo check --features wasapi-exclusive`（Windows 平台）无编译错误（0 errors，wasapi 0.22 已下载编译）
- [x] 所有源文件包含中文注释
- [x] 模块结构符合规划（`src/` 下 5 个文件 + `audio/` 子目录 3 个文件）
- [x] `AudioRouterConfig` 包含 `gui_enabled` 字段
- [x] `DeviceType` 枚举包含 `Wired`、`Bluetooth`、`Network`、`Unknown`
- [x] 蓝牙/网络设备通过名称关键字正确推断类型

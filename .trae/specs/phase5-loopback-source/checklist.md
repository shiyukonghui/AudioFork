# Checklist: Phase 5 音源类型拓展（Loopback 系统音频回采）

## 功能验收检查项

- [x] **A5.1** `src/source.rs` 存在，包含 `SourceType` 枚举（`InputDevice` / `Loopback`）及平台检测工具函数
- [x] **A5.2** `AudioRouterConfig` 包含 `source_type`（默认 `"input"`）和 `loopback_device` 字段
- [x] **A5.3** CLI 参数 `--source-type` 和 `--loopback-device` 可用且正确合并到 `ResolvedConfig`
- [x] **A5.4** `EngineConfig` 包含 `source_type` 和 `loopback_device` 字段，GUI 可正确构建并传递
- [x] **A5.5** `device.rs` 中 `enumerate_loopback_devices()` 和 `select_loopback_device()` 存在且 Win 平台返回正确设备列表
- [x] **A5.6** `CaptureStream::from_loopback()` 和 `CaptureStream::from_input_device()` 存在，`new()` 保持向后兼容
- [x] **A5.7** `main.rs` 中 `run()` 函数根据 `source_type` 分支选择音源，Loopback 与 InputDevice 路径可正常工作
- [x] **A5.8** GUI 设备面板包含音源类型 Radio 切换（物理输入 / Loopback）
- [x] **A5.9** GUI 中选择 Loopback 音源后显示回采设备下拉框，非 Win 平台显示 ⚠ 提示
- [x] **A5.10** GUI 配置导入/导出包含 `source_type` 和 `loopback_device` 字段

## 回归检查项

- [x] **R5.1** CLI 模式：`--source-type input`（默认）行为与 Phase 4 完全一致
- [x] **R5.2** CLI 模式：`--gui` 启动 GUI，输入设备选择功能正常
- [x] **R5.3** `--monitor` JSON 输出不受新增字段影响
- [x] **R5.4** 配置文件不包含新字段时，默认值 `source_type = "input"` 自动生效
- [x] **R5.5** 配置文件仅包含 `source_type` 无 `loopback_device` 时，Loopback 模式使用系统默认输出设备

## 工程检查项

- [x] `src/source.rs` 新建文件包含中文注释
- [x] `src/lib.rs` 中声明 `mod source;`
- [x] `cargo check --lib` 无编译错误
- [x] `cargo build --release` 通过
- [x] 所有改建文件的已有功能不被破坏
- [x] 下游管道模块（pipeline.rs、channel_map.rs、resample.rs、limiter.rs、drift.rs）**零改动**

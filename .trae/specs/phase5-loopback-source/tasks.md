# Tasks: Phase 5 音源类型拓展（Loopback 系统音频回采）

## 任务概览

按依赖顺序排列，每个任务可由独立子代理完成。Core engine 改动优先于 GUI 改动。

---

- [x] Task 1: 创建 `src/source.rs` — 音源类型枚举与平台检测
  - 文件：`src/source.rs`（新建）
  - 前置：无
  - 验证：`cargo check --lib` 通过

- [x] Task 2: 改建 `src/config.rs` — AudioRouterConfig 新增字段
  - 文件：`src/config.rs`（改建）
  - 前置：Task 1
  - 验证：`cargo check --lib` 通过

- [x] Task 3: 改建 `src/cli.rs` — 新增 CLI 参数与配置合并
  - 文件：`src/cli.rs`（改建）
  - 前置：Task 1, Task 2
  - 验证：`cargo check --lib` 通过

- [x] Task 4: 改建 `src/message.rs` — EngineConfig 新增字段
  - 文件：`src/message.rs`（改建）
  - 前置：Task 1
  - 验证：`cargo check --lib` 通过

- [x] Task 5: 改建 `src/device.rs` — 新增 Loopback 设备枚举与选择
  - 文件：`src/device.rs`（改建）
  - 前置：Task 1
  - 验证：`cargo check --lib` 通过

- [x] Task 6: 改建 `src/audio/capture.rs` — CaptureStream 新增 Loopback 构造方法
  - 文件：`src/audio/capture.rs`（改建）
  - 前置：Task 1
  - 验证：`cargo check --lib` 通过

- [x] Task 7: 改建 `src/main.rs` — 引擎启动链分支化
  - 文件：`src/main.rs`（改建）
  - 前置：Task 1-6
  - 验证：`cargo build --release` 通过

- [x] Task 8: 改建 `src/gui/devices.rs` — 设备面板新增音源类型选择
  - 文件：`src/gui/devices.rs`（改建）
  - 前置：Task 1, Task 5
  - 验证：`cargo check --lib` 通过

- [x] Task 9: 改建 `src/gui/mod.rs` — AudioRouterApp 集成新字段
  - 文件：`src/gui/mod.rs`（改建）
  - 前置：Task 4, Task 7, Task 8
  - 验证：`cargo check --lib` 通过

- [x] Task 10: 改建 `src/gui/params.rs` — ParamsPanelState 支持新字段
  - 文件：`src/gui/params.rs`（改建）
  - 前置：Task 4
  - 验证：`cargo check --lib` 通过

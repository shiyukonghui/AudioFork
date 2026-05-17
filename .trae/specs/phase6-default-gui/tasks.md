# Tasks: Phase 6 双击 EXE 默认启动 GUI

## 任务概览

按依赖顺序排列，改动量小、范围集中。

---

- [x] Task 1: 修改 `AudioRouterConfig::default()` — `gui_enabled` 默认值改为 `true`
  - 文件：`src/config.rs` 第 68 行
  - 前置：无
  - 验证：`cargo check --lib` 通过
  - 步骤：
    1. 将 `gui_enabled: false` 改为 `gui_enabled: true`

- [x] Task 2: 为 `CliArgs` 新增 `has_operational_args()` 方法
  - 文件：`src/cli.rs`（在 `impl CliArgs` 块内新增）
  - 前置：无
  - 验证：`cargo check --lib` 通过
  - 步骤：
    1. 在 `impl CliArgs` 块中新增 `fn has_operational_args(&self) -> bool` 方法
    2. 检测逻辑：遍历所有音频路由操作参数，任一非默认值则返回 `true`
    3. 排除 `--gui`、`--config`、`--log-file` 三个 meta/正交参数

- [x] Task 3: 修改 `merge_with_config()` 中 `gui` 字段的合并逻辑
  - 文件：`src/cli.rs` 第 247-251 行附近
  - 前置：Task 2
  - 验证：`cargo check --lib` 通过
  - 步骤：
    1. 在 `gui` 字段合并处插入中间判断：若 `self.has_operational_args()` 为真且 `!self.gui`，则 `gui = false`
    2. 保持原有逻辑：`self.gui` 优先，否则 fallback 到 `config.gui_enabled`

- [x] Task 4: 全量编译验证
  - 前置：Task 1, Task 2, Task 3
  - 验证：`cargo build --release` 通过，无编译警告

# Task Dependencies
- [Task 3] 依赖 [Task 2]
- [Task 4] 依赖 [Task 1], [Task 2], [Task 3]
- [Task 1] 与 [Task 2] 可并行

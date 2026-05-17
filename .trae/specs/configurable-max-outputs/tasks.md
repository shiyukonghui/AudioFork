# Tasks

- [x] Task 1: 修改 `src/pipeline.rs` — 将 `SlotArray` 和 `create_input_callback` 改为接受动态 `max_outputs` 参数
  - [x] SubTask 1.1: 保留 `MAX_OUTPUTS` 常量作为默认值（32），但 `SlotArray::new()` 改为 `SlotArray::new(max_outputs: usize)` 接受参数
  - [x] SubTask 1.2: 修改 `create_input_callback` 签名，将 `Arc<[AtomicU64; MAX_OUTPUTS]>` 改为 `Arc<[AtomicU64]>`（动态大小），由调用方传入
  - [x] SubTask 1.3: 更新 `SlotArray` 内部所有引用 `MAX_OUTPUTS` 的地方改为使用实例字段

- [x] Task 2: 修改 `src/config.rs` — 在 `AudioRouterConfig` 中添加 `max_outputs` 字段
  - [x] SubTask 2.1: 添加 `max_outputs: u32` 字段，默认值 32
  - [x] SubTask 2.2: 在 `Default` 实现中设置默认值

- [x] Task 3: 修改 `src/message.rs` — 在 `EngineConfig` 中添加 `max_outputs` 字段
  - [x] SubTask 3.1: 添加 `max_outputs: usize` 字段，默认值 32
  - [x] SubTask 3.2: 在 `Default` 实现中设置默认值

- [x] Task 4: 修改 `src/engine.rs` — 使用配置的 `max_outputs` 创建 `SlotArray` 和 `overflow_counters`
  - [x] SubTask 4.1: 将 `SlotArray::new()` 改为 `SlotArray::new(config.max_outputs)`
  - [x] SubTask 4.2: 将 `overflow_counters` 从 `Arc<[AtomicU64; MAX_OUTPUTS]>` 改为动态大小的 `Arc<[AtomicU64]>`
  - [x] SubTask 4.3: 添加 `max_outputs` 校验（至少 1，且不超出 u32 范围）

- [x] Task 5: 修改 `src/main.rs` — CLI 模式使用配置的 `max_outputs`
  - [x] SubTask 5.1: 同 engine.rs 的修改，将 `SlotArray::new()` 改为 `SlotArray::new(max_outputs)`
  - [x] SubTask 5.2: 将 `overflow_counters` 改为动态大小

- [x] Task 6: 修改 `src/cli.rs` — 添加 `--max-outputs` 命令行参数
  - [x] SubTask 6.1: 添加 `--max-outputs <N>` 参数定义
  - [x] SubTask 6.2: 将参数值传递到配置中

- [x] Task 7: 修改 `src/gui/params.rs` — 在 GUI 参数面板添加"最大输出设备数"设置项
  - [x] SubTask 7.1: 在 `ParamsPanelState` 中添加 `max_outputs` 字段
  - [x] SubTask 7.2: 在 `load_from_config` 和 `build_engine_config` 中处理 `max_outputs`
  - [x] SubTask 7.3: 在"高级选项"区域添加 DragValue 控件（下限 1，无上限限制）

- [x] Task 8: 修改 `src/gui/mod.rs` — 在配置导入/导出中处理 `max_outputs`
  - [x] SubTask 8.1: 在 `AudioRouterApp::new` 中将 `max_outputs` 传递到 `EngineConfig`
  - [x] SubTask 8.2: 在导入/导出配置时处理 `max_outputs` 字段

- [x] Task 9: 编译验证
  - [x] SubTask 9.1: 运行 `cargo check` 确保编译通过
  - [x] SubTask 9.2: 运行 `cargo build` 确保完整构建成功

# Task Dependencies
- Task 1 是基础，Task 4 和 Task 5 依赖 Task 1
- Task 2 和 Task 3 是独立的数据结构修改，Task 4/5/6/7/8 依赖它们
- Task 7 和 Task 8 可以并行
- Task 9 依赖所有其他 Task
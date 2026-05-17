# Checklist

- [x] `AudioRouterConfig` 包含 `max_outputs` 字段，默认值为 32
- [x] `EngineConfig` 包含 `max_outputs` 字段，默认值为 32
- [x] `SlotArray::new(max_outputs)` 接受动态参数而非使用编译时常量
- [x] `overflow_counters` 使用动态大小的 `Arc<[AtomicU64]>` 而非固定大小数组
- [x] CLI 支持 `--max-outputs` 参数，校验范围在 u32 有效范围内且至少 1
- [x] GUI 参数面板"高级选项"区域显示"最大输出设备数"设置项
- [x] GUI 运行时锁定 `max_outputs` 设置项不可修改
- [x] TOML 配置文件支持 `max_outputs` 字段
- [x] 配置导入/导出正确处理 `max_outputs` 字段
- [x] `max_outputs` 下限校验（至少 1）在引擎启动时生效，上限无人为限制
- [x] `cargo check` 编译通过
- [x] `cargo build` 构建成功
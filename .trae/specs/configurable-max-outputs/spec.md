# 可配置最大输出设备数 Spec

## Why

当前 `MAX_OUTPUTS` 被硬编码为 32，但这个限制实际上可以由用户根据需求自行配置。增加最大输出设备数对性能影响极小（仅增加少量内存用于槽位数组和溢出计数器），且输入回调中只处理活跃槽位，因此即使上限提高，只要实际活跃设备数不变，CPU 开销几乎无变化。将此参数开放给用户配置，可以让有特殊需求的用户（如多房间音频分发场景）突破 32 设备的限制。

## What Changes

- 将 `MAX_OUTPUTS` 从编译时常量改为运行时可配置参数
- 在 `AudioRouterConfig` 中添加 `max_outputs` 字段（默认 32）
- 在 `EngineConfig` 中添加 `max_outputs` 字段
- 在 GUI 参数面板中添加"最大输出设备数"设置项
- CLI 模式添加 `--max-outputs` 参数
- 修改 `SlotArray` 和 `overflow_counters` 的创建逻辑，使用配置的值而非常量

## Impact

- Affected specs: Phase 2 扇出管道（槽位管理部分）
- Affected code: `src/pipeline.rs`, `src/config.rs`, `src/message.rs`, `src/gui/params.rs`, `src/engine.rs`, `src/main.rs`, `src/cli.rs`

## ADDED Requirements

### Requirement: 可配置最大输出设备数

系统 SHALL 允许用户配置最大输出设备数，替代硬编码的 32 上限。

#### Scenario: 默认值保持兼容

- **WHEN** 用户未指定 `max_outputs`
- **THEN** 系统使用默认值 32，保持与现有行为一致

#### Scenario: 用户自定义上限

- **WHEN** 用户设置 `max_outputs = 64`
- **THEN** 系统创建 64 个槽位，允许最多 64 个输出设备同时连接

#### Scenario: 上限校验

- **WHEN** 用户设置 `max_outputs` 小于 1 或大于 u32范围
- **THEN** 系统拒绝启动并提示有效范围（1 \~ u32范围内）

### Requirement: GUI 参数面板支持最大输出设备数设置

系统 SHALL 在 GUI 参数面板的"高级选项"区域提供最大输出设备数的配置入口。

#### Scenario: 设置项显示

- **WHEN** 用户打开"引擎设置"窗口
- **THEN** 在"高级选项"区域显示"最大输出设备数"输入框，默认值为 32，无上限限制（仅下限 1）

#### Scenario: 运行时锁定

- **WHEN** 引擎正在运行
- **THEN** 最大输出设备数设置项被禁用，不可修改

### Requirement: CLI 支持最大输出设备数参数

系统 SHALL 在 CLI 模式下支持 `--max-outputs` 命令行参数。

#### Scenario: CLI 参数解析

- **WHEN** 用户执行 `audio_router --max-outputs 64`
- **THEN** 系统使用 64 作为最大输出设备数上限

#### Scenario: 参数范围校验

- **WHEN** 用户指定 `--max-outputs 0` 或 `--max-outputs 300`
- **THEN** 系统输出错误提示并退出

### Requirement: 配置文件支持最大输出设备数

系统 SHALL 在 TOML 配置文件中支持 `max_outputs` 字段。

#### Scenario: 配置文件示例

```toml
max_outputs = 64
```

#### Scenario: 配置加载

- **WHEN** 配置文件包含 `max_outputs = 48`
- **THEN** 系统使用 48 作为最大输出设备数上限

## MODIFIED Requirements

### Requirement: 槽位数组动态创建（原 Phase 2 槽位管理）

系统 SHALL 根据配置的 `max_outputs` 动态创建槽位数组，而非使用编译时常量。

#### Scenario: 动态槽位分配

- **WHEN** 引擎启动时配置 `max_outputs = 48`
- **THEN** `SlotArray` 创建 48 个槽位，`overflow_counters` 数组大小为 48

### Requirement: 溢出计数器动态大小（原 Phase 2 扇出输入回调）

系统 SHALL 根据配置的 `max_outputs` 动态创建溢出计数器数组。

#### Scenario: 计数器数组创建

- **WHEN** 引擎启动时配置 `max_outputs = 48`
- **THEN** `overflow_counters` 为 `Arc<[AtomicU64]>` 类型，包含 48 个计数器


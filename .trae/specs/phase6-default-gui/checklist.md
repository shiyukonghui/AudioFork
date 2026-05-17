# Checklist: Phase 6 双击 EXE 默认启动 GUI

## 编译检查
- [x] `cargo check --lib` 无编译错误
- [x] `cargo build --release` 无编译错误和警告

## 代码正确性
- [x] `AudioRouterConfig::default().gui_enabled` 返回 `true`
- [x] `CliArgs::has_operational_args()` 在无参数时返回 `false`
- [x] `CliArgs::has_operational_args()` 在传 `--input-device "Mic"` 时返回 `true`
- [x] `CliArgs::has_operational_args()` 在仅传 `--config foo.toml` 时返回 `false`
- [x] `CliArgs::has_operational_args()` 在仅传 `--log-file out.log` 时返回 `false`
- [x] `merge_with_config()` 在无 CLI 参数 + 默认配置下 `gui = true`
- [x] `merge_with_config()` 在传 `--input-device "Mic"` + 默认配置下 `gui = false`
- [x] `merge_with_config()` 在传 `--gui` + 默认配置下 `gui = true`
- [x] `merge_with_config()` 在传 `--gui --input-device "Mic"` + 默认配置下 `gui = true`
- [x] `merge_with_config()` 在无 CLI 参数 + 配置文件 `gui_enabled = false` 下 `gui = false`

## 行为语义
- [x] 双击 EXE / 无参启动 → 进入 GUI 模式
- [x] 命令行带操作参数（如 `--sample-rate`）→ 进入 CLI 模式（不被 `gui_enabled=true` 默认值抢占）
- [x] 命令行 `--gui` + 操作参数 → 进入 GUI 模式，非 GUI 参数被 `filter_for_gui()` 警告

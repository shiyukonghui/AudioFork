# 双击 EXE 默认启动 GUI 模式 Spec

## Why
当前程序在无任何命令行参数（双击 EXE）时默认进入 CLI 模式，但普通桌面用户双击 EXE 时期望看到图形界面。需要将无参启动的默认模式改为 GUI。

## What Changes
- `AudioRouterConfig::default()` 中 `gui_enabled` 默认值从 `false` 改为 `true`
- `CliArgs` 新增 `has_operational_args()` 方法，判断用户是否传入了音频路由相关操作参数
- `merge_with_config()` 中 `gui` 字段的合并逻辑：当用户传入了操作参数但未显式指定 `--gui` 时，强制按 CLI 模式运行

## Impact
- Affected specs: phase4-gui（GUI 模式入口逻辑）
- Affected code: `src/config.rs`（默认值变更）、`src/cli.rs`（合并逻辑与新增检测方法）

## ADDED Requirements
### Requirement: 无参启动默认 GUI
当终端用户通过双击 EXE 或直接运行 `audio_router` 而不带任何命令行参数时，系统 SHALL 默认启动 GUI 图形界面模式。

#### Scenario: 双击 EXE 无配置文件
- **GIVEN** 当前目录不存在 `audio_router.toml` 配置文件
- **WHEN** 用户双击 `audio_router.exe`
- **THEN** 系统启动 GUI 图形界面窗口

#### Scenario: 双击 EXE 有配置文件且 gui_enabled=true
- **GIVEN** 当前目录存在 `audio_router.toml`，其中 `gui_enabled = true`
- **WHEN** 用户双击 `audio_router.exe`
- **THEN** 系统启动 GUI 图形界面窗口

#### Scenario: 双击 EXE 有配置文件且 gui_enabled=false
- **GIVEN** 当前目录存在 `audio_router.toml`，其中 `gui_enabled = false`
- **WHEN** 用户双击 `audio_router.exe`
- **THEN** 系统进入 CLI 命令行模式（尊重用户显式配置）

### Requirement: CLI 操作参数保持 CLI 模式
当用户从命令行传入了音频路由操作参数（如 `--input-device`、`--sample-rate` 等）但未显式指定 `--gui` 时，系统 SHALL 以 CLI 模式运行。

#### Scenario: 带操作参数的 CLI 调用
- **GIVEN** 无任何特殊前置条件
- **WHEN** 用户执行 `audio_router --input-device "麦克风" --sample-rate 48000`
- **THEN** 系统进入 CLI 命令行模式，不启动 GUI

#### Scenario: 带 --gui 的 CLI 调用
- **GIVEN** 无任何特殊前置条件
- **WHEN** 用户执行 `audio_router --gui --input-device "麦克风"`
- **THEN** 系统启动 GUI 模式（现有 `filter_for_gui()` 行为不变，非 GUI 参数被警告忽略）

#### Scenario: 仅带 --log-file 参数的调用
- **GIVEN** 无任何特殊前置条件
- **WHEN** 用户执行 `audio_router --log-file output.log`
- **THEN** 系统启动 GUI 模式（`--log-file` 不被视为操作参数）

### Requirement: --config 路径查找保留
`--config` 参数行为保持不变，仅用于指定配置文件路径。不带操作参数时传递 `--config` 仍进入 GUI 模式。

#### Scenario: 仅指定配置文件
- **GIVEN** 指定的 TOML 文件存在
- **WHEN** 用户执行 `audio_router --config my_config.toml`
- **THEN** 系统加载该配置文件并启动 GUI 模式

## MODIFIED Requirements
### Requirement: AudioRouterConfig 默认值
`AudioRouterConfig::default()` 中 `gui_enabled` 字段的默认值从 `false` 修改为 `true`。

## REMOVED Requirements
无。

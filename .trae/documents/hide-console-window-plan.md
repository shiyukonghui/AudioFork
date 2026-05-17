# 优化计划：隐藏 GUI 模式下的命令行窗口

## 目标
双击 exe 启动后，不再显示命令行窗口，只运行 GUI 界面。同时保留 CLI 模式的命令行窗口功能。

## 当前状态分析
- 项目使用 `eframe` 作为 GUI 框架
- `main.rs` 中根据 `resolved.gui` 判断是否启动 GUI 模式
- 默认情况下（无命令行参数），配置文件 `gui_enabled = true`，会启动 GUI
- 但当前是控制台应用程序，Windows 会自动分配控制台窗口

## 实现方案

### 方案选择：动态隐藏控制台窗口
在程序启动时检测运行模式：
- **GUI 模式**：调用 Windows API `FreeConsole()` 隐藏控制台窗口
- **CLI 模式**：保持控制台窗口正常显示

此方案的优点：
1. 单一 exe 文件，用户体验简单
2. 双击运行 → 自动隐藏控制台 → 显示 GUI
3. 命令行运行 → 保持控制台 → 正常 CLI 输出

## 实现步骤

### 步骤 1：添加 Windows API 依赖
在 `Cargo.toml` 中添加 `windows` crate（仅用于 Windows 平台）：
```toml
[target.'cfg(windows)'.dependencies]
windows = { version = "0.58", features = ["Win32_System_Console"] }
```

### 步骤 2：修改 main.rs 入口
在 `main()` 函数开头添加控制台窗口隐藏逻辑：
```rust
fn main() {
    // 解析命令行参数（快速解析，仅获取 gui 标志）
    let cli_args = cli::CliArgs::parse();
    
    // Windows 平台：GUI 模式下隐藏控制台窗口
    #[cfg(windows)]
    if cli_args.gui || !cli_args.has_operational_args() {
        // 调用 Windows API 释放控制台
        unsafe {
            windows::Win32::System::Console::FreeConsole();
        }
    }
    
    // 继续原有主逻辑
    if let Err(e) = run() {
        // GUI 模式下错误需要通过对话框显示
        if cli_args.gui {
            rfd::MessageDialog::new()
                .set_title("音频路由器 - 错误")
                .set_description(&format!("程序启动失败: {}", e))
                .set_buttons(rfd::MessageButtons::Ok)
                .show();
        }
        std::process::exit(1);
    }
}
```

### 步骤 3：调整 run() 函数签名
将 `cli_args` 传递给 `run()` 函数，避免重复解析：
```rust
fn run() -> Result<()> {
    let cli_args = cli::CliArgs::parse();
    // ... 原有逻辑
}
```
改为：
```rust
fn run(cli_args: cli::CliArgs) -> Result<()> {
    // ... 原有逻辑，移除内部的 CliArgs::parse() 调用
}
```

### 步骤 4：处理 GUI 模式下的错误显示
当 GUI 模式发生错误时，使用 `rfd` 消息对话框显示错误，而不是控制台输出：
- 项目已依赖 `rfd = "0.15"`，可直接使用

### 步骤 5：测试验证
1. 双击 exe → 应只显示 GUI 窗口，无命令行窗口
2. 命令行运行 `audio_router.exe --help` → 应正常显示帮助信息
3. 命令行运行 `audio_router.exe --gui` → 应只显示 GUI 窗口
4. 命令行运行带音频参数 → 应正常 CLI 运行

## 文件修改清单
| 文件 | 修改内容 |
|------|----------|
| `Cargo.toml` | 添加 `windows` crate 依赖（仅 Windows 平台） |
| `src/main.rs` | 添加控制台隐藏逻辑，调整 `run()` 函数签名 |

## 注意事项
1. `FreeConsole()` 是 Windows 特有 API，需要 `#[cfg(windows)]` 条件编译
2. 需要使用 `unsafe` 块调用 Windows API
3. 错误处理需要区分 GUI 和 CLI 模式
4. 日志系统在 GUI 模式下仍可写入文件（通过 `--log-file` 参数）
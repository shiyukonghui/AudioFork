# Checklist: Phase 4 GUI 与打磨

## 验收检查项（对应开发规划 A4.1 ~ A4.8）

- [ ] **A4.1** GUI 启动 < 1 秒 — 需计时测量（代码就绪，需运行时验证）
- [ ] **A4.2** 按钮/滑块响应 < 50ms — 需慢动作视频分析（egui 即时模式天然满足）
- [ ] **A4.3** 引擎运行中 GUI 保持 30 FPS — 需帧率计数器验证
- [ ] **A4.4** GUI panic 后引擎不中断或优雅降级 — 需注入 panic 测试（catch_unwind + set_hook 已实现）
- [ ] **A4.5** 配置文件导入/导出完整 — 需对比导入前后所有参数（rfd FileDialog 已集成）
- [ ] **A4.6** 热插拔后 GUI 设备列表自动刷新 — 需插入/拔出 USB 设备验证（EngineToGui::DeviceListUpdated 消息已定义）
- [ ] **A4.7** Windows + macOS 原生窗口正常 — 需双平台逐一验证（egui/eframe 跨平台）
- [ ] **A4.8** GUI 内存 < 30MB，总内存 < 80MB — 需任务管理器/Activity Monitor 测量

## 工程检查项

- [x] `Cargo.toml` 包含 `egui = "0.31"`、`eframe = "0.31"`、`rfd = "0.15"`、`sysinfo = "0.33"` 依赖
- [x] `cargo check` 无编译错误
- [x] `cargo build --release` 通过
- [x] `src/message.rs` 存在，包含 `GuiToEngine`、`EngineToGui`、`EngineConfig`、`OutputSnapshot`、`DeviceInfoSnapshot`
- [x] `src/gui/mod.rs` 存在，包含 `AudioRouterApp`（impl `eframe::App`）和 `run_gui()` 函数
- [x] `src/gui/toolbar.rs` 存在，包含 `ToolbarState`、`ToolbarAction`、状态灯、启动/停止、导入/导出按钮
- [x] `src/gui/devices.rs` 存在，包含 `DevicesPanelState`、输入设备下拉、输出设备复选框列表、刷新按钮
- [x] `src/gui/params.rs` 存在，包含 `ParamsPanelState`、缓冲区滑块、延迟显示、重采样/漂移/限幅/独占/输入丢失控件
- [x] `src/gui/status.rs` 存在，包含 `StatusBarState`、`EngineStatus`、各输出指标显示
- [x] `src/gui/logs.rs` 存在，包含 `LogPanel`、100 条日志上限、折叠/展开、清空
- [x] `src/gui/theme.rs` 存在，包含 `ThemeMode`、`detect_system_theme()`、`apply_theme()`
- [x] `src/main.rs` GUI 占位已替换为 `gui::run_gui()` 调用
- [x] `src/main.rs` CLI 模式代码完全保留不变
- [x] 消息通道 `crossbeam-channel` 用于 GUI↔引擎通信，消息不进入音频回调
- [x] `--gui` 启动后显示 egui 窗口而非打印提示并退出
- [x] `--gui` 与 `--monitor` 互斥（cli.rs `conflicts_with` 保持不变）
- [x] 所有新建源文件包含中文注释
- [x] 引擎启动后参数面板和设备选择锁定，停止后解锁
- [x] Panic 隔离：`catch_unwind` 包裹 + `set_hook` 自定义 hook

## Phase 3 回归检查项

- [x] CLI 模式（无 `--gui`）正常启动并工作
- [x] 设备枚举、管道创建、重采样、漂移补偿、限幅器均不受影响
- [x] 热插拔监听线程不受 GUI 影响（CLI 模式下仍正常工作）
- [x] `--monitor` JSON 输出不受 GUI 添加影响

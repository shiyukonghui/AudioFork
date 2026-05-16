# Tasks: Phase 4 GUI 与打磨

## 任务概览

每个子代理严格只负责**一个文件或逻辑模块**的开发或改建，按依赖顺序执行。

---

- [x] Task 1: 添加 Phase 4 依赖到 `Cargo.toml`
  - 文件：`Cargo.toml`
  - 内容：
    - 在 `[dependencies]` 中添加：
      - `egui = "0.31"` — 即时模式 GUI 库
      - `eframe = "0.31"` — egui 框架（含窗口、事件循环、原生集成）
      - `rfd = "0.15"` — 原生文件对话框
      - `sysinfo = "0.33"` — 系统信息（CPU/内存统计）
    - 保留现有所有依赖
  - 验证：`cargo check` 通过

- [x] Task 2: 创建 `src/message.rs` — 消息通道枚举
  - 文件：`src/message.rs`（新建）
  - 内容：
    - **`GuiToEngine` 枚举**（GUI → 引擎）：
      ```rust
      pub enum GuiToEngine {
          Start(EngineConfig),
          Stop,
          EnableDevice(String),
          DisableDevice(String),
          UpdateParam(ParamChange),
      }
      ```
    - **`EngineToGui` 枚举**（引擎 → GUI）：
      ```rust
      pub enum EngineToGui {
          Started,
          Stopped { stats: Vec<OutputSnapshot> },
          DeviceListUpdated(Vec<DeviceInfoSnapshot>),
          OutputStatus(Vec<OutputSnapshot>),
          Error(String),
          Log(String),
      }
      ```
    - **`EngineConfig` 结构体**：从 GUI 面板参数构建的完整启动配置
      ```rust
      #[derive(Debug, Clone)]
      pub struct EngineConfig {
          pub input_device: Option<String>,
          pub output_devices: Vec<String>,
          pub sample_rate: u32,
          pub channels: u16,
          pub buffer_frames: u32,
          pub max_latency_ms: u32,
          pub resampler: String,
          pub no_drift_compensation: bool,
          pub exit_on_input_loss: bool,
          pub input_fallback_to_default: bool,
          pub no_limiter: bool,
          pub wasapi_exclusive: bool,
      }
      ```
    - **`ParamChange` 枚举**：运行时可变参数（Phase 4 初始仅支持设备启停）
      ```rust
      #[derive(Debug, Clone)]
      pub enum ParamChange {
          EnableOutput(String),
          DisableOutput(String),
      }
      ```
    - **`OutputSnapshot` 结构体**：输出设备实时状态快照
      ```rust
      #[derive(Debug, Clone)]
      pub struct OutputSnapshot {
          pub device_name: String,
          pub underrun_count: u64,
          pub overflow_count: u64,
          pub latency_ms: f64,
          pub delta: i32,
          pub water_level_pct: f64,
      }
      ```
    - **`DeviceInfoSnapshot` 结构体**：设备信息的可序列化快照（不含 cpal::Device 句柄）
      ```rust
      #[derive(Debug, Clone)]
      pub struct DeviceInfoSnapshot {
          pub name: String,
          pub sample_rates: Vec<u32>,
          pub channels: Vec<u16>,
          pub formats: Vec<String>,
          pub device_type: String,  // "Wired" / "Bluetooth" / "Network" / "Unknown"
      }
      ```
    - **中文注释**
  - 验证：`cargo check --lib` 通过
  - **依赖**：无（独立模块）

- [x] Task 3: 创建 `src/gui/theme.rs` — 主题管理
  - 文件：`src/gui/theme.rs`（新建）
  - 内容：
    - **`ThemeMode` 枚举**：`Light` / `Dark`
    - **函数 `detect_system_theme() -> ThemeMode`**：
      - Windows：检测注册表 `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`（1=浅色, 0=深色）。若读取失败则默认 Light。
      - macOS：暂返回 Light（后续可用 `NSAppearance` 检测）。
    - **函数 `apply_theme(ctx: &egui::Context, mode: ThemeMode)`**：
      - `Light` → `ctx.set_visuals(egui::Visuals::light())`
      - `Dark` → `ctx.set_visuals(egui::Visuals::dark())`
    - **中文注释**
  - 验证：`cargo check` 通过
  - **依赖**：Task 1（egui 依赖）

- [x] Task 4: 创建 `src/gui/logs.rs` — 日志/事件区域面板
  - 文件：`src/gui/logs.rs`（新建）
  - 内容：
    - **`LogPanel` 结构体**：
      - `entries: Vec<(String, String)>` — (时间戳, 日志内容)，最多 100 条
      - `collapsed: bool` — 是否折叠
    - **方法**：
      - `new() -> Self`
      - `push(&mut self, timestamp: String, msg: String)` — 追加日志，超出 100 条移除最旧
      - `clear(&mut self)` — 清空
      - `show(&mut self, ui: &mut egui::Ui)` — 渲染日志面板：
        - 标题 "日志" 带折叠/展开按钮
        - 展开时显示 `egui::ScrollArea` 内嵌 `egui::Grid` 或 `egui::Label`
        - 每条日志显示时间戳 + 内容
        - "清空"按钮
    - **中文注释**
  - 验证：`cargo check` 通过
  - **依赖**：Task 1（egui 依赖）

- [x] Task 5: 创建 `src/gui/toolbar.rs` — 顶部工具栏面板
  - 文件：`src/gui/toolbar.rs`（新建）
  - 内容：
    - **`ToolbarState` 结构体**：
      - `engine_running: bool` — 引擎是否运行中
      - `engine_error: bool` — 引擎是否出错
      - `config_path: Option<String>` — 当前配置路径
    - **方法**：
      - `new() -> Self`
      - `show(&mut self, ui: &mut egui::Ui) -> ToolbarAction` — 渲染工具栏：
        - 左侧：状态指示灯（`egui::Label` 带彩色圆圈 Unicode）
        - 中间：启动/停止按钮（根据 `engine_running` 切换文本和颜色）
        - 右侧：配置导入/导出按钮（调用 `rfd::FileDialog` 逻辑在返回值中）
        - 最右：帮助链接
    - **`ToolbarAction` 枚举**：
      ```rust
      pub enum ToolbarAction {
          None,
          Start,
          Stop,
          ImportConfig,
          ExportConfig,
      }
      ```
    - **中文注释**
  - 验证：`cargo check` 通过
  - **依赖**：Task 1（egui + rfd 依赖）

- [x] Task 6: 创建 `src/gui/devices.rs` — 设备管理面板（左侧）
  - 文件：`src/gui/devices.rs`（新建）
  - 内容：
    - **`DevicesPanelState` 结构体**：
      - `input_devices: Vec<DeviceInfoSnapshot>` — 输入设备列表
      - `output_devices: Vec<DeviceInfoSnapshot>` — 输出设备列表
      - `selected_input_device: Option<usize>` — 选中的输入设备索引
      - `selected_output_devices: std::collections::HashSet<String>` — 勾选的输出设备名
      - `engine_running: bool` — 引擎是否运行（控制是否锁定选择）
    - **方法**：
      - `new() -> Self`
      - `refresh_devices(&mut self)` — 调用 `device::enumerate_*_devices()` 刷新列表
      - `show(&mut self, ui: &mut egui::Ui) -> DevicesPanelAction` — 渲染设备面板：
        - 输入设备区：`egui::ComboBox` 下拉选择，显示格式 `"设备名 [48kHz, 2ch, f32]"`
        - 输出设备区：`egui::ScrollArea` + 每行一个 `egui::Checkbox`，显示格式 `"[✓] 设备名 — 48kHz 2ch [直通]"`，蓝牙设备标注 `⚠ 高延迟`
        - "刷新设备"按钮
        - 若 `engine_running`，所有控件 `set_enabled(false)`
    - **`DevicesPanelAction` 枚举**：
      ```rust
      pub enum DevicesPanelAction {
          None,
          RefreshDevices,
          InputDeviceSelected(String),
          OutputDeviceToggled(String),
      }
      ```
    - **中文注释**
  - 验证：`cargo check` 通过
  - **依赖**：Task 1（egui 依赖），Task 2（message.rs 的 DeviceInfoSnapshot）

- [x] Task 7: 创建 `src/gui/params.rs` — 参数配置面板（右侧）
  - 文件：`src/gui/params.rs`（新建）
  - 内容：
    - **`ParamsPanelState` 结构体**：
      - 所有可配置参数字段（模仿 `EngineConfig` 的字段）
      - `engine_running: bool`
    - **字段**：
      - `buffer_frames: u32`（默认 256）
      - `max_latency_ms: u32`（默认 30）
      - `sample_rate: u32`（默认 48000）
      - `resampler: usize`（索引: 0=Sinc, 1=Cubic, 2=None）
      - `no_drift_compensation: bool`
      - `wasapi_exclusive: bool`
      - `exit_on_input_loss: bool`
      - `input_fallback_to_default: bool`
      - `no_limiter: bool`
    - **方法**：
      - `new() -> Self` — 从默认值初始化
      - `load_from_config(config: &EngineConfig)` — 从配置填充
      - `build_engine_config(&self, input_device: Option<String>, output_devices: Vec<String>) -> EngineConfig` — 构建启动配置
      - `show(&mut self, ui: &mut egui::Ui)` — 渲染参数面板：
        - 缓冲区帧数：`egui::Slider::new(&mut self.buffer_frames, 32..=4096)` + 实时显示延迟 ms
        - 最大延迟：`egui::DragValue` 输入框
        - 重采样算法：`egui::ComboBox` 下拉
        - 漂移补偿：`egui::Checkbox`
        - 独占模式：`egui::Checkbox`（`#[cfg(feature = "wasapi-exclusive")]` 条件下才显示，非 Windows 平台隐藏）
        - 限幅器：`egui::Checkbox`（反向：勾选=启用限幅器，即 `!no_limiter`）
        - 输入丢失行为：`egui::RadioButton` 组（退出 / 保持静默并重试）
        - 若 `engine_running`，所有控件 `set_enabled(false)`
    - **中文注释**
  - 验证：`cargo check` 通过
  - **依赖**：Task 1（egui 依赖），Task 2（message.rs 的 EngineConfig）

- [x] Task 8: 创建 `src/gui/status.rs` — 底部状态栏面板
  - 文件：`src/gui/status.rs`（新建）
  - 内容：
    - **`StatusBarState` 结构体**：
      - `engine_status: EngineStatus` — 引擎状态
      - `output_statuses: Vec<OutputSnapshot>` — 各输出状态
      - `cpu_usage: f32` — CPU 使用率（%）
      - `memory_mb: f32` — 内存占用（MB）
      - `show_system_usage: bool` — 是否显示系统资源
    - **`EngineStatus` 枚举**：`Stopped` / `Running` / `Error(String)`
    - **方法**：
      - `new() -> Self`
      - `update_outputs(&mut self, statuses: Vec<OutputSnapshot>)`
      - `show(&mut self, ui: &mut egui::Ui)` — 渲染状态栏：
        - 单行水平布局：引擎状态文本 + 各输出设备紧凑卡片（名称/欠载/溢出/延迟/delta/水位%）
        - 若 `show_system_usage`，追加 CPU 和内存信息
        - 使用 `egui::Frame` 包裹，背景色区别于主区域
    - **中文注释**
  - 验证：`cargo check` 通过
  - **依赖**：Task 1（egui 依赖），Task 2（message.rs 的 OutputSnapshot）

- [x] Task 9: 创建 `src/gui/mod.rs` — GUI 入口与主窗口聚合
  - 文件：`src/gui/mod.rs`（新建）
  - 内容：
    - **`AudioRouterApp` 结构体** — 实现 `eframe::App` trait：
      - **字段**：
        - `toolbar: ToolbarState`
        - `devices: DevicesPanelState`
        - `params: ParamsPanelState`
        - `status_bar: StatusBarState`
        - `log_panel: LogPanel`
        - `theme_mode: ThemeMode`
        - `engine_running: bool`
        - `engine_tx: Option<crossbeam_channel::Sender<GuiToEngine>>` — 发送到引擎线程
        - `engine_rx: Option<crossbeam_channel::Receiver<EngineToGui>>` — 接收引擎推送
        - `config_path: Option<String>`
        - `show_logs: bool`
      - **方法**：
        - `new(engine_tx: Sender<GuiToEngine>, engine_rx: Receiver<EngineToGui>, config_path: Option<String>, config: &AudioRouterConfig) -> Self` — 初始化所有面板
        - 实现 `eframe::App::update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame)`：
          1. 应用主题（`theme::apply_theme`）
          2. 处理引擎消息（`engine_rx.try_recv()` 非阻塞）：
             - `Started` → `engine_running = true`
             - `Stopped { stats }` → `engine_running = false`，更新状态栏
             - `DeviceListUpdated(devices)` → 更新设备列表
             - `OutputStatus(statuses)` → 更新状态栏
             - `Error(msg)` → 更新引擎错误状态
             - `Log(msg)` → `log_panel.push(...)`
          3. 渲染布局（`egui::TopBottomPanel` + `egui::SidePanel` + `egui::CentralPanel`）：
             - **顶部**：`toolbar.show()`，根据返回的 `ToolbarAction` 执行对应操作
             - **底部**：`status_bar.show()` + `log_panel.show()`（可折叠）
             - **左侧**：`devices.show()`
             - **右侧**：`params.show()`
          4. 启动/停止逻辑：
             - 启动：`engine_tx.send(GuiToEngine::Start(config))` → 启动独立引擎线程
             - 停止：`engine_tx.send(GuiToEngine::Stop)`
          5. 30 FPS 刷新：`ctx.request_repaint_after(Duration::from_millis(33))`

    - **公共函数 `run_gui(config_path: Option<String>) -> Result<()>`**：
      - 加载配置文件（或使用默认值）
      - 枚举设备填充初始列表
      - 创建 `crossbeam_channel::unbounded::<GuiToEngine>()` 和 `crossbeam_channel::unbounded::<EngineToGui>()`
      - 构建 `AudioRouterApp`
      - 调用 `eframe::run_native("音频路由器", options, Box::new(|_cc| Ok(Box::new(app))))`
      - 窗口关闭时若引擎运行中，发送停止指令
    
    - **Panic 隔离**：在 `update()` 开头设置 `std::panic::set_hook`（仅一次），捕获 panic 信息后通过 `rfd::MessageDialog` 弹窗

    - **中文注释**
  - 验证：`cargo check` 通过
  - **依赖**：Task 1-8 全部完成

- [x] Task 10: 改建 `src/main.rs` — 替换 GUI 占位为实际启动
  - 文件：`src/main.rs`（改建）
  - 内容：
    - **(A) 模块声明**：添加 `mod message; mod gui;`
    - **(B) 替换 GUI 占位代码**（约第 164-169 行）：
      - 删除 `tracing::info!("GUI 模式将在第四阶段实现"); println!("GUI 模式将在第四阶段实现"); std::process::exit(0);`
      - 替换为：
        ```rust
        if resolved.gui {
            let _filtered = cli_args.filter_for_gui();
            tracing::info!("启动 GUI 模式...");
            return gui::run_gui(resolved.config_path.clone());
        }
        ```
    - **(C) 保留所有现有**：Phase 3 的 CLI 流程、设备枚举、管道创建、主循环、停止流程完全不变
    - **中文注释**
  - 验证：`cargo build --release` 通过
  - **依赖**：Task 1-9 全部完成

---

## 任务依赖关系

```
Task 1 (Cargo.toml: egui/eframe/rfd/sysinfo)
 ├─► Task 2 (message.rs) ──────────────────────┐
 ├─► Task 3 (theme.rs) ────────────────────────┤
 ├─► Task 4 (logs.rs) ─────────────────────────┤
 ├─► Task 6 (devices.rs ─── 依赖 Task 2) ──────┤
 ├─► Task 7 (params.rs ──── 依赖 Task 2) ──────┤
 ├─► Task 5 (toolbar.rs) ──────────────────────┤
 └─► Task 8 (status.rs ─── 依赖 Task 2) ───────┤
                                               │
      Task 2-8 全部完成 ──► Task 9 (mod.rs) ◄──┘
                                    │
                                    ▼
                              Task 10 (main.rs)
```

**可并行执行的任务组合：**
- Task 2, Task 3, Task 4, Task 5, Task 8 可在 Task 1 完成后**同时执行**
- Task 6, Task 7 依赖 Task 2（需要 DeviceInfoSnapshot / EngineConfig 类型），可在 Task 1+2 完成后与上述任务并行

**串行执行的任务：**
- Task 9 必须在 Task 2-8 全部完成后执行
- Task 10 必须在 Task 9 完成后执行

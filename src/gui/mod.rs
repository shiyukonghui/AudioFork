// 音频路由器 — GUI 图形界面模块
// 基于 egui/eframe 实现跨平台原生窗口
// AudioRouterApp 负责整合所有子面板并协调消息通信

// ===== 子模块声明 =====
pub mod devices;
pub mod logs;
pub mod params;
pub mod status;
pub mod theme;
pub mod toolbar;

// ===== 外部依赖导入 =====
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender};

use crate::config::AudioRouterConfig;
use crate::message::{EngineConfig, EngineToGui, GuiToEngine};

// ============================================================================
// AudioRouterApp — 主应用程序结构体，实现 eframe::App trait
// ============================================================================

/// 音频路由器 GUI 主体，整合各面板状态并处理消息循环
pub struct AudioRouterApp {
    /// 顶部工具栏状态
    toolbar: toolbar::ToolbarState,
    /// 左侧设备管理面板状态
    devices: devices::DevicesPanelState,
    /// 右侧参数配置面板状态
    params: params::ParamsPanelState,
    /// 底部状态栏状态
    status_bar: status::StatusBarState,
    /// 底部日志面板状态
    log_panel: logs::LogPanel,
    /// 当前主题模式（浅色/深色）
    theme_mode: theme::ThemeMode,
    /// 引擎是否正在运行
    engine_running: bool,
    /// 发送消息给引擎的通道
    engine_tx: Sender<GuiToEngine>,
    /// 接收引擎消息的通道
    engine_rx: Receiver<EngineToGui>,
    /// 当前配置文件的路径（可选）
    config_path: Option<String>,
    /// 是否已显示日志面板（预留给用户手动切换日志面板的显示状态）
    #[allow(dead_code)]
    show_logs: bool,
    /// panic hook 是否已设置（确保只设置一次）
    panic_hook_set: bool,
    /// 中文字体是否已加载（确保只加载一次）
    font_loaded: bool,
}

impl AudioRouterApp {
    /// 创建 AudioRouterApp 实例
    ///
    /// # 参数
    /// * `engine_tx` - 发送消息给后台引擎的通道
    /// * `engine_rx` - 接收后台引擎消息的通道
    /// * `config_path` - 配置文件路径（可选）
    /// * `file_config` - 从文件加载的音频路由器配置
    ///
    /// # 初始化流程
    /// 1. 创建所有面板的默认状态
    /// 2. 从文件配置填充参数面板
    /// 3. 枚举并刷新设备列表
    /// 4. 检测系统主题
    pub fn new(
        engine_tx: Sender<GuiToEngine>,
        engine_rx: Receiver<EngineToGui>,
        config_path: Option<String>,
        file_config: &AudioRouterConfig,
    ) -> Self {
        let mut toolbar = toolbar::ToolbarState::new();
        toolbar.config_path = config_path.clone();

        let mut devices = devices::DevicesPanelState::new();
        // 从文件配置设置音源类型和回采设备
        devices.source_type = file_config.source_type.clone();
        devices.selected_loopback_device = file_config.loopback_device.clone().unwrap_or_default();
        // 尝试枚举设备（初始化时的失败不致命，仅记录警告）
        if let Err(e) = devices.refresh_devices() {
            tracing::warn!("初始化设备列表失败: {}", e);
        }

        // 从文件配置构建 EngineConfig 并加载到参数面板
        let engine_cfg = EngineConfig {
            input_device: file_config.input_device.clone(),
            output_devices: file_config.output_devices.clone(),
            sample_rate: file_config.sample_rate.unwrap_or(48000),
            buffer_frames: file_config.buffer_frames.unwrap_or(256),
            max_latency_ms: file_config.max_latency_ms.unwrap_or(30),
            resampler: file_config.resampler.clone(),
            no_drift_compensation: file_config.no_drift_compensation,
            exit_on_input_loss: file_config.exit_on_input_loss,
            input_fallback_to_default: file_config.input_fallback_to_default,
            no_limiter: file_config.no_limiter,
            wasapi_exclusive: file_config.wasapi_exclusive,
            source_type: file_config.source_type.clone(),
            loopback_device: file_config.loopback_device.clone(),
        };
        let mut params = params::ParamsPanelState::new();
        params.load_from_config(&engine_cfg);

        let theme_mode = theme::detect_system_theme();

        Self {
            toolbar,
            devices,
            params,
            status_bar: status::StatusBarState::new(),
            log_panel: logs::LogPanel::new(),
            theme_mode,
            engine_running: false,
            engine_tx,
            engine_rx,
            config_path,
            show_logs: true,
            panic_hook_set: false,
            font_loaded: false,
        }
    }
}

// ============================================================================
// eframe::App trait 实现
// ============================================================================

impl eframe::App for AudioRouterApp {
    /// eframe 每帧调用的主更新函数
    ///
    /// # 每帧执行流程
    /// 1. 设置 panic hook（仅首次）
    /// 2. 应用当前主题
    /// 3. 处理来自引擎的消息（非阻塞）
    /// 4. 渲染 UI 布局
    /// 5. 请求下一帧刷新（约 30 FPS）
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // ====================================================================
        // (a) 设置 panic hook，仅执行一次
        // ====================================================================
        if !self.panic_hook_set {
            self.panic_hook_set = true;
            let engine_tx = self.engine_tx.clone();
            let previous_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                // 先调用默认的 panic hook 以保留标准错误输出
                previous_hook(info);
                // 格式化错误信息
                let msg = format!("GUI 内部错误: {}", info);
                // 尝试通知引擎停止
                let _ = engine_tx.send(GuiToEngine::Stop);
                // 弹出错误对话框（在 panic hook 中不可靠，尽力而为）
                let _ = rfd::MessageDialog::new()
                    .set_title("音频路由器 - 错误")
                    .set_description(&msg)
                    .set_buttons(rfd::MessageButtons::Ok)
                    .show();
            }));
        }

        // ====================================================================
        // (a.2) 首次加载中文字体，解决默认字体不包含 CJK 字形的问题
        // ====================================================================
        if !self.font_loaded {
            self.font_loaded = true;
            load_chinese_font(ctx);
        }

        // ====================================================================
        // (b) 应用当前主题
        // ====================================================================
        theme::apply_theme(ctx, self.theme_mode);

        // ====================================================================
        // (c) 非阻塞处理来自引擎的消息
        // ====================================================================
        while let Ok(msg) = self.engine_rx.try_recv() {
            match msg {
                EngineToGui::Ready => {
                    self.log_panel
                        .push(chrono_now(), "引擎线程已就绪".to_string());
                }
                EngineToGui::Started => {
                    self.engine_running = true;
                    self.toolbar.engine_running = true;
                    self.devices.engine_running = true;
                    self.params.engine_running = true;
                    self.status_bar.engine_status = status::EngineStatus::Running;
                    self.log_panel
                        .push(chrono_now(), "引擎已启动".to_string());
                }
                EngineToGui::Stopped { stats } => {
                    self.engine_running = false;
                    self.toolbar.engine_running = false;
                    self.devices.engine_running = false;
                    self.params.engine_running = false;
                    self.status_bar.engine_status = status::EngineStatus::Stopped;
                    self.status_bar.update_outputs(stats);
                    self.log_panel
                        .push(chrono_now(), "引擎已停止".to_string());
                }
                EngineToGui::DeviceListUpdated(devices) => {
                    // 引擎通知设备列表发生了变化，重新枚举本地设备
                    if let Err(e) = self.devices.refresh_devices() {
                        self.log_panel
                            .push(chrono_now(), format!("刷新设备失败: {}", e));
                    }
                    self.log_panel.push(
                        chrono_now(),
                        format!("设备列表已更新（{} 个设备）", devices.len()),
                    );
                }
                EngineToGui::OutputStatus(statuses) => {
                    self.status_bar.update_outputs(statuses);
                }
                EngineToGui::Error(msg) => {
                    self.status_bar.engine_status =
                        status::EngineStatus::Error(msg.clone());
                    self.log_panel
                        .push(chrono_now(), format!("错误: {}", msg));
                }
                EngineToGui::Log(msg) => {
                    self.log_panel.push(chrono_now(), msg);
                }
            }
        }

        // ====================================================================
        // (d) 渲染窗口布局
        // ====================================================================

        // ---------- 顶部工具栏 ----------
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            match self.toolbar.show(ui) {
                toolbar::ToolbarAction::Start => {
                    // 检查是否选择了输出设备
                    if self.devices.selected_output_devices.is_empty() {
                        self.log_panel.push(
                            chrono_now(),
                            "错误: 请至少选择一个输出设备".to_string(),
                        );
                        self.status_bar.engine_status =
                            status::EngineStatus::Error("未选择输出设备".to_string());
                    } else {
                        // 从参数面板和设备面板收集配置，构建引擎配置
                        tracing::info!(
                            "用户点击启动按钮，输出设备: {:?}",
                            self.devices.selected_output_devices
                        );
                        let mut config = self.params.build_engine_config(
                            if self.devices.selected_input_device.is_empty() {
                                None
                            } else {
                                Some(self.devices.selected_input_device.clone())
                            },
                            self.devices
                                .selected_output_devices
                                .iter()
                                .cloned()
                                .collect(),
                        );
                        // 使用设备面板中选择的音源类型和回采设备
                        config.source_type = self.devices.source_type.clone();
                        config.loopback_device = if self.devices.selected_loopback_device.is_empty() {
                            None
                        } else {
                            Some(self.devices.selected_loopback_device.clone())
                        };
                        tracing::info!("正在发送启动指令给引擎线程");
                        let _ = self.engine_tx.send(GuiToEngine::Start(config));
                        tracing::info!("启动指令已发送");
                    }
                }
                toolbar::ToolbarAction::Stop => {
                    let _ = self.engine_tx.send(GuiToEngine::Stop);
                }
                toolbar::ToolbarAction::ImportConfig => {
                    // 使用 rfd 原生文件对话框选择 TOML 配置文件
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("TOML 配置", &["toml"])
                        .pick_file()
                    {
                        match crate::config::load_config(&path) {
                            Ok(cfg) => {
                                // 将文件配置转换为引擎配置并加载到参数面板
                                let engine_cfg = EngineConfig {
                                    input_device: cfg.input_device.clone(),
                                    output_devices: cfg.output_devices.clone(),
                                    sample_rate: cfg.sample_rate.unwrap_or(48000),
                                    buffer_frames: cfg.buffer_frames.unwrap_or(256),
                                    max_latency_ms: cfg.max_latency_ms.unwrap_or(30),
                                    resampler: cfg.resampler.clone(),
                                    no_drift_compensation: cfg.no_drift_compensation,
                                    exit_on_input_loss: cfg.exit_on_input_loss,
                                    input_fallback_to_default: cfg.input_fallback_to_default,
                                    no_limiter: cfg.no_limiter,
                                    wasapi_exclusive: cfg.wasapi_exclusive,
                                    source_type: cfg.source_type.clone(),
                                    loopback_device: cfg.loopback_device.clone(),
                                };
                                self.params.load_from_config(&engine_cfg);
                                // 设置设备面板的源类型
                                self.devices.source_type = cfg.source_type.clone();
                                self.devices.selected_loopback_device = cfg.loopback_device.clone().unwrap_or_default();
                                let path_str = path.to_string_lossy().to_string();
                                self.config_path = Some(path_str.clone());
                                self.toolbar.config_path = Some(path_str);
                                self.log_panel
                                    .push(chrono_now(), "配置已导入".to_string());
                            }
                            Err(e) => {
                                self.log_panel.push(
                                    chrono_now(),
                                    format!("导入配置失败: {}", e),
                                );
                            }
                        }
                    }
                }
                toolbar::ToolbarAction::ExportConfig => {
                    // 使用 rfd 原生文件对话框选择保存路径
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("TOML 配置", &["toml"])
                        .set_file_name("audio_router.toml")
                        .save_file()
                    {
                        // 从参数面板和设备面板收集配置
                        let config = self.params.build_engine_config(
                            if self.devices.selected_input_device.is_empty() {
                                None
                            } else {
                                Some(self.devices.selected_input_device.clone())
                            },
                            self.devices
                                .selected_output_devices
                                .iter()
                                .cloned()
                                .collect(),
                        );
                        // 转换为 AudioRouterConfig 格式用于序列化
                        let file_cfg = AudioRouterConfig {
                            input_device: config.input_device,
                            output_devices: config.output_devices,
                            sample_rate: Some(config.sample_rate),
                            buffer_frames: Some(config.buffer_frames),
                            max_latency_ms: Some(config.max_latency_ms),
                            resampler: config.resampler,
                            no_drift_compensation: config.no_drift_compensation,
                            exit_on_input_loss: config.exit_on_input_loss,
                            input_fallback_to_default: config.input_fallback_to_default,
                            no_limiter: config.no_limiter,
                            wasapi_exclusive: config.wasapi_exclusive,
                            source_type: config.source_type.clone(),
                            loopback_device: config.loopback_device.clone(),
                            ..AudioRouterConfig::default()
                        };
                        match crate::config::save_config(&file_cfg, &path) {
                            Ok(()) => {
                                let path_str = path.to_string_lossy().to_string();
                                self.config_path = Some(path_str.clone());
                                self.toolbar.config_path = Some(path_str);
                                self.log_panel
                                    .push(chrono_now(), "配置已导出".to_string());
                            }
                            Err(e) => {
                                self.log_panel.push(
                                    chrono_now(),
                                    format!("导出配置失败: {}", e),
                                );
                            }
                        }
                    }
                }
                toolbar::ToolbarAction::None => {}
            }
        });

        // ---------- 左侧设备管理面板 ----------
        egui::SidePanel::left("devices")
            .default_width(280.0)
            .resizable(true)
            .show(ctx, |ui| {
                match self.devices.show(ui) {
                    devices::DevicesPanelAction::RefreshDevices => {
                        if let Err(e) = self.devices.refresh_devices() {
                            self.log_panel
                                .push(chrono_now(), format!("刷新设备失败: {}", e));
                        }
                    }
                    devices::DevicesPanelAction::InputDeviceSelected(name) => {
                        // 更新选中的输入设备
                        self.devices.selected_input_device = name;
                    }
                    devices::DevicesPanelAction::OutputDeviceToggled(name) => {
                        // 切换输出设备的勾选状态
                        if self.devices.selected_output_devices.contains(&name) {
                            self.devices.selected_output_devices.remove(&name);
                        } else {
                            self.devices.selected_output_devices.insert(name);
                        }
                    }
                    devices::DevicesPanelAction::SourceTypeChanged(source_type) => {
                        self.devices.source_type = source_type.clone();
                        // 当切换到 Loopback 模式时，刷新设备列表以排除回采设备
                        if source_type == "loopback" {
                            if let Err(e) = self.devices.refresh_devices() {
                                self.log_panel
                                    .push(chrono_now(), format!("刷新设备失败: {}", e));
                            }
                        }
                    }
                    devices::DevicesPanelAction::LoopbackDeviceSelected(name) => {
                        self.devices.selected_loopback_device = name.clone();
                        // 当用户选择回采设备时，自动从已勾选的输出设备集合中移除该设备
                        // 防止同一设备同时作为回采源和输出目标
                        if !name.is_empty() {
                            // 使用模糊匹配移除（不区分大小写）
                            let lb_name_lower = name.to_lowercase();
                            self.devices.selected_output_devices.retain(|dev_name| {
                                let dev_lower = dev_name.to_lowercase();
                                if dev_lower.contains(&lb_name_lower) || lb_name_lower.contains(&dev_lower) {
                                    tracing::info!(
                                        "已从输出设备勾选列表中移除 '{}'（与回采设备 '{}' 重名）",
                                        dev_name, name
                                    );
                                    false
                                } else {
                                    true
                                }
                            });
                        }
                        // 刷新设备列表，确保输出设备列表中排除回采设备
                        if let Err(e) = self.devices.refresh_devices() {
                            self.log_panel
                                .push(chrono_now(), format!("刷新设备失败: {}", e));
                        }
                    }
                    devices::DevicesPanelAction::None => {}
                }
            });

        // ---------- 右侧参数配置面板 ----------
        egui::SidePanel::right("params")
            .default_width(300.0)
            .resizable(true)
            .show(ctx, |ui| {
                self.params.show(ui);
            });

        // ---------- 中央区域占位（设备面板和参数面板之间） ----------
        egui::CentralPanel::default().show(ctx, |_ui| {
            // 中央区域目前为空，可在此放置波形显示或监控信息
        });

        // ---------- 底部状态栏 ----------
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            self.status_bar.show(ui);
        });

        // ---------- 底部日志面板 ----------
        egui::TopBottomPanel::bottom("log_area")
            .resizable(true)
            .show(ctx, |ui| {
                self.log_panel.show(ui);
            });

        // ====================================================================
        // (e) 请求约 30 FPS 的刷新率
        // ====================================================================
        ctx.request_repaint_after(Duration::from_millis(33));
    }
}

// ============================================================================
// 公共入口函数
// ============================================================================

/// 启动音频路由器的 GUI 图形界面模式
///
/// # 参数
/// * `config_path` - 可选的 TOML 配置文件路径
///
/// # 返回值
/// * `Ok(())` - GUI 窗口正常关闭
/// * `Err(AudioRouterError::Fatal)` - GUI 启动或运行期间发生致命错误
///
/// # 执行流程
/// 1. 加载配置文件（不存在则使用默认配置）
/// 2. 创建消息通道用于 GUI ↔ 引擎通信
/// 3. 构造 AudioRouterApp 实例
/// 4. 启动 eframe 原生窗口主循环
pub fn run_gui(config_path: Option<String>) -> crate::error::Result<()> {
    // ---------- 加载配置文件 ----------
    let file_config = if let Some(ref path) = config_path {
        let p = std::path::Path::new(path);
        if p.exists() {
            crate::config::load_config(p)?
        } else {
            tracing::info!("配置文件 '{}' 不存在，使用默认配置", p.display());
            AudioRouterConfig::default()
        }
    } else {
        AudioRouterConfig::default()
    };

    // ---------- 创建消息通道 ----------
    tracing::info!("正在创建消息通道...");
    // engine_to_gui: 引擎 → GUI 方向
    let (engine_to_gui_tx, engine_to_gui_rx) =
        crossbeam_channel::unbounded::<EngineToGui>();
    // gui_to_engine: GUI → 引擎方向
    let (gui_to_engine_tx, gui_to_engine_rx) =
        crossbeam_channel::unbounded::<GuiToEngine>();

    // 启动后台音频引擎线程，监听 GUI 消息并执行音频管道
    tracing::info!("正在启动引擎线程...");
    crate::engine::spawn_engine(gui_to_engine_rx, engine_to_gui_tx);
    tracing::info!("引擎线程已启动，正在创建 GUI 应用...");

    // ---------- 构造应用程序实例 ----------
    let app = AudioRouterApp::new(
        gui_to_engine_tx,
        engine_to_gui_rx,
        config_path.clone(),
        &file_config,
    );

    // ---------- 配置原生窗口选项 ----------
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0]),
        ..Default::default()
    };

    // ---------- 启动 eframe 主循环 ----------
    eframe::run_native(
        "音频路由器",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .map_err(|e| crate::error::AudioRouterError::Fatal(format!("GUI 启动失败: {}", e)))?;

    Ok(())
}

// ============================================================================
// 内部辅助函数
// ============================================================================

/// 加载系统中文字体并注册到 egui 字体系统
///
/// 按优先级尝试多个系统中文字体路径：
/// - Windows: Microsoft YaHei (微软雅黑) → SimHei (黑体)
/// - macOS: PingFang (苹方)
/// - Linux: Noto Sans CJK
///
/// 找到第一个可读取的字体后即停止搜索，并将其设置为最高优先级的
/// 等宽比例字体族，使中文、日文、韩文等 CJK 字符正确渲染。
fn load_chinese_font(ctx: &egui::Context) {
    // 系统中文字体的候选路径列表（按优先级排序）
    let font_paths = [
        // Windows
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        // macOS
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        // Linux
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ];

    for path in &font_paths {
        match std::fs::read(path) {
            Ok(data) => {
                let mut fonts = egui::FontDefinitions::default();

                // 将系统中文字体注册为 "chinese" 字体
                fonts
                    .font_data
                    .insert("chinese".to_owned(), egui::FontData::from_owned(data).into());

                // 将中文字体插入到 Proportional 和 Monospace 字体族的最前面，
                // 作为首选回退字体：egui 先尝试默认字体，找不到的字符用此字体渲染
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts
                        .families
                        .entry(family)
                        .or_default()
                        .insert(0, "chinese".to_owned());
                }

                ctx.set_fonts(fonts);
                tracing::info!("已加载中文字体: {}", path);
                return;
            }
            Err(_) => continue,
        }
    }

    tracing::warn!("未找到系统中文字体，中文可能显示为口字形");
}

/// 生成当前时间的格式化字符串 "HH:MM:SS"
///
/// 用于日志面板中的时间戳标记，无需引入 chrono 等第三方时间库。
fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    let seconds = secs % 60;
    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

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
pub mod tray;

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
    /// 日志面板状态（用于弹窗显示）
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
    /// 一次性初始化是否完成（panic hook，字体，HWND 获取等）
    init_done: bool,
    /// 系统托盘状态（仅用于持有托盘图标生命周期）
    #[allow(dead_code)]
    tray_state: Option<tray::TrayState>,
    /// 窗口 HWND（用于 ShowWindow 直接隐藏窗口，绕过 eframe 的 visibility 检测）
    hwnd: Option<isize>,
}

impl AudioRouterApp {
    pub fn new(
        engine_tx: Sender<GuiToEngine>,
        engine_rx: Receiver<EngineToGui>,
        config_path: Option<String>,
        file_config: &AudioRouterConfig,
        tray_state: Option<tray::TrayState>,
    ) -> Self {
        let mut toolbar = toolbar::ToolbarState::new();
        toolbar.config_path = config_path.clone();

        let mut devices = devices::DevicesPanelState::new();
        devices.source_type = file_config.source_type.clone();
        devices.selected_loopback_device = file_config.loopback_device.clone().unwrap_or_default();
        if let Err(e) = devices.refresh_devices() {
            tracing::warn!("初始化设备列表失败: {}", e);
        }

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
            max_outputs: file_config.max_outputs as usize,
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
            init_done: false,
            tray_state,
            hwnd: None,
        }
    }
}

// ============================================================================
// eframe::App trait 实现
//
// 关键设计：窗口隐藏使用 Windows ShowWindow(SW_HIDE) 直接在 HWND 上操作，
// 而不是使用 egui::ViewportCommand::Visible(false)。
// 托盘退出处理在 tray 模块的独立线程中完成，不依赖 update() 被调用。
// ============================================================================

impl eframe::App for AudioRouterApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 一次性初始化
        if !self.init_done {
            self.init_done = true;

            // panic hook
            let engine_tx = self.engine_tx.clone();
            let previous_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                previous_hook(info);
                let msg = format!("GUI 内部错误: {}", info);
                let _ = engine_tx.send(GuiToEngine::Stop);
                let _ = rfd::MessageDialog::new()
                    .set_title("音频路由器 - 错误")
                    .set_description(&msg)
                    .set_buttons(rfd::MessageButtons::Ok)
                    .show();
            }));

            load_chinese_font(ctx);

            self.hwnd = find_window_hwnd();
            if self.hwnd.is_some() {
                tracing::info!("已获取窗口 HWND");
            }
        }

        // 处理来自引擎的消息
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
                    self.toolbar.engine_error = true;
                    self.log_panel
                        .push(chrono_now(), format!("错误: {}", msg));
                }
                EngineToGui::Log(msg) => {
                    self.log_panel.push(chrono_now(), msg);
                }
            }
        }

        // 渲染 UI
        theme::apply_theme(ctx, self.theme_mode);

        // ---------- 顶部工具栏 ----------
        egui::TopBottomPanel::top("toolbar")
            .height_range(40.0..=60.0)
            .show(ctx, |ui| {
                match self.toolbar.show(ui) {
                    toolbar::ToolbarAction::Start => {
                        if self.devices.selected_output_devices.is_empty() {
                            self.log_panel.push(
                                chrono_now(),
                                "错误: 请至少选择一个输出设备".to_string(),
                            );
                            self.status_bar.engine_status =
                                status::EngineStatus::Error("未选择输出设备".to_string());
                        } else {
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
                            config.source_type = self.devices.source_type.clone();
                            config.loopback_device = if self.devices.selected_loopback_device.is_empty() {
                                None
                            } else {
                                Some(self.devices.selected_loopback_device.clone())
                            };
                            let _ = self.engine_tx.send(GuiToEngine::Start(config));
                        }
                    }
                    toolbar::ToolbarAction::Stop => {
                        let _ = self.engine_tx.send(GuiToEngine::Stop);
                    }
                    toolbar::ToolbarAction::ImportConfig => {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("TOML 配置", &["toml"])
                            .pick_file()
                        {
                            match crate::config::load_config(&path) {
                                Ok(cfg) => {
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
                                        max_outputs: cfg.max_outputs as usize,
                                    };
                                    self.params.load_from_config(&engine_cfg);
                                    self.devices.source_type = cfg.source_type.clone();
                                    self.devices.selected_loopback_device = cfg.loopback_device.clone().unwrap_or_default();
                                    let path_str = path.to_string_lossy().to_string();
                                    self.config_path = Some(path_str.clone());
                                    self.toolbar.config_path = Some(path_str);
                                    self.log_panel.push(chrono_now(), "配置已导入".to_string());
                                }
                                Err(e) => {
                                    self.log_panel.push(chrono_now(), format!("导入配置失败: {}", e));
                                }
                            }
                        }
                    }
                    toolbar::ToolbarAction::ExportConfig => {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("TOML 配置", &["toml"])
                            .set_file_name("audio_router.toml")
                            .save_file()
                        {
                            let config = self.params.build_engine_config(
                                if self.devices.selected_input_device.is_empty() { None }
                                else { Some(self.devices.selected_input_device.clone()) },
                                self.devices.selected_output_devices.iter().cloned().collect(),
                            );
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
                                max_outputs: config.max_outputs as u32,
                                ..AudioRouterConfig::default()
                            };
                            match crate::config::save_config(&file_cfg, &path) {
                                Ok(()) => {
                                    let path_str = path.to_string_lossy().to_string();
                                    self.config_path = Some(path_str.clone());
                                    self.toolbar.config_path = Some(path_str);
                                    self.log_panel.push(chrono_now(), "配置已导出".to_string());
                                }
                                Err(e) => {
                                    self.log_panel.push(chrono_now(), format!("导出配置失败: {}", e));
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
                            self.log_panel.push(chrono_now(), format!("刷新设备失败: {}", e));
                        }
                    }
                    devices::DevicesPanelAction::InputDeviceSelected(name) => {
                        self.devices.selected_input_device = name;
                    }
                    devices::DevicesPanelAction::OutputDeviceToggled(name) => {
                        if self.devices.selected_output_devices.contains(&name) {
                            self.devices.selected_output_devices.remove(&name);
                        } else {
                            self.devices.selected_output_devices.insert(name);
                        }
                    }
                    devices::DevicesPanelAction::SourceTypeChanged(source_type) => {
                        self.devices.source_type = source_type.clone();
                        if source_type == "loopback" {
                            if let Err(e) = self.devices.refresh_devices() {
                                self.log_panel.push(chrono_now(), format!("刷新设备失败: {}", e));
                            }
                        }
                    }
                    devices::DevicesPanelAction::LoopbackDeviceSelected(name) => {
                        self.devices.selected_loopback_device = name.clone();
                        if !name.is_empty() {
                            let lb_name_lower = name.to_lowercase();
                            self.devices.selected_output_devices.retain(|dev_name| {
                                let dev_lower = dev_name.to_lowercase();
                                !(dev_lower.contains(&lb_name_lower) || lb_name_lower.contains(&dev_lower))
                            });
                        }
                        if let Err(e) = self.devices.refresh_devices() {
                            self.log_panel.push(chrono_now(), format!("刷新设备失败: {}", e));
                        }
                    }
                    devices::DevicesPanelAction::None => {}
                }
            });

        // ---------- 中央区域 ----------
        egui::CentralPanel::default().show(ctx, |_ui| {});

        // ---------- 弹窗 ----------
        self.params.show_window(ctx, &mut self.toolbar.show_settings_window);
        self.log_panel.show_window(ctx, &mut self.toolbar.show_log_window);
        self.toolbar.show_status_window(ctx, &self.status_bar);

        ctx.request_repaint_after(Duration::from_millis(33));
    }

    /// 拦截窗口关闭事件，改为隐藏到托盘
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if raw_input.viewport().close_requested() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window_to_tray();
        }
    }
}

// ============================================================================
// 窗口控制辅助方法
// ============================================================================

impl AudioRouterApp {
    /// 使用 Windows ShowWindow(SW_HIDE) 隐藏窗口
    fn hide_window_to_tray(&self) {
        #[cfg(windows)]
        if let Some(hwnd) = self.hwnd {
            unsafe {
                use windows::Win32::Foundation::HWND;
                use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
                let _ = ShowWindow(HWND(hwnd as *mut core::ffi::c_void), SW_HIDE);
            }
        }
        tracing::info!("窗口已隐藏到系统托盘");
    }
}

// ============================================================================
// 公共入口函数
// ============================================================================

pub fn run_gui(config_path: Option<String>) -> crate::error::Result<()> {
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

    let (engine_to_gui_tx, engine_to_gui_rx) =
        crossbeam_channel::unbounded::<EngineToGui>();
    let (gui_to_engine_tx, gui_to_engine_rx) =
        crossbeam_channel::unbounded::<GuiToEngine>();

    crate::engine::spawn_engine(gui_to_engine_rx, engine_to_gui_tx);

    // 创建系统托盘 — 传入 engine_tx，退出时托盘线程自行停止引擎并 exit
    let tray_state = match tray::TrayState::new(gui_to_engine_tx.clone()) {
        Ok(state) => {
            tracing::info!("系统托盘图标已创建");
            Some(state)
        }
        Err(e) => {
            tracing::warn!("创建系统托盘失败: {}，程序将正常运行", e);
            None
        }
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0]),
        ..Default::default()
    };

    eframe::run_native(
        "音频路由器",
        native_options,
        Box::new(move |_cc| {
            let app = AudioRouterApp::new(
                gui_to_engine_tx,
                engine_to_gui_rx,
                config_path.clone(),
                &file_config,
                tray_state,
            );
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| crate::error::AudioRouterError::Fatal(format!("GUI 启动失败: {}", e)))?;

    Ok(())
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 使用原生 FFI FindWindowW 搜索窗口句柄
#[cfg(windows)]
fn find_window_hwnd() -> Option<isize> {
    extern "system" {
        fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
    }
    let title: Vec<u16> = "音频路由器\0".encode_utf16().collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd == 0 { None } else { Some(hwnd) }
    }
}

#[cfg(not(windows))]
fn find_window_hwnd() -> Option<isize> { None }

fn load_chinese_font(ctx: &egui::Context) {
    let font_paths = [
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ];

    for path in &font_paths {
        match std::fs::read(path) {
            Ok(data) => {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert("chinese".to_owned(), egui::FontData::from_owned(data).into());
                for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
                    fonts.families.entry(family).or_default().insert(0, "chinese".to_owned());
                }
                ctx.set_fonts(fonts);
                return;
            }
            Err(_) => continue,
        }
    }
}

fn chrono_now() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    format!("{:02}:{:02}:{:02}", (secs / 3600) % 24, (secs / 60) % 60, secs % 60)
}

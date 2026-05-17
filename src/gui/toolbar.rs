// 顶部工具栏面板 — 引擎状态指示与控制操作
// 包含：红绿状态环（可点击弹窗）、启动/停止按钮、日志按钮、导入/导出配置

use crate::gui::status::{EngineStatus, StatusBarState};
use egui::{Color32, Ui, Vec2};

/// 工具栏按钮操作枚举
/// 由 show() 方法返回，供上层处理对应的业务逻辑
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolbarAction {
    /// 无操作（默认返回值）
    None,
    /// 启动引擎
    Start,
    /// 停止引擎
    Stop,
    /// 导入配置文件
    ImportConfig,
    /// 导出配置文件
    ExportConfig,
}

/// 工具栏内部状态
pub struct ToolbarState {
    /// 引擎是否正在运行
    pub engine_running: bool,
    /// 引擎是否处于错误状态
    pub engine_error: bool,
    /// 当前配置文件的路径（可选）
    pub config_path: Option<String>,
    /// 是否显示日志弹窗
    pub show_log_window: bool,
    /// 是否显示状态弹窗
    pub show_status_window: bool,
}

impl ToolbarState {
    /// 创建默认的工具栏状态
    /// 初始状态：引擎停止、无错误、无配置文件路径、弹窗关闭
    pub fn new() -> Self {
        Self {
            engine_running: false,
            engine_error: false,
            config_path: None,
            show_log_window: false,
            show_status_window: false,
        }
    }

    /// 在指定的 Ui 中绘制工具栏面板
    /// 返回用户点击产生的操作，无操作时返回 ToolbarAction::None
    pub fn show(&mut self, ui: &mut Ui) -> ToolbarAction {
        let mut action = ToolbarAction::None;

        // 使用更大的内边距和垂直居中布局
        ui.vertical_centered(|ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                // ======================== 左侧 — 状态环（可点击） ========================
                // 根据引擎状态决定颜色：绿色=运行中 / 灰色=已停止 / 红色=错误
                let (ring_color, status_text) = if self.engine_error {
                    (Color32::RED, "错误")
                } else if self.engine_running {
                    (Color32::GREEN, "运行中")
                } else {
                    (Color32::GRAY, "已停止")
                };

                // 绘制可点击的状态环
                let ring_size = 22.0;
                let (rect, response) = ui.allocate_exact_size(
                    Vec2::splat(ring_size),
                    egui::Sense::click(),
                );

                if ui.is_rect_visible(rect) {
                    // 绘制外环
                    ui.painter().circle_stroke(
                        rect.center(),
                        ring_size / 2.0 - 1.0,
                        egui::Stroke::new(3.0, ring_color),
                    );
                    // 绘制内部填充圆点
                    ui.painter().circle_filled(
                        rect.center(),
                        ring_size / 4.0,
                        ring_color,
                    );
                }

                // 点击状态环时打开状态弹窗
                if response.clicked() {
                    self.show_status_window = true;
                }

                // 鼠标悬停提示
                response.on_hover_text("点击查看引擎状态详情");

                // 状态文本
                ui.label(
                    egui::RichText::new(status_text)
                        .size(14.0)
                        .color(ring_color),
                );

                // 分隔间距
                ui.add_space(16.0);

                // ======================== 中间 — 启动 / 停止按钮 ========================
                if !self.engine_running {
                    // 引擎未运行时显示启动按钮
                    if ui.button(
                        egui::RichText::new("\u{25B6} 启动").size(15.0),
                    ).clicked() {
                        action = ToolbarAction::Start;
                    }
                } else {
                    // 引擎运行时显示停止按钮
                    if ui.button(
                        egui::RichText::new("\u{25A0} 停止").size(15.0),
                    ).clicked() {
                        action = ToolbarAction::Stop;
                    }
                }

                // 分隔间距
                ui.add_space(8.0);

                // ======================== 日志按钮 ========================
                if ui.button(
                    egui::RichText::new("\u{1F4CB} 日志").size(15.0),
                ).clicked() {
                    self.show_log_window = true;
                }

                // 弹性空间，将右侧按钮推到最右
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        // ======================== 右侧 — 配置操作按钮 ========================
                        // 导出配置按钮
                        if ui.button(
                            egui::RichText::new("\u{1F4BE} 导出配置").size(14.0),
                        ).clicked() {
                            action = ToolbarAction::ExportConfig;
                        }
                        // 导入配置按钮
                        if ui.button(
                            egui::RichText::new("\u{1F4C2} 导入配置").size(14.0),
                        ).clicked() {
                            action = ToolbarAction::ImportConfig;
                        }
                    },
                );
            });
            ui.add_space(4.0);
        });

        action
    }

    /// 渲染状态信息弹窗
    /// 点击工具栏上的状态环时弹出，显示引擎运行状态和输出设备诊断信息
    pub fn show_status_window(
        &mut self,
        ctx: &egui::Context,
        status_bar: &StatusBarState,
    ) {
        if !self.show_status_window {
            return;
        }

        egui::Window::new("引擎状态")
            .open(&mut self.show_status_window)
            .default_size([420.0, 300.0])
            .resizable(true)
            .show(ctx, |ui| {
                // ---------- 引擎状态指示 ----------
                match &status_bar.engine_status {
                    EngineStatus::Stopped => {
                        ui.colored_label(Color32::GRAY, "⚫ 引擎已停止");
                    }
                    EngineStatus::Running => {
                        ui.colored_label(Color32::GREEN, "🟢 引擎运行中");
                    }
                    EngineStatus::Error(msg) => {
                        ui.colored_label(Color32::RED, format!("🔴 错误: {}", msg));
                    }
                }

                // ---------- 输出设备诊断信息 ----------
                if !status_bar.output_statuses.is_empty() {
                    ui.separator();
                    ui.heading("输出设备状态");
                    for snap in &status_bar.output_statuses {
                        egui::Frame::group(ui.style())
                            .show(ui, |ui| {
                                ui.strong(&snap.device_name);
                                ui.label(format!(
                                    "欠载: {} | 溢出: {} | 延迟: {:.1}ms | delta: {:+} | 水位: {:.0}%",
                                    snap.underrun_count,
                                    snap.overflow_count,
                                    snap.latency_ms,
                                    snap.delta,
                                    snap.water_level_pct,
                                ));
                            });
                    }
                }

                // ---------- 系统资源使用情况 ----------
                if status_bar.show_system_usage {
                    ui.separator();
                    ui.label(format!(
                        "CPU: {:.1}% | 内存: {:.1}MB",
                        status_bar.cpu_usage, status_bar.memory_mb,
                    ));
                }
            });
    }
}

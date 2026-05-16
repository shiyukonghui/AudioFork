// 顶部工具栏面板 — 引擎状态指示与控制操作

use egui::{Color32, Ui};

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
}

impl ToolbarState {
    /// 创建默认的工具栏状态
    /// 初始状态：引擎停止、无错误、无配置文件路径
    pub fn new() -> Self {
        Self {
            engine_running: false,
            engine_error: false,
            config_path: None,
        }
    }

    /// 在指定的 Ui 中绘制工具栏面板
    /// 返回用户点击产生的操作，无操作时返回 ToolbarAction::None
    pub fn show(&mut self, ui: &mut Ui) -> ToolbarAction {
        let mut action = ToolbarAction::None;

        // 使用水平布局排列所有控件
        ui.horizontal(|ui| {
            // ======================== 左侧 — 状态指示灯 ========================
            // 根据引擎状态决定颜色：绿色=运行中 / 灰色=已停止 / 红色=错误
            let (color, status_text) = if self.engine_error {
                (Color32::RED, "错误")
            } else if self.engine_running {
                (Color32::GREEN, "运行中")
            } else {
                (Color32::GRAY, "已停止")
            };

            // 状态指示灯圆点
            ui.colored_label(color, "\u{25CF}");
            // 状态文本
            ui.label(status_text);

            // 分隔间距
            ui.add_space(16.0);

            // ======================== 中间 — 启动 / 停止按钮 ========================
            if !self.engine_running {
                // 引擎未运行时显示启动按钮
                if ui.button("\u{25B6} 启动").clicked() {
                    action = ToolbarAction::Start;
                }
            } else {
                // 引擎运行时显示停止按钮
                if ui.button("\u{25A0} 停止").clicked() {
                    action = ToolbarAction::Stop;
                }
            }

            // 弹性空间，将右侧按钮推到最右
            ui.with_layout(
                egui::Layout::right_to_left(egui::Align::Center),
                |ui| {
                    // ======================== 右侧 — 配置操作按钮 ========================
                    // 导出配置按钮
                    if ui.button("\u{1F4BE} 导出配置").clicked() {
                        action = ToolbarAction::ExportConfig;
                    }
                    // 导入配置按钮
                    if ui.button("\u{1F4C2} 导入配置").clicked() {
                        action = ToolbarAction::ImportConfig;
                    }
                },
            );
        });

        action
    }
}

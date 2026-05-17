// 底部状态栏面板模块
// 显示引擎状态、输出设备诊断信息和系统资源使用情况

use crate::message::OutputSnapshot;

// ============================================================================
// 引擎运行状态枚举
// ============================================================================

/// 表示音频路由引擎的当前运行状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineStatus {
    /// 引擎已停止，未处理音频流
    Stopped,
    /// 引擎正常运行中
    Running,
    /// 引擎发生错误，附带错误描述信息
    Error(String),
}

// ============================================================================
// 状态栏状态结构体
// ============================================================================

/// 底部状态栏的完整 UI 状态
pub struct StatusBarState {
    /// 引擎运行状态
    pub engine_status: EngineStatus,
    /// 各输出设备的诊断快照列表
    pub output_statuses: Vec<OutputSnapshot>,
    /// CPU 使用率百分比（0.0 ~ 100.0）
    pub cpu_usage: f32,
    /// 内存使用量，单位 MB
    pub memory_mb: f32,
    /// 是否显示系统资源使用信息
    pub show_system_usage: bool,
}

impl StatusBarState {
    /// 构造默认的状态栏状态
    ///
    /// 默认：引擎停止、无输出设备、CPU/内存为零、不显示系统用量
    pub fn new() -> Self {
        Self {
            engine_status: EngineStatus::Stopped,
            output_statuses: Vec::new(),
            cpu_usage: 0.0,
            memory_mb: 0.0,
            show_system_usage: false,
        }
    }

    /// 更新输出设备状态快照列表
    ///
    /// # 参数
    /// * `statuses` - 来自音频引擎的最新输出设备状态快照
    pub fn update_outputs(&mut self, statuses: Vec<OutputSnapshot>) {
        self.output_statuses = statuses;
    }

    /// 在 egui 上下文中渲染底部状态栏面板（简化版）
    ///
    /// 渲染内容包括：
    /// - 引擎状态指示器（彩色圆点 + 文本）
    /// - 输出设备诊断信息（运行时显示）
    ///
    /// # 参数
    /// * `ui` - egui 的 Ui 引用，用于布局和渲染
    #[allow(dead_code)]
    pub fn show(&mut self, ui: &mut egui::Ui) {
        // 使用 Frame::group 包裹整体，背景色区别于主编辑区
        egui::Frame::group(ui.style())
            .show(ui, |ui| {
                // ---------- 状态指示行 ----------
                ui.horizontal(|ui| {
                    match &self.engine_status {
                        EngineStatus::Stopped => {
                            // 灰色圆点 + 停止文本
                            ui.colored_label(
                                egui::Color32::GRAY,
                                "⚫ 引擎已停止",
                            );
                        }
                        EngineStatus::Running => {
                            // 绿色圆点 + 运行文本
                            ui.colored_label(
                                egui::Color32::GREEN,
                                "🟢 引擎运行中",
                            );
                            // 显示输出设备数量
                            if !self.output_statuses.is_empty() {
                                ui.label(format!(
                                    " | {} 个输出设备",
                                    self.output_statuses.len()
                                ));
                            }
                        }
                        EngineStatus::Error(msg) => {
                            // 红色圆点 + 错误信息
                            ui.colored_label(
                                egui::Color32::RED,
                                format!("🔴 错误: {}", msg),
                            );
                        }
                    }
                });
            });
    }
}
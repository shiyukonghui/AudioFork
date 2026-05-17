// 日志/事件区域面板 — 管理运行时日志条目的显示与交互

/// 日志面板：收集并展示运行时日志条目，支持弹窗视图和条目上限
pub struct LogPanel {
    /// 日志条目列表，每条为 (时间戳, 日志内容) 元组
    entries: Vec<(String, String)>,
}

impl LogPanel {
    /// 创建一个空的日志面板
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 追加一条日志条目到面板末尾
    /// 当条目数超过 100 条上限时，自动移除最早的一条
    pub fn push(&mut self, timestamp: String, msg: String) {
        self.entries.push((timestamp, msg));
        if self.entries.len() > 100 {
            self.entries.remove(0);
        }
    }

    /// 清空所有日志条目
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// 在独立弹窗中渲染日志面板
    /// 使用 egui::Window 创建可拖动、可调整大小的弹窗
    ///
    /// # 参数
    /// * `ctx` - egui 上下文引用
    /// * `show` - 控制弹窗显示状态的布尔引用，关闭时自动设为 false
    pub fn show_window(&mut self, ctx: &egui::Context, show: &mut bool) {
        if !*show {
            return;
        }

        egui::Window::new("运行日志")
            .open(show)
            .default_size([550.0, 400.0])
            .resizable(true)
            .show(ctx, |ui| {
                // 顶部工具栏：清空按钮 + 日志计数
                ui.horizontal(|ui| {
                    if ui.button("清空").clicked() {
                        self.clear();
                    }
                    ui.label(format!("共 {} 条日志", self.entries.len()));
                });

                ui.separator();

                // 可滚动日志列表
                egui::ScrollArea::vertical()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        for (ts, msg) in &self.entries {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(ts)
                                        .color(egui::Color32::GRAY)
                                        .size(12.0),
                                );
                                ui.label(msg);
                            });
                        }
                    });
            });
    }
}

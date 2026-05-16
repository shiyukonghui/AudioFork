// 日志/事件区域面板 — 管理运行时日志条目的显示与交互

/// 日志面板：收集并展示运行时日志条目，支持可折叠视图和条目上限
pub struct LogPanel {
    /// 日志条目列表，每条为 (时间戳, 日志内容) 元组
    entries: Vec<(String, String)>,
    /// 面板初始折叠状态，true 表示默认折叠
    collapsed: bool,
}

impl LogPanel {
    /// 创建一个空的日志面板，默认展开
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            collapsed: false,
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

    /// 在 egui UI 中渲染日志面板
    /// 使用可折叠标题区域：
    /// - 展开时：显示清空按钮和带滚动条的日志列表
    /// - 折叠时：显示日志总条数
    pub fn show(&mut self, ui: &mut egui::Ui) {
        // 创建可折叠标题，默认行为由 collapsed 字段控制
        let response = egui::CollapsingHeader::new("日志")
            .default_open(!self.collapsed)
            .show(ui, |ui| {
                // 展开状态：显示清空按钮
                if ui.button("清空").clicked() {
                    self.clear();
                }

                // 可滚动区域包含所有日志条目
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (ts, msg) in &self.entries {
                        ui.label(format!("{} {}", ts, msg));
                        ui.separator();
                    }
                });
            });

        // 折叠状态：在标题下方显示日志总条数
        if response.body_returned.is_none() {
            ui.label(format!("共 {} 条日志", self.entries.len()));
        }
    }
}

# GUI 工具栏优化计划

## 需求概述

优化音频路由器界面布局：
1. 增大顶部工具栏的高度和按钮尺寸
2. 将日志面板改为工具栏按钮弹窗显示
3. 将引擎运行状态通过红绿环展示，点击弹窗显示状态信息
4. 移除底部日志面板区域

## 当前布局分析

### 现有组件结构
- **顶部工具栏** (`toolbar.rs`): 状态指示灯 + 启动/停止按钮 + 导入/导出配置
- **左侧设备面板** (`devices.rs`): 音源类型、输入/输出设备选择
- **右侧参数面板** (`params.rs`): 音频参数配置
- **中央区域**: 空白占位
- **底部状态栏** (`status.rs`): 引擎状态 + 输出设备诊断
- **底部日志面板** (`logs.rs`): 可折叠日志列表

### 现有消息流
- `EngineToGui::Log(msg)` → `log_panel.push()`
- `EngineToGui::OutputStatus(statuses)` → `status_bar.update_outputs()`

## 实施步骤

### 步骤 1: 增大工具栏尺寸

**文件**: `src/gui/toolbar.rs`

修改内容：
1. 增加工具栏面板的默认高度（通过 `egui::TopBottomPanel::top().height_range()`)
2. 增大按钮的字体大小和内边距
3. 增大状态指示灯的尺寸

### 步骤 2: 添加日志弹窗功能

**文件**: `src/gui/toolbar.rs`

修改内容：
1. 在 `ToolbarState` 中添加 `show_log_window: bool` 字段
2. 添加 `ToolbarAction::ShowLogWindow` 枚举变体
3. 在工具栏中添加"日志"按钮
4. 实现 `show_log_window()` 方法，使用 `egui::Window` 渲染日志弹窗

**文件**: `src/gui/logs.rs`

修改内容：
1. 添加 `show_window()` 方法，用于在弹窗中渲染日志内容
2. 保持原有的 `push()` 和 `clear()` 方法不变

### 步骤 3: 添加状态环弹窗功能

**文件**: `src/gui/toolbar.rs`

修改内容：
1. 在 `ToolbarState` 中添加 `show_status_window: bool` 字段
2. 将状态指示灯改为可点击的红绿环（使用 `ui.interact()` 检测点击）
3. 实现 `show_status_window()` 方法，显示引擎状态和输出设备诊断信息
4. 从 `status_bar` 获取状态信息用于显示

### 步骤 4: 重构主布局

**文件**: `src/gui/mod.rs`

修改内容：
1. 移除底部日志面板 (`egui::TopBottomPanel::bottom("log_area")`)
2. 在工具栏渲染后调用日志弹窗和状态弹窗的渲染方法
3. 将 `status_bar` 的状态信息传递给工具栏用于弹窗显示
4. 调整中央区域布局，使其填充更多空间

### 步骤 5: 更新状态栏

**文件**: `src/gui/status.rs`

修改内容：
1. 简化状态栏显示（可选保留或移除）
2. 添加 `get_status_info()` 方法，返回状态信息供工具栏弹窗使用

## 详细代码修改

### 1. toolbar.rs 修改

```rust
// 新增枚举变体
pub enum ToolbarAction {
    None,
    Start,
    Stop,
    ImportConfig,
    ExportConfig,
    ShowLogWindow,    // 新增
    ShowStatusWindow, // 新增
}

// 扩展 ToolbarState
pub struct ToolbarState {
    pub engine_running: bool,
    pub engine_error: bool,
    pub config_path: Option<String>,
    pub show_log_window: bool,      // 新增
    pub show_status_window: bool,  // 新增
    pub status_info: StatusInfo,    // 新增：状态信息
}

// 新增状态信息结构
pub struct StatusInfo {
    pub engine_status: String,
    pub output_statuses: Vec<OutputSnapshot>,
}
```

### 2. logs.rs 修改

```rust
impl LogPanel {
    // 新增方法：在弹窗中渲染
    pub fn show_window(&mut self, ctx: &egui::Context, show: &mut bool) {
        if !*show {
            return;
        }
        
        egui::Window::new("运行日志")
            .open(show)
            .default_size([500.0, 400.0])
            .show(ctx, |ui| {
                // 清空按钮
                if ui.button("清空").clicked() {
                    self.clear();
                }
                
                ui.separator();
                
                // 可滚动日志列表
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (ts, msg) in &self.entries {
                        ui.label(format!("{} {}", ts, msg));
                    }
                });
            });
    }
}
```

### 3. mod.rs 布局修改

```rust
// 移除底部日志面板
// 原: egui::TopBottomPanel::bottom("log_area")...

// 在 update() 末尾添加弹窗渲染
self.log_panel.show_window(ctx, &mut self.toolbar.show_log_window);
self.toolbar.show_status_window(ctx, &mut self.toolbar.show_status_window, &self.status_bar);
```

## 文件修改清单

| 文件 | 修改类型 | 说明 |
|------|----------|------|
| `src/gui/toolbar.rs` | 大幅修改 | 增大尺寸、添加日志按钮、状态环可点击、弹窗方法 |
| `src/gui/logs.rs` | 小幅修改 | 添加 `show_window()` 方法 |
| `src/gui/mod.rs` | 中等修改 | 移除日志面板、添加弹窗渲染调用 |
| `src/gui/status.rs` | 小幅修改 | 添加获取状态信息的方法 |

## 测试要点

1. 工具栏按钮尺寸是否合适
2. 点击日志按钮是否正确弹出日志窗口
3. 点击状态环是否正确弹出状态信息窗口
4. 引擎状态变化时红绿环颜色是否正确更新
5. 日志内容是否正确显示在弹窗中
6. 窗口关闭后再次打开是否保留之前的状态

## 风险评估

- **低风险**: 日志弹窗功能实现简单，不影响现有逻辑
- **中风险**: 状态环点击交互需要正确处理事件传递
- **低风险**: 移除底部日志面板后布局自动调整

## 预估工作量

- 步骤 1: 增大工具栏尺寸 - 约 20 行代码修改
- 步骤 2: 日志弹窗功能 - 约 50 行新增代码
- 步骤 3: 状态环弹窗功能 - 约 80 行新增代码
- 步骤 4: 主布局重构 - 约 30 行代码修改
- 步骤 5: 状态栏更新 - 约 20 行代码修改

总计约 200 行代码修改/新增。

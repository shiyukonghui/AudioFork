// 设备管理面板 — 左侧面板
// 提供输入设备下拉选择、输出设备多选勾选、设备刷新等功能

use std::collections::HashSet;

use crate::device::DeviceType;
use crate::message::DeviceInfoSnapshot;

// ============================================================================
// DevicesPanelAction — 设备面板产生的用户操作
// ============================================================================

/// 设备面板用户操作枚举
/// 父组件通过匹配此枚举来响应面板交互
#[derive(Debug, Clone)]
pub enum DevicesPanelAction {
    /// 无操作
    None,
    /// 点击刷新按钮
    RefreshDevices,
    /// 选择了某个输入设备（空字符串表示系统默认）
    InputDeviceSelected(String),
    /// 切换了某个输出设备的勾选状态
    OutputDeviceToggled(String),
}

// ============================================================================
// DevicesPanelState — 设备面板运行时状态
// ============================================================================

/// 设备面板状态结构体
/// 持有设备列表、选中状态和引擎运行标志
pub struct DevicesPanelState {
    /// 输入设备快照列表
    pub input_devices: Vec<DeviceInfoSnapshot>,
    /// 输出设备快照列表
    pub output_devices: Vec<DeviceInfoSnapshot>,
    /// 当前选中的输入设备名（空字符串表示使用系统默认）
    pub selected_input_device: String,
    /// 已勾选的输出设备名集合
    pub selected_output_devices: HashSet<String>,
    /// 音频引擎是否正在运行（运行中禁用控件修改）
    pub engine_running: bool,
}

impl DevicesPanelState {
    /// 创建空的设备面板状态
    ///
    /// - 设备列表为空
    /// - 输入设备选为系统默认（空字符串）
    /// - 输出设备勾选集合为空
    /// - 引擎状态为未运行
    pub fn new() -> Self {
        Self {
            input_devices: Vec::new(),
            output_devices: Vec::new(),
            selected_input_device: String::new(),
            selected_output_devices: HashSet::new(),
            engine_running: false,
        }
    }

    // ========================================================================
    // refresh_devices — 刷新设备列表
    // ========================================================================

    /// 从系统枚举输入/输出设备并刷新面板设备列表
    ///
    /// - 调用 `crate::device::enumerate_input_devices()` 填充输入设备列表
    /// - 调用 `crate::device::enumerate_output_devices()` 填充输出设备列表
    /// - 首次刷新时（输出设备列表为空），默认勾选所有输出设备
    /// - 枚举失败时返回 `Err(String)` 描述具体错误原因
    pub fn refresh_devices(&mut self) -> Result<(), String> {
        // 枚举输入设备并转为 DeviceInfoSnapshot 快照
        let inputs = crate::device::enumerate_input_devices()
            .map_err(|e| format!("枚举输入设备失败: {}", e))?;
        self.input_devices = inputs.iter().map(|info| info.into()).collect();

        // 枚举输出设备并转为 DeviceInfoSnapshot 快照
        let outputs = crate::device::enumerate_output_devices()
            .map_err(|e| format!("枚举输出设备失败: {}", e))?;

        // 记录是否为首次刷新（输出设备列表为空表示首次）
        let is_first_refresh = self.output_devices.is_empty();
        self.output_devices = outputs.iter().map(|info| info.into()).collect();

        // 首次刷新时默认勾选所有输出设备
        if is_first_refresh {
            for device in &self.output_devices {
                self.selected_output_devices.insert(device.name.clone());
            }
        }

        Ok(())
    }

    // ========================================================================
    // show — 渲染设备面板 UI
    // ========================================================================

    /// 在 egui 中渲染设备管理面板
    ///
    /// 面板内容分为两个区域：
    /// 1. **输入设备区**：下拉选择框（含"系统默认"选项）
    /// 2. **输出设备区**：checkbox 列表 + 顶部刷新按钮
    ///
    /// 当 `engine_running` 为 `true` 时，所有交互控件被禁用。
    ///
    /// 返回用户操作，父组件根据返回值执行相应逻辑。
    pub fn show(&mut self, ui: &mut egui::Ui) -> DevicesPanelAction {
        let mut action = DevicesPanelAction::None;

        // 整个面板放在可滚动区域中
        egui::ScrollArea::vertical().show(ui, |ui| {
            // ================================================================
            // 输入设备区
            // ================================================================
            ui.heading("输入设备");

            // 引擎运行中则禁用所有输入控件
            ui.add_enabled_ui(!self.engine_running, |ui| {
                // 构建下拉框当前选中文本
                let selected_text = if self.selected_input_device.is_empty() {
                    "系统默认".to_string()
                } else {
                    self.selected_input_device.clone()
                };

                // 输入设备下拉选择框
                egui::ComboBox::from_label("输入设备")
                    .selected_text(&selected_text)
                    .show_ui(ui, |ui| {
                        // "系统默认"特殊选项
                        if ui
                            .selectable_label(
                                self.selected_input_device.is_empty(),
                                "系统默认",
                            )
                            .clicked()
                        {
                            action =
                                DevicesPanelAction::InputDeviceSelected(String::new());
                        }
                        // 遍历所有输入设备生成下拉选项
                        // 格式: "设备名 [48000Hz, 2ch, f32]"
                        for device in &self.input_devices {
                            let label = format!(
                                "{} [{}Hz, {}ch, {}]",
                                device.name,
                                device.sample_rate,
                                device.channels,
                                device.format
                            );
                            let is_selected = !self.selected_input_device.is_empty()
                                && self.selected_input_device == device.name;
                            if ui.selectable_label(is_selected, &label).clicked() {
                                action = DevicesPanelAction::InputDeviceSelected(
                                    device.name.clone(),
                                );
                            }
                        }
                    });
            });

            ui.separator();

            // ================================================================
            // 输出设备区
            // ================================================================
            // 标题行："输出设备" + 右侧刷新按钮
            ui.horizontal(|ui| {
                ui.heading("输出设备");
                ui.with_layout(
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if ui.button("刷新").clicked() {
                            action = DevicesPanelAction::RefreshDevices;
                        }
                    },
                );
            });

            // 引擎运行中则禁用所有输出设备控件
            ui.add_enabled_ui(!self.engine_running, |ui| {
                // 遍历每个输出设备绘制 checkbox 行
                // 格式: "设备名 — 48000Hz 2ch [直通]"
                // 蓝牙设备追加 " ⚠ 高延迟"
                for device in &self.output_devices {
                    let mut checked =
                        self.selected_output_devices.contains(&device.name);

                    // 构建设备显示标签
                    let mut label = format!(
                        "{} — {}Hz {}ch [直通]",
                        device.name, device.sample_rate, device.channels
                    );

                    // 蓝牙设备追加高延迟警告
                    if matches!(device.device_type, DeviceType::Bluetooth) {
                        label.push_str(" ⚠ 高延迟");
                    }

                    // checkbox 状态变更时产生 OutputDeviceToggled 操作
                    if ui.checkbox(&mut checked, &label).changed() {
                        action = DevicesPanelAction::OutputDeviceToggled(
                            device.name.clone(),
                        );
                    }
                }
            });
        });

        action
    }
}

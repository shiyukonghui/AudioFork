// 音频路由器 — 参数配置面板（GUI 右侧面板）
// 提供音频参数、重采样、高级选项和输入丢失行为的配置界面

use crate::message::EngineConfig;

/// 参数面板运行时状态
/// 存储用户可调整的所有音频引擎参数
pub struct ParamsPanelState {
    /// 缓冲区帧数（32 ~ 4096）
    pub buffer_frames: u32,
    /// 最大延迟限制（毫秒）
    pub max_latency_ms: u32,
    /// 采样率（Hz）
    pub sample_rate: u32,
    /// 重采样算法索引：0 = Sinc, 1 = Cubic, 2 = None
    pub resampler_index: usize,
    /// 是否禁用时钟漂移补偿
    pub no_drift_compensation: bool,
    /// 是否启用 WASAPI 独占模式
    pub wasapi_exclusive: bool,
    /// 输入设备丢失时是否立即退出
    pub exit_on_input_loss: bool,
    /// 输入设备丢失时是否降级到默认设备
    pub input_fallback_to_default: bool,
    /// 是否禁用砖墙限幅器
    pub no_limiter: bool,
    /// 音频源类型："input" / "loopback"
    pub source_type: String,
    /// 回环捕获设备名称，None 表示使用默认回环设备
    pub loopback_device: Option<String>,
    /// 最大输出设备数量限制
    pub max_outputs: usize,
    /// 引擎是否正在运行（运行中锁定参数编辑）
    pub engine_running: bool,
}

impl ParamsPanelState {
    /// 创建带有默认值的参数面板状态
    /// 默认：buffer_frames=256, max_latency_ms=30, sample_rate=48000,
    ///       resampler_index=0 (Sinc), 所有 bool=false
    pub fn new() -> Self {
        Self {
            buffer_frames: 256,
            max_latency_ms: 30,
            sample_rate: 48000,
            resampler_index: 0, // 默认 Sinc 算法
            no_drift_compensation: false,
            wasapi_exclusive: false,
            exit_on_input_loss: false,
            input_fallback_to_default: false,
            no_limiter: false,
            source_type: "input".to_string(),
            loopback_device: None,
            max_outputs: 32,
            engine_running: false,
        }
    }

    /// 从 EngineConfig 加载配置到面板状态
    /// 将引擎配置的各个字段映射到面板控件对应的状态字段
    pub fn load_from_config(&mut self, config: &EngineConfig) {
        self.buffer_frames = config.buffer_frames;
        self.max_latency_ms = config.max_latency_ms;
        self.sample_rate = config.sample_rate;
        // 将重采样算法字符串映射为索引
        self.resampler_index = match config.resampler.as_str() {
            "sinc" => 0,
            "cubic" => 1,
            "none" => 2,
            _ => 0, // 未知值默认回退到 Sinc
        };
        self.no_drift_compensation = config.no_drift_compensation;
        self.wasapi_exclusive = config.wasapi_exclusive;
        self.exit_on_input_loss = config.exit_on_input_loss;
        self.input_fallback_to_default = config.input_fallback_to_default;
        self.no_limiter = config.no_limiter;
        self.source_type = config.source_type.clone();
        self.loopback_device = config.loopback_device.clone();
        self.max_outputs = config.max_outputs;
    }

    /// 根据面板当前状态构建 EngineConfig
    /// 将面板控件状态反向映射为引擎启动配置
    pub fn build_engine_config(
        &self,
        input_device: Option<String>,
        output_devices: Vec<String>,
    ) -> EngineConfig {
        // 将重采样索引映射回算法名称字符串
        let resampler = match self.resampler_index {
            0 => "sinc".to_string(),
            1 => "cubic".to_string(),
            2 => "none".to_string(),
            _ => "sinc".to_string(), // 未知索引默认回退到 Sinc
        };

        EngineConfig {
            input_device,
            output_devices,
            sample_rate: self.sample_rate,
            buffer_frames: self.buffer_frames,
            max_latency_ms: self.max_latency_ms,
            resampler,
            no_drift_compensation: self.no_drift_compensation,
            exit_on_input_loss: self.exit_on_input_loss,
            input_fallback_to_default: self.input_fallback_to_default,
            wasapi_exclusive: self.wasapi_exclusive,
            no_limiter: self.no_limiter,
            source_type: self.source_type.clone(),
            loopback_device: self.loopback_device.clone(),
            max_outputs: self.max_outputs,
        }
    }

    /// 在弹窗中渲染参数配置面板
    ///
    /// # 参数
    /// * `ctx` - egui 上下文引用
    /// * `show` - 控制弹窗显示状态的布尔引用
    pub fn show_window(&mut self, ctx: &egui::Context, show: &mut bool) {
        if !*show {
            return;
        }

        egui::Window::new("引擎设置")
            .open(show)
            .default_size([380.0, 500.0])
            .resizable(true)
            .show(ctx, |ui| {
                self.render_params(ui);
            });
    }

    /// 渲染参数配置面板 UI
    /// 使用 egui::ScrollArea 包裹，分段显示音频参数、重采样、高级选项和输入丢失行为
    #[allow(dead_code)]
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.render_params(ui);
    }

    /// 内部渲染方法：绘制所有参数控件
    fn render_params(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().show(ui, |ui| {
            // 引擎运行时禁用所有控件编辑
            ui.add_enabled_ui(!self.engine_running, |ui| {

            // ==================== 音频参数 ====================
            ui.heading("音频参数");

            // 缓冲区帧数滑块
            ui.add(
                egui::Slider::new(&mut self.buffer_frames, 32..=4096)
                    .text("缓冲区帧数"),
            );
            // 显示当前缓冲区对应的延迟
            let latency_ms =
                self.buffer_frames as f64 * 1000.0 / self.sample_rate as f64;
            ui.label(format!("对应延迟: {:.1}ms", latency_ms));

            // 最大延迟限制
            ui.horizontal(|ui| {
                ui.label("最大延迟限制");
                ui.add(
                    egui::DragValue::new(&mut self.max_latency_ms)
                        .range(1..=200)
                        .suffix("ms"),
                );
            });

            // ==================== 重采样 ====================
            ui.heading("重采样");

            // 重采样算法下拉选择框
            let resampler_labels = ["Sinc (高质量)", "Cubic (快速)", "None (不重采样)"];
            let selected_text = resampler_labels[self.resampler_index];
            egui::ComboBox::from_label("算法")
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for (i, label) in resampler_labels.iter().enumerate() {
                        ui.selectable_value(
                            &mut self.resampler_index,
                            i,
                            *label,
                        );
                    }
                });

            // ==================== 高级选项 ====================
            ui.heading("高级选项");

            ui.checkbox(
                &mut self.no_drift_compensation,
                "禁用时钟漂移补偿",
            );
            ui.checkbox(&mut self.no_limiter, "禁用砖墙限幅器");

            // WASAPI 独占模式仅在 feature 启用时显示
            #[cfg(feature = "wasapi-exclusive")]
            {
                ui.checkbox(
                    &mut self.wasapi_exclusive,
                    "WASAPI 独占模式（Windows）",
                );
            }

            // 最大输出设备数
            ui.horizontal(|ui| {
                ui.label("最大输出设备数");
                ui.add(
                    egui::DragValue::new(&mut self.max_outputs)
                        .range(1..=u32::MAX as usize)
                        .speed(1),
                );
            });

            // ==================== 输入设备丢失行为 ====================
            ui.heading("输入设备丢失行为");

            // exit_on_input_loss 和 input_fallback_to_default 互斥
            if ui
                .radio_value(&mut self.exit_on_input_loss, true, "立即退出")
                .clicked()
                && self.exit_on_input_loss
            {
                self.input_fallback_to_default = false;
            }
            if ui
                .radio_value(
                    &mut self.input_fallback_to_default,
                    true,
                    "30秒后切换为默认设备",
                )
                .clicked()
                && self.input_fallback_to_default
            {
                self.exit_on_input_loss = false;
            }
            });
        });
    }
}

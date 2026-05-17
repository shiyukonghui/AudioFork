// 音频路由器 — CLI 参数定义与解析模块
// 使用 clap derive 模式定义命令行参数，并与配置文件合并生成最终运行配置

use clap::Parser;

/// 音频路由器命令行参数结构体
///
/// 所有参数均支持从命令行传入，命令行参数的优先级高于配置文件
#[derive(Debug, Clone, Parser)]
#[command(
    name = "audio_router",
    about = "高性能音频路由工具，支持多输出设备分发与时钟漂移补偿",
    version
)]
pub struct CliArgs {
    /// 指定输入设备名称，若不指定则使用系统默认输入设备
    #[arg(long = "input-device")]
    pub input_device: Option<String>,

    /// 指定输出设备名称，可多次指定以路由到多个输出设备
    #[arg(long = "output-device", action = clap::ArgAction::Append)]
    pub output_device: Vec<String>,

    /// 强制指定输入采样率（Hz），若不指定则使用设备默认值
    #[arg(long = "sample-rate")]
    pub sample_rate: Option<u32>,

    /// 强制指定输入声道数，若不指定则使用设备默认值
    #[arg(long = "channels")]
    pub channels: Option<u16>,

    /// 音频缓冲区帧数，影响延迟与稳定性，默认 256
    #[arg(long = "buffer-frames")]
    pub buffer_frames: Option<u32>,

    /// 最大允许延迟（毫秒），超过此值将触发告警，默认 30
    #[arg(long = "max-latency-ms")]
    pub max_latency_ms: Option<u32>,

    /// 重采样算法: sinc（高质量）/ cubic（快速）/ none（不做重采样），默认 sinc
    #[arg(long = "resampler")]
    pub resampler: Option<String>,

    /// 禁用时钟漂移补偿，有助于降低 CPU 占用但可能导致音质下降
    #[arg(long = "no-drift-compensation", default_value_t = false)]
    pub no_drift_compensation: bool,

    /// 输入设备丢失或出错时立即退出进程
    #[arg(long = "exit-on-input-loss", default_value_t = false)]
    pub exit_on_input_loss: bool,

    /// 输入设备丢失 30 秒后自动切换为系统默认输入设备继续运行
    #[arg(long = "input-fallback-to-default", default_value_t = false)]
    pub input_fallback_to_default: bool,

    /// 日志文件输出路径，若不指定则不输出日志文件
    #[arg(long = "log-file")]
    pub log_file: Option<String>,

    /// 启用 JSON 行格式的统计信息输出，供外部监控工具集成
    #[arg(long = "monitor", default_value_t = false, conflicts_with = "gui")]
    pub monitor: bool,

    /// WASAPI 独占模式（Windows），可降低延迟但独占音频设备
    #[cfg(feature = "wasapi-exclusive")]
    #[arg(long = "wasapi-exclusive", default_value_t = false)]
    pub wasapi_exclusive: bool,

    /// 禁用砖墙限幅器，允许削波（默认启用限幅器）
    #[arg(long = "no-limiter", default_value_t = false)]
    pub no_limiter: bool,

    /// 启动图形用户界面模式，与 --monitor 互斥
    #[arg(long = "gui", default_value_t = false, conflicts_with = "monitor")]
    pub gui: bool,

    /// 配置文件路径，默认读取当前目录下的 audio_router.toml
    #[arg(long = "config", default_value = "audio_router.toml")]
    pub config: Option<String>,

    /// 音源类型: "input"（物理输入设备, 默认）或 "loopback"（系统音频回采）
    #[arg(long = "source-type", default_value = "input")]
    pub source_type: String,

    /// Loopback 模式下的回采目标输出设备名称，不指定则使用系统默认输出设备
    #[arg(long = "loopback-device")]
    pub loopback_device: Option<String>,

    /// 预设配置文件路径，从指定 TOML 文件导入预设配置
    #[arg(long = "preset")]
    pub preset: Option<String>,

    /// 最大输出设备数量（槽位上限），默认 32
    #[arg(long = "max-outputs")]
    pub max_outputs: Option<u32>,
}

/// 最终解析后的运行配置
///
/// 所有 Option 字段已在合并阶段消解为具体值，
/// 可直接用于音频路由引擎的初始化
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    /// 输入设备名称，None 表示使用系统默认
    pub input_device: Option<String>,
    /// 输出设备名称列表
    pub output_devices: Vec<String>,
    /// 采样率（Hz），None 表示使用设备默认值
    pub sample_rate: Option<u32>,
    /// 声道数，None 表示使用设备默认值
    pub channels: Option<u16>,
    /// 缓冲区帧数，已消解为具体数值（默认 256）
    pub buffer_frames: u32,
    /// 最大允许延迟（毫秒），已消解为具体数值（默认 30）
    pub max_latency_ms: u32,
    /// 重采样算法，已消解为具体字符串（默认 "sinc"）
    pub resampler: String,
    /// 是否禁用时钟漂移补偿
    pub no_drift_compensation: bool,
    /// 输入设备丢失时是否退出进程
    pub exit_on_input_loss: bool,
    /// 输入设备丢失后是否降级使用默认设备
    pub input_fallback_to_default: bool,
    /// 日志文件路径，None 表示不输出日志
    #[allow(dead_code)]
    pub log_file: Option<String>,
    /// 是否启用 JSON 监控输出
    pub monitor: bool,
    /// 是否启用 WASAPI 独占模式（仅当 wasapi-exclusive feature 启用时可用）
    #[cfg(feature = "wasapi-exclusive")]
    pub wasapi_exclusive: bool,
    /// 是否禁用砖墙限幅器
    pub no_limiter: bool,
    /// 是否启动图形界面模式
    pub gui: bool,
    /// 配置文件路径，None 表示未指定
    pub config_path: Option<String>,
    /// 音源类型
    pub source_type: String,
    /// Loopback 回采设备
    pub loopback_device: Option<String>,
    /// 预设配置路径，None 表示未导入预设
    #[allow(dead_code)]
    pub preset_path: Option<String>,
    /// 最大输出设备数量限制，默认 32
    pub max_outputs: u32,
}

impl CliArgs {
    /// 判断是否传入了音频路由操作参数（排除 --gui / --config / --log-file）
    ///
    /// 当用户通过命令行传入了与音频处理相关的参数时返回 true，
    /// 这些参数表明用户意图以 CLI 模式运行而非 GUI 模式。
    pub fn has_operational_args(&self) -> bool {
        self.input_device.is_some()
            || !self.output_device.is_empty()
            || self.sample_rate.is_some()
            || self.channels.is_some()
            || self.buffer_frames.is_some()
            || self.max_latency_ms.is_some()
            || self.resampler.is_some()
            || self.no_drift_compensation
            || self.exit_on_input_loss
            || self.input_fallback_to_default
            || self.monitor
            || self.no_limiter
            || self.source_type != "input"
            || self.loopback_device.is_some()
            || self.preset.is_some()
            || self.max_outputs.is_some()
        // 排除 --config（meta 参数）、--log-file（正交参数）、--gui（GUI 标识）
    }

    /// 将命令行参数与配置文件合并，生成最终运行配置
    ///
    /// 合并规则：
    /// - 命令行参数优先级高于配置文件
    /// - 对于 Option 字段：CLI 有值则使用 CLI 值，否则使用配置文件值
    /// - 对于 bool 字段：CLI 标志为 true 时覆盖配置文件，否则沿用配置文件值
    /// - 数值型字段消解默认值：buffer_frames 默认 256，max_latency_ms 默认 30
    /// - resampler 默认 "sinc"
    pub fn merge_with_config(&self, config: &crate::config::AudioRouterConfig) -> ResolvedConfig {
        ResolvedConfig {
            // 输入设备：CLI 优先，否则用配置文件
            input_device: self
                .input_device
                .clone()
                .or_else(|| config.input_device.clone()),

            // 输出设备：CLI 有指定就用 CLI，否则用配置文件
            output_devices: if !self.output_device.is_empty() {
                self.output_device.clone()
            } else {
                config.output_devices.clone()
            },

            // 采样率：CLI 优先
            sample_rate: self.sample_rate.or(config.sample_rate),

            // 声道数：CLI 优先
            channels: self.channels.or(config.channels),

            // 缓冲区帧数：CLI 优先，配置文件次之，默认 256
            buffer_frames: self
                .buffer_frames
                .or(config.buffer_frames)
                .unwrap_or(256),

            // 最大延迟：CLI 优先，配置文件次之，默认 30
            max_latency_ms: self
                .max_latency_ms
                .or(config.max_latency_ms)
                .unwrap_or(30),

            // 重采样算法：CLI 优先，配置文件次之，默认 "sinc"
            resampler: self
                .resampler
                .clone()
                .or_else(|| {
                    let cfg = &config.resampler;
                    if cfg.is_empty() {
                        None
                    } else {
                        Some(cfg.clone())
                    }
                })
                .unwrap_or_else(|| "sinc".to_string()),

            // 漂移补偿：CLI 为 true 则覆盖，否则用配置文件值
            no_drift_compensation: if self.no_drift_compensation {
                true
            } else {
                config.no_drift_compensation
            },

            // 输入丢失退出：CLI 标志优先
            exit_on_input_loss: if self.exit_on_input_loss {
                true
            } else {
                config.exit_on_input_loss
            },

            // 输入丢失降级默认设备：CLI 标志优先
            input_fallback_to_default: if self.input_fallback_to_default {
                true
            } else {
                config.input_fallback_to_default
            },

            // 日志文件路径：CLI 优先
            log_file: self
                .log_file
                .clone()
                .or_else(|| config.log_file.clone()),

            // 监控模式：CLI 标志优先
            monitor: if self.monitor {
                true
            } else {
                config.monitor
            },

            // WASAPI 独占模式：CLI 标志优先
            #[cfg(feature = "wasapi-exclusive")]
            wasapi_exclusive: if self.wasapi_exclusive {
                true
            } else {
                config.wasapi_exclusive
            },

            // 限幅器：CLI 为 true 则覆盖，否则用配置文件值
            no_limiter: if self.no_limiter {
                true
            } else {
                config.no_limiter
            },

            // 图形界面模式：--gui 标志最优先，否则若传入了操作参数表明用户意图 CLI，
            // 最后 fallback 到配置文件设置（默认 true，即双击 EXE 默认启动 GUI）
            gui: if self.gui {
                true
            } else if self.has_operational_args() {
                false
            } else {
                config.gui_enabled
            },

            // 配置文件路径：CLI 优先
            config_path: self.config.clone(),

            // 音源类型：CLI 提供非默认值则用 CLI，否则用配置文件
            source_type: if self.source_type != "input" || config.source_type != "input" {
                if self.source_type != "input" {
                    self.source_type.clone()
                } else {
                    config.source_type.clone()
                }
            } else {
                "input".to_string()
            },

            // Loopback 设备：CLI 优先，否则用配置文件
            loopback_device: self
                .loopback_device
                .clone()
                .or_else(|| config.loopback_device.clone()),

            // 预设配置路径：CLI 优先
            preset_path: self.preset.clone(),

            // 最大输出设备数：CLI 优先，配置文件次之，默认 32
            max_outputs: self
                .max_outputs
                .or(Some(config.max_outputs))
                .unwrap_or(32),
        }
    }

    /// 判断当前是否为 GUI 图形界面模式
    ///
    /// 返回 true 表示用户请求启动图形界面
    #[allow(dead_code)]
    pub fn is_gui_mode(&self) -> bool {
        self.gui
    }

    /// GUI 模式下过滤命令行参数，仅保留 --config 和 --gui
    ///
    /// 对于被清空的每个非空参数，输出 tracing::warn 级别日志提示用户
    /// 该方法用于在 GUI 启动前剥离音频引擎相关参数
    pub fn filter_for_gui(&self) -> Self {
        // 逐字段检查并记录警告日志
        if self.input_device.is_some() {
            tracing::warn!("GUI 模式下 --input-device 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if !self.output_device.is_empty() {
            tracing::warn!("GUI 模式下 --output-device 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.sample_rate.is_some() {
            tracing::warn!("GUI 模式下 --sample-rate 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.channels.is_some() {
            tracing::warn!("GUI 模式下 --channels 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.buffer_frames.is_some() {
            tracing::warn!("GUI 模式下 --buffer-frames 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.max_latency_ms.is_some() {
            tracing::warn!("GUI 模式下 --max-latency-ms 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.resampler.is_some() {
            tracing::warn!("GUI 模式下 --resampler 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.log_file.is_some() {
            tracing::warn!("GUI 模式下 --log-file 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.preset.is_some() {
            tracing::warn!("GUI 模式下 --preset 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.source_type != "input" {
            tracing::warn!("GUI 模式下 --source-type 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.loopback_device.is_some() {
            tracing::warn!("GUI 模式下 --loopback-device 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.max_outputs.is_some() {
            tracing::warn!("GUI 模式下 --max-outputs 参数被忽略，请通过配置文件或 GUI 界面设置");
        }

        // 对于 bool 标志：仅当被用户显式设置为 true 时才告警
        if self.no_drift_compensation {
            tracing::warn!("GUI 模式下 --no-drift-compensation 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.exit_on_input_loss {
            tracing::warn!("GUI 模式下 --exit-on-input-loss 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.input_fallback_to_default {
            tracing::warn!("GUI 模式下 --input-fallback-to-default 参数被忽略，请通过配置文件或 GUI 界面设置");
        }
        if self.monitor {
            tracing::warn!("GUI 模式下 --monitor 参数被忽略，GUI 模式与监控模式互斥");
        }
        if self.no_limiter {
            tracing::warn!("GUI 模式下 --no-limiter 参数被忽略，请通过配置文件或 GUI 界面设置");
        }

        // 构建过滤后的 CliArgs，仅保留 config 和 gui
        Self {
            input_device: None,
            output_device: Vec::new(),
            sample_rate: None,
            channels: None,
            buffer_frames: None,
            max_latency_ms: None,
            resampler: None,
            no_drift_compensation: false,
            exit_on_input_loss: false,
            input_fallback_to_default: false,
            log_file: None,
            monitor: false,
            #[cfg(feature = "wasapi-exclusive")]
            wasapi_exclusive: false,
            no_limiter: false,
            gui: self.gui,
            config: self.config.clone(),
            source_type: "input".to_string(),
            loopback_device: None,
            preset: None,
            max_outputs: None,
        }
    }
}

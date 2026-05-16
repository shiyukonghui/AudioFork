// 音频路由器 — 配置文件读写模块
// 对应开发规划，定义 AudioRouterConfig 结构体及其加载/保存方法

use serde::{Deserialize, Serialize};
use std::path::Path;

/// 音频路由器配置结构体
/// 所有字段支持 TOML 序列化与反序列化
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioRouterConfig {
    /// 输入设备名称，None 表示使用系统默认输入设备
    pub input_device: Option<String>,
    /// 输出设备名称列表，空 Vec 表示不使用任何输出设备
    pub output_devices: Vec<String>,
    /// 采样率（Hz），None 表示使用设备默认采样率
    pub sample_rate: Option<u32>,
    /// 声道数，None 表示使用设备默认声道数
    pub channels: Option<u16>,
    /// 缓冲区帧数，None 表示使用自适应大小
    pub buffer_frames: Option<u32>,
    /// 最大允许延迟（毫秒），None 表示无限制
    pub max_latency_ms: Option<u32>,
    /// 重采样算法，默认 "sinc"
    pub resampler: String,
    /// 是否禁用漂移补偿
    pub no_drift_compensation: bool,
    /// 输入设备丢失时是否退出进程
    pub exit_on_input_loss: bool,
    /// 输入设备丢失时是否降级使用系统默认输入设备，默认 true
    pub input_fallback_to_default: bool,
    /// 日志文件路径，None 表示不输出日志文件
    pub log_file: Option<String>,
    /// 是否启用 JSON 格式的监控输出（用于外部工具集成）
    pub monitor: bool,
    /// 是否启用 WASAPI 独占模式（仅 Windows）
    pub wasapi_exclusive: bool,
    /// 是否禁用砖墙限幅器，默认 false（即默认启用限幅器）
    pub no_limiter: bool,
    /// 是否启用 GUI 图形界面模式
    pub gui_enabled: bool,
}

impl Default for AudioRouterConfig {
    /// 创建带有合理默认值的配置实例
    fn default() -> Self {
        Self {
            // 所有可选字段默认为 None
            input_device: None,
            sample_rate: None,
            channels: None,
            buffer_frames: None,
            max_latency_ms: None,
            log_file: None,
            // output_devices 默认为空列表
            output_devices: Vec::new(),
            // resampler 默认使用 sinc 算法
            resampler: "sinc".to_string(),
            // 所有布尔字段默认为 false
            no_drift_compensation: false,
            exit_on_input_loss: false,
            monitor: false,
            wasapi_exclusive: false,
            no_limiter: false,
            gui_enabled: false,
            // input_fallback_to_default 默认启用降级
            input_fallback_to_default: true,
        }
    }
}

/// 从指定路径的 TOML 文件加载配置
///
/// # 参数
/// * `path` - TOML 配置文件的路径
///
/// # 返回
/// * `Ok(AudioRouterConfig)` - 成功加载的配置
/// * `Err(AudioRouterError::ConfigError)` - 文件读取或解析失败
pub fn load_config(path: &Path) -> crate::error::Result<AudioRouterConfig> {
    // 读取 TOML 文件的全部内容
    let content = std::fs::read_to_string(path).map_err(|e| {
        crate::error::AudioRouterError::ConfigError(format!(
            "无法读取配置文件 '{}': {}",
            path.display(),
            e
        ))
    })?;

    // 将 TOML 字符串反序列化为 AudioRouterConfig
    let config: AudioRouterConfig = toml::from_str(&content).map_err(|e| {
        crate::error::AudioRouterError::ConfigError(format!(
            "解析配置文件 '{}' 失败: {}",
            path.display(),
            e
        ))
    })?;

    Ok(config)
}

/// 将配置序列化并保存到指定路径的 TOML 文件
///
/// # 参数
/// * `config` - 要保存的配置引用
/// * `path` - 目标 TOML 文件路径
///
/// # 返回
/// * `Ok(())` - 保存成功
/// * `Err(AudioRouterError::ConfigError)` - 序列化或写入失败
pub fn save_config(config: &AudioRouterConfig, path: &Path) -> crate::error::Result<()> {
    // 将配置序列化为 TOML 字符串（pretty 格式，便于阅读）
    let toml_string = toml::to_string_pretty(config).map_err(|e| {
        crate::error::AudioRouterError::ConfigError(format!(
            "序列化配置失败: {}",
            e
        ))
    })?;

    // 将 TOML 字符串写入文件
    std::fs::write(path, &toml_string).map_err(|e| {
        crate::error::AudioRouterError::ConfigError(format!(
            "无法写入配置文件 '{}': {}",
            path.display(),
            e
        ))
    })?;

    Ok(())
}

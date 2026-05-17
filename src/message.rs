// 音频路由器 — 消息通道类型定义
// 提供 GUI 与后端通信所需的数据结构

use crate::device::DeviceType;

/// 设备信息快照，用于 GUI 面板展示
/// 将 DeviceInfo 中的 Vec 字段简化为单个默认值
#[derive(Debug, Clone)]
pub struct DeviceInfoSnapshot {
    /// 设备显示名称
    pub name: String,
    /// 首选采样率（Hz），取设备支持列表的第一个
    pub sample_rate: u32,
    /// 首选声道数，取设备支持列表的第一个
    pub channels: u16,
    /// 首选采样格式，取设备支持列表的第一个
    pub format: String,
    /// 设备连接类型
    pub device_type: DeviceType,
}

impl From<&crate::device::DeviceInfo> for DeviceInfoSnapshot {
    /// 从 DeviceInfo 转换为 GUI 友好的快照结构
    /// 各字段取支持列表的第一个值，无数据时使用零值/空字符串兜底
    fn from(info: &crate::device::DeviceInfo) -> Self {
        Self {
            name: info.name.clone(),
            sample_rate: info.sample_rates.first().copied().unwrap_or(0),
            channels: info.channels.first().copied().unwrap_or(0),
            format: info.formats.first().cloned().unwrap_or_default(),
            device_type: info.device_type,
        }
    }
}

// ============================================================================
// 输出设备状态诊断快照
// ============================================================================

/// 输出设备状态快照，包含诊断与监控所需的全部指标
#[derive(Debug, Clone)]
pub struct OutputSnapshot {
    /// 输出设备名称
    pub device_name: String,
    /// 欠载次数（输出缓冲区耗尽次数）
    pub underrun_count: u64,
    /// 溢出次数（输入缓冲区溢出次数）
    pub overflow_count: u64,
    /// 输出音频延迟，单位毫秒
    pub latency_ms: f64,
    /// 漂移补偿微调量（+1 / -1 / 0，控制帧数微调方向）
    pub delta: i32,
    /// 环形缓冲区水位百分比（0.0 ~ 100.0+）
    pub water_level_pct: f64,
}

/// 音频引擎启动配置
/// 由 GUI 参数面板构建，通过消息通道发送给引擎线程
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// 输入设备名称，None 表示使用系统默认设备
    pub input_device: Option<String>,
    /// 输出设备名称列表
    pub output_devices: Vec<String>,
    /// 采样率（Hz）
    pub sample_rate: u32,
    /// 缓冲区帧数
    pub buffer_frames: u32,
    /// 最大允许延迟（毫秒）
    pub max_latency_ms: u32,
    /// 重采样算法字符串："sinc" / "cubic" / "none"
    pub resampler: String,
    /// 是否禁用漂移补偿
    pub no_drift_compensation: bool,
    /// 输入设备丢失时是否退出进程
    pub exit_on_input_loss: bool,
    /// 输入设备丢失时是否降级使用默认设备
    pub input_fallback_to_default: bool,
    /// 是否启用 WASAPI 独占模式
    pub wasapi_exclusive: bool,
    /// 是否禁用砖墙限幅器
    pub no_limiter: bool,
    /// 音频源类型："input" / "loopback"
    pub source_type: String,
    /// 回环捕获设备名称，None 表示使用默认回环设备
    pub loopback_device: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            input_device: None,
            output_devices: Vec::new(),
            sample_rate: 48000,
            buffer_frames: 256,
            max_latency_ms: 30,
            resampler: "sinc".to_string(),
            no_drift_compensation: false,
            exit_on_input_loss: false,
            input_fallback_to_default: true,
            wasapi_exclusive: false,
            no_limiter: false,
            source_type: "input".to_string(),
            loopback_device: None,
        }
    }
}

// ============================================================================
// 消息通道枚举 — GUI 与引擎之间的通信协议
// ============================================================================

/// GUI 发送给音频引擎的消息
#[derive(Debug, Clone)]
pub enum GuiToEngine {
    /// 启动引擎，携带完整的引擎配置参数
    Start(EngineConfig),
    /// 停止引擎，引擎会执行淡出并返回最终统计信息
    Stop,
}

/// 音频引擎发送给 GUI 的消息
#[derive(Debug, Clone)]
pub enum EngineToGui {
    /// 引擎已成功启动
    Started,
    /// 引擎已停止，携带各输出设备在运行期间的最终统计快照
    Stopped { stats: Vec<OutputSnapshot> },
    /// 设备列表已更新（如热插拔事件），携带当前所有设备信息
    #[allow(dead_code)]
    DeviceListUpdated(Vec<DeviceInfoSnapshot>),
    /// 输出设备实时诊断状态更新
    OutputStatus(Vec<OutputSnapshot>),
    /// 引擎发生错误，携带错误描述信息
    Error(String),
    /// 引擎日志消息
    #[allow(dead_code)]
    Log(String),
}

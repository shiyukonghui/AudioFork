// 音频路由器 — 音源类型抽象与平台检测模块
// 提供 SourceType 枚举使音源可在物理输入设备和系统音频回采之间切换

/// 音源类型枚举
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceType {
    /// 物理输入设备（麦克风/Line-in）
    InputDevice,
    /// 系统音频回采（Loopback）
    Loopback,
}

impl SourceType {
    /// 从字符串解析音源类型
    /// "loopback" → Loopback，其余 → InputDevice（默认）
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "loopback" => SourceType::Loopback,
            _ => SourceType::InputDevice,
        }
    }

    /// 转为静态字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            SourceType::InputDevice => "input",
            SourceType::Loopback => "loopback",
        }
    }

    /// 是否为 Loopback 模式
    #[allow(dead_code)]
    pub fn is_loopback(&self) -> bool {
        matches!(self, SourceType::Loopback)
    }
}

impl Default for SourceType {
    fn default() -> Self {
        SourceType::InputDevice
    }
}

/// 检测当前平台是否原生支持 Loopback 音频回采
/// Windows: WASAPI 原生支持 loopback 模式 → true
/// 其他平台: 需安装虚拟声卡 → false
pub fn is_loopback_native_supported() -> bool {
    cfg!(target_os = "windows")
}

/// 返回非 Windows 平台的 Loopback 不支持引导提示文本
pub fn loopback_unsupported_message() -> &'static str {
    "当前平台不支持直接 Loopback 音频回采。\n\
     解决方案：\n\
     - macOS: 安装 BlackHole 虚拟声卡 (https://github.com/ExistentialAudio/BlackHole)\n\
     - Linux: 使用 PulseAudio monitor source 或 PipeWire"
}

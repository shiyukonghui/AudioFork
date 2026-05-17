// 音频路由器 — 统一错误类型定义
// 对应开发规划 1.5 节，定义 AudioRouterError、ErrorSeverity、RecoveryState

use std::fmt;
use std::time::Duration;

/// 统一错误类型
#[derive(Debug)]
pub enum AudioRouterError {
    /// 设备未找到
    DeviceNotFound(String),
    /// 音频流错误（输入/输出）
    StreamError(String),
    /// 配置/参数错误
    ConfigError(String),
    /// 致命错误，立即退出
    Fatal(String),
    /// 消息通道错误
    ChannelError(String),
    /// 功能不支持（如非 Windows 平台 Loopback）
    NotSupported(String),
}

impl fmt::Display for AudioRouterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceNotFound(msg) => write!(f, "设备未找到: {}", msg),
            Self::StreamError(msg) => write!(f, "音频流错误: {}", msg),
            Self::ConfigError(msg) => write!(f, "配置错误: {}", msg),
            Self::Fatal(msg) => write!(f, "致命错误: {}", msg),
            Self::ChannelError(msg) => write!(f, "消息通道错误: {}", msg),
            Self::NotSupported(msg) => write!(f, "不支持的操作: {}", msg),
        }
    }
}

impl std::error::Error for AudioRouterError {}

/// 错误严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorSeverity {
    /// 可恢复（如输出流断开，槽位停用）
    Recoverable,
    /// 降级运行（如输入流断开，静音输出并重连）
    Degraded,
    /// 致命（立即退出进程）
    Fatal,
}

/// 错误恢复状态机（对应开发规划 1.5 和 3.4）
#[derive(Debug, Clone)]
pub enum RecoveryState {
    /// 正常运行
    Normal,
    /// 指数退避重连原设备
    ReconnectBackoff {
        /// 当前重试次数
        attempt: u32,
        /// 下次重试间隔
        next_interval: Duration,
    },
    /// 降级使用系统默认设备
    FallbackToDefault {
        /// 已尝试次数
        attempt: u32,
    },
}

/// 模块级类型别名，简化 Result 使用
pub type Result<T> = std::result::Result<T, AudioRouterError>;

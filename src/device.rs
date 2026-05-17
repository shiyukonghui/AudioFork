// 音频路由器 — 设备枚举与选择模块
// 负责检测、枚举音频设备，并提供设备选择逻辑

use crate::error::{AudioRouterError, Result};
use cpal::traits::{DeviceTrait, HostTrait};

/// 设备类型枚举，标识音频设备的连接方式
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceType {
    /// 有线设备（USB、3.5mm、内置声卡等）
    Wired,
    /// 蓝牙无线设备
    Bluetooth,
    /// 网络音频设备（AirPlay、DLNA、Chromecast 等）
    Network,
    /// 无法识别的设备类型
    #[allow(dead_code)]
    Unknown,
}

/// 设备信息结构体，存储从 cpal 提取的设备元数据
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// 设备显示名称
    pub name: String,
    /// 设备支持的采样率列表（Hz）
    pub sample_rates: Vec<u32>,
    /// 设备支持的声道数列表
    pub channels: Vec<u16>,
    /// 设备支持的采样格式列表（如 "f32", "i16", "u16"）
    pub formats: Vec<String>,
    /// 设备连接类型
    pub device_type: DeviceType,
}

/// 根据设备名称检测设备类型
///
/// 通过关键字匹配判断设备的连接方式：
/// - 包含 "bluetooth" 或 "蓝牙" → Bluetooth
/// - 包含 "network"、"airplay"、"dlna"、"chromecast"、"网络" → Network
/// - 其他情况默认为有线设备 Wired
pub fn detect_device_type(name: &str) -> DeviceType {
    let lower = name.to_lowercase();

    // 检查蓝牙关键字
    if lower.contains("bluetooth") || lower.contains("蓝牙") {
        return DeviceType::Bluetooth;
    }

    // 检查网络音频关键字
    if lower.contains("network")
        || lower.contains("airplay")
        || lower.contains("dlna")
        || lower.contains("chromecast")
        || lower.contains("网络")
    {
        return DeviceType::Network;
    }

    // 默认当作有线设备
    DeviceType::Wired
}

/// 将 cpal 的 SampleFormat 转换为字符串表示
fn sample_format_to_string(format: cpal::SampleFormat) -> String {
    // cpal 在 Win/macOS 平台上可能通过 WASAPI/CoreAudio 返回非标准格式值
    // 使用 Debug trait 兜底，确保不会丢失格式信息
    match format {
        cpal::SampleFormat::F32 => "f32".to_string(),
        cpal::SampleFormat::I16 => "i16".to_string(),
        cpal::SampleFormat::U16 => "u16".to_string(),
        other => format!("{:?}", other),
    }
}

/// 从 cpal 设备提取默认配置范围信息，构建 DeviceInfo
pub fn get_device_info(device: &cpal::Device, device_type: DeviceType) -> DeviceInfo {
    let name = device.name().unwrap_or_else(|_| "未知设备".to_string());

    let mut sample_rates = Vec::new();
    let mut channels = Vec::new();
    let mut formats = Vec::new();

    // 尝试获取输入配置范围
    if let Ok(configs) = device.supported_input_configs() {
        for cfg in configs {
            // 收集采样率范围
            let min_rate = cfg.min_sample_rate().0;
            let max_rate = cfg.max_sample_rate().0;
            sample_rates.push(min_rate);
            if max_rate != min_rate {
                sample_rates.push(max_rate);
            }
            // 收集声道数
            channels.push(cfg.channels());
            // 收集格式
            let fmt_str = sample_format_to_string(cfg.sample_format());
            if !formats.contains(&fmt_str) {
                formats.push(fmt_str);
            }
        }
    }

    // 如果输入配置为空，尝试获取输出配置范围
    if sample_rates.is_empty() {
        if let Ok(configs) = device.supported_output_configs() {
            for cfg in configs {
                let min_rate = cfg.min_sample_rate().0;
                let max_rate = cfg.max_sample_rate().0;
                sample_rates.push(min_rate);
                if max_rate != min_rate {
                    sample_rates.push(max_rate);
                }
                channels.push(cfg.channels());
                let fmt_str = sample_format_to_string(cfg.sample_format());
                if !formats.contains(&fmt_str) {
                    formats.push(fmt_str);
                }
            }
        }
    }

    // 去重并排序采样率
    sample_rates.sort();
    sample_rates.dedup();
    // 去重并排序声道数
    channels.sort();
    channels.dedup();

    DeviceInfo {
        name,
        sample_rates,
        channels,
        formats,
        device_type,
    }
}

/// 枚举系统中所有可用的输入（录音）设备
///
/// 调用 cpal 获取输入设备列表，并为每个设备提取名称、
/// 支持的配置范围和设备类型信息。
pub fn enumerate_input_devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal::default_host();

    let devices = host
        .input_devices()
        .map_err(|e| AudioRouterError::StreamError(format!("获取输入设备列表失败: {}", e)))?;

    let mut result = Vec::new();

    for device in devices {
        let name = device
            .name()
            .unwrap_or_else(|_| "未知设备".to_string());
        let device_type = detect_device_type(&name);
        let info = get_device_info(&device, device_type);
        result.push(info);
    }

    Ok(result)
}

/// 枚举系统中所有可用的输出（播放）设备
///
/// 调用 cpal 获取输出设备列表，并过滤掉不支持输出配置的设备。
pub fn enumerate_output_devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal::default_host();

    let devices = host
        .output_devices()
        .map_err(|e| AudioRouterError::StreamError(format!("获取输出设备列表失败: {}", e)))?;

    let mut result = Vec::new();

    for device in devices {
        let name = device
            .name()
            .unwrap_or_else(|_| "未知设备".to_string());

        // 安全检查：确认该设备确实支持输出配置
        if device.supported_output_configs().is_err() {
            continue;
        }

        let device_type = detect_device_type(&name);
        let info = get_device_info(&device, device_type);
        result.push(info);
    }

    Ok(result)
}

/// 选择输入设备
///
/// - 若 `name` 为 `None`，返回系统默认输入设备及其信息。
/// - 若 `name` 为 `Some`，在所有输入设备中按名称模糊匹配
///   （设备名包含指定字符串即可，不区分大小写）。
/// - 找不到匹配设备则返回 `DeviceNotFound` 错误。
pub fn select_input_device(name: Option<&str>) -> Result<(cpal::Device, DeviceInfo)> {
    let host = cpal::default_host();

    let devices = host
        .input_devices()
        .map_err(|e| AudioRouterError::StreamError(format!("获取输入设备列表失败: {}", e)))?;

    // 收集所有输入设备
    let all_inputs: Vec<cpal::Device> = devices.collect();

    match name {
        None => {
            // 未指定名称：返回系统默认输入设备
            let default_device = host
                .default_input_device()
                .ok_or_else(|| AudioRouterError::DeviceNotFound("没有默认输入设备".to_string()))?;

            let default_name = default_device
                .name()
                .unwrap_or_else(|_| "未知设备".to_string());
            let device_type = detect_device_type(&default_name);
            let info = get_device_info(&default_device, device_type);

            Ok((default_device, info))
        }
        Some(query) => {
            // 指定了名称：模糊匹配（不区分大小写）
            let query_lower = query.to_lowercase();

            for device in all_inputs {
                let device_name = device
                    .name()
                    .unwrap_or_else(|_| String::new());
                if device_name.to_lowercase().contains(&query_lower) {
                    let device_type = detect_device_type(&device_name);
                    let info = get_device_info(&device, device_type);
                    return Ok((device, info));
                }
            }

            Err(AudioRouterError::DeviceNotFound(format!(
                "未找到匹配的输入设备: {}",
                query
            )))
        }
    }
}

/// 选择输出设备（支持批量选择）
///
/// - 若 `names` 为空，返回所有有效的输出设备。
/// - 若 `names` 非空，按名称模糊匹配每个名称（不区分大小写），
///   匹配到的设备加入结果列表。
/// - 至少需要匹配到一个设备，否则返回 `DeviceNotFound` 错误。
pub fn select_output_devices(names: &[String]) -> Result<Vec<(cpal::Device, DeviceInfo)>> {
    let host = cpal::default_host();

    let devices = host
        .output_devices()
        .map_err(|e| AudioRouterError::StreamError(format!("获取输出设备列表失败: {}", e)))?;

    // 收集所有支持输出的设备及其名称
    // 使用 (Device, name) 元组便于后续按名称查找和所有权转移
    let mut all_outputs: Vec<(cpal::Device, String)> = devices
        .filter(|d| d.supported_output_configs().is_ok())
        .map(|d| {
            let name = d.name().unwrap_or_else(|_| "未知设备".to_string());
            (d, name)
        })
        .collect();

    if names.is_empty() {
        // 未指定名称：返回所有输出设备
        let mut result = Vec::new();

        for (device, name) in all_outputs {
            let device_type = detect_device_type(&name);
            let info = get_device_info(&device, device_type);
            result.push((device, info));
        }

        if result.is_empty() {
            return Err(AudioRouterError::DeviceNotFound(
                "没有可用的输出设备".to_string(),
            ));
        }

        Ok(result)
    } else {
        // 指定了名称列表：对每个名称模糊匹配
        let mut result = Vec::new();

        for query in names {
            let query_lower = query.to_lowercase();

            // 在剩余设备中查找首个匹配项，取出所有权后移出列表
            if let Some(pos) = all_outputs
                .iter()
                .position(|(_, name)| name.to_lowercase().contains(&query_lower))
            {
                let (device, name) = all_outputs.remove(pos);
                let device_type = detect_device_type(&name);
                let info = get_device_info(&device, device_type);
                result.push((device, info));
            }
        }

        if result.is_empty() {
            return Err(AudioRouterError::DeviceNotFound(format!(
                "未找到匹配的输出设备: {:?}",
                names
            )));
        }

        Ok(result)
    }
}

/// 枚举系统中所有可用的回环（Loopback）设备
///
/// 回环设备用于捕获系统音频输出，通过枚举输出设备并过滤
/// 不支持输出配置的设备来构建设备信息列表。
pub fn enumerate_loopback_devices() -> Result<Vec<DeviceInfo>> {
    let host = cpal::default_host();

    let devices = host
        .output_devices()
        .map_err(|e| AudioRouterError::StreamError(format!("获取输出设备列表失败: {}", e)))?;

    let mut result = Vec::new();

    for device in devices {
        let name = device
            .name()
            .unwrap_or_else(|_| "未知设备".to_string());

        // 过滤掉不支持输出配置的设备
        if device.supported_output_configs().is_err() {
            continue;
        }

        let device_type = detect_device_type(&name);
        let info = get_device_info(&device, device_type);
        result.push(info);
    }

    Ok(result)
}

/// 选择回环（Loopback）设备
///
/// - 若 `name` 为 `None`，返回系统默认输出设备及其信息。
/// - 若 `name` 为 `Some`，在所有输出设备中按名称模糊匹配
///   （设备名包含指定字符串即可，不区分大小写）。
/// - 找不到匹配设备则返回 `DeviceNotFound` 错误。
pub fn select_loopback_device(name: Option<&str>) -> Result<(cpal::Device, DeviceInfo)> {
    let host = cpal::default_host();

    let devices = host
        .output_devices()
        .map_err(|e| AudioRouterError::StreamError(format!("获取输出设备列表失败: {}", e)))?;

    // 收集所有支持输出的设备
    let all_outputs: Vec<cpal::Device> = devices
        .filter(|d| d.supported_output_configs().is_ok())
        .collect();

    match name {
        None => {
            // 未指定名称：返回系统默认输出设备
            let default_device = host
                .default_output_device()
                .ok_or_else(|| AudioRouterError::DeviceNotFound("没有默认输出设备".to_string()))?;

            let default_name = default_device
                .name()
                .unwrap_or_else(|_| "未知设备".to_string());
            let device_type = detect_device_type(&default_name);
            let info = get_device_info(&default_device, device_type);

            Ok((default_device, info))
        }
        Some(query) => {
            // 指定了名称：模糊匹配（不区分大小写）
            let query_lower = query.to_lowercase();

            for device in all_outputs {
                let device_name = device
                    .name()
                    .unwrap_or_else(|_| String::new());
                if device_name.to_lowercase().contains(&query_lower) {
                    let device_type = detect_device_type(&device_name);
                    let info = get_device_info(&device, device_type);
                    return Ok((device, info));
                }
            }

            Err(AudioRouterError::DeviceNotFound(format!(
                "未找到匹配的回环设备: {}",
                query
            )))
        }
    }
}

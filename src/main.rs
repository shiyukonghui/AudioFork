// 音频路由器入口 — Phase 1: 直通管道
// 实现输入设备 → 共享环形缓冲区 → 输出设备的音频直通路由
mod error;
mod config;
mod device;
mod audio;
mod cli;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use clap::Parser;
use tracing_subscriber::prelude::*;

use crate::audio::{CaptureStream, PlaybackStream};
use crate::device::DeviceInfo;
use crate::error::Result;

fn main() {
    // 使用闭包包装主逻辑，方便统一错误处理和日志输出
    if let Err(e) = run() {
        tracing::error!("致命错误，程序退出: {}", e);
        std::process::exit(1);
    }
}

/// Phase 1 主流程入口，返回 Result 便于上层错误处理
fn run() -> Result<()> {
    // ========================================================================
    // 1. 解析命令行参数
    // ========================================================================
    let cli_args = cli::CliArgs::parse();

    // ========================================================================
    // 2. 初始化日志系统
    // ========================================================================
    // 构建环境过滤器：优先使用 RUST_LOG 环境变量，否则默认 INFO 级别
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    // 文件日志守卫：必须在整个程序运行期间存活，否则非阻塞写入器会被关闭
    let _file_guard;

    if let Some(ref log_file) = cli_args.log_file {
        // 解析日志文件路径
        let log_path = std::path::Path::new(log_file);
        let dir = log_path
            .parent()
            .unwrap_or(std::path::Path::new("."));
        let filename = log_path
            .file_name()
            .unwrap_or(std::ffi::OsStr::new("audio_router.log"));

        // 创建滚动日志写入器（never 表示不按日期切分，改用追加模式）
        let file_appender = tracing_appender::rolling::never(dir, filename);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        _file_guard = Some(guard);

        // 控制台输出层（带颜色）
        let console_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stdout)
            .with_target(true);

        // 文件输出层（无 ANSI 颜色转义码）
        let file_layer = tracing_subscriber::fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_target(true);

        // 组合注册：同时输出到控制台和文件
        tracing_subscriber::registry()
            .with(env_filter)
            .with(console_layer)
            .with(file_layer)
            .init();
    } else {
        // 仅控制台输出
        _file_guard = None;
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_target(true)
            .init();
    }

    tracing::info!("音频路由器 v{} 启动", env!("CARGO_PKG_VERSION"));

    // ========================================================================
    // 3. 加载与合并配置
    // ========================================================================
    // 尝试加载 TOML 配置文件，文件不存在则使用默认配置
    let file_config = if let Some(ref config_path) = cli_args.config {
        let path = std::path::Path::new(config_path);
        if path.exists() {
            tracing::info!("加载配置文件: {}", path.display());
            config::load_config(path)?
        } else {
            tracing::info!(
                "配置文件 '{}' 不存在，使用默认配置",
                path.display()
            );
            config::AudioRouterConfig::default()
        }
    } else {
        config::AudioRouterConfig::default()
    };

    // 合并 CLI 参数与文件配置，CLI 参数优先级更高
    let resolved = cli_args.merge_with_config(&file_config);

    // ========================================================================
    // 4. GUI 模式处理
    // ========================================================================
    if resolved.gui {
        // 调用 filter_for_gui 过滤参数并输出警告日志
        let _filtered = cli_args.filter_for_gui();
        tracing::info!("GUI 模式将在第四阶段实现");
        println!("GUI 模式将在第四阶段实现");
        std::process::exit(0);
    }

    // ========================================================================
    // 5. 枚举所有音频设备并打印信息
    // ========================================================================
    tracing::info!("正在枚举音频设备...");
    enumerate_and_log_devices()?;

    // ========================================================================
    // 6. 选择输入设备和输出设备
    // ========================================================================
    tracing::info!("正在选择音频设备...");

    // 选择输入设备（按名称匹配或使用系统默认）
    let (device_in, info_in) =
        device::select_input_device(resolved.input_device.as_deref())?;
    tracing::info!("输入设备: {}", info_in.name);

    // 选择输出设备列表，取第一个
    let output_devices = device::select_output_devices(&resolved.output_devices)?;
    let (device_out, info_out) = output_devices
        .into_iter()
        .next()
        .ok_or_else(|| crate::error::AudioRouterError::DeviceNotFound(
            "没有可用的输出设备".to_string(),
        ))?;
    tracing::info!("输出设备: {}", info_out.name);

    // ========================================================================
    // 7. 构建流配置并做参数校验（同参数直通验证）
    // ========================================================================
    // 解析采样率和声道数：优先使用用户指定值，否则取设备默认值或兜底默认值
    let sample_rate = resolved.sample_rate.unwrap_or(48000);
    let channels = resolved.channels.unwrap_or(2);

    // 输入流配置（f32 采样格式）
    let input_config = cpal::StreamConfig {
        channels,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    // 输出流配置（Phase 1 必须与输入完全相同，不支持重采样）
    let output_config = cpal::StreamConfig {
        channels,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    // ========================================================================
    // 8. Phase 1 同参数直通验证
    // ========================================================================
    // 检查输入与输出配置是否一致（Phase 1 不支持重采样和声道变换）
    if input_config.sample_rate.0 != output_config.sample_rate.0
        || input_config.channels != output_config.channels
    {
        tracing::error!(
            "输入配置: {}Hz/{}ch, 输出配置: {}Hz/{}ch",
            input_config.sample_rate.0,
            input_config.channels,
            output_config.sample_rate.0,
            output_config.channels,
        );
        tracing::error!(
            "需要重采样能力，将在第三阶段支持。当前要求输入和输出采样率、声道数必须相同。"
        );
        return Err(crate::error::AudioRouterError::ConfigError(
            "需要重采样能力，将在第三阶段支持。当前要求输入和输出采样率、声道数必须相同。"
                .to_string(),
        ));
    }

    // ========================================================================
    // 9. 创建共享环形缓冲区（输入 → 缓冲区 → 输出）
    // ========================================================================
    // 使用 Arc<Mutex<VecDeque<f32>>> 作为线程间共享的音频数据缓冲
    // 缓冲区容量约为 1 秒的音频数据（sample_rate * channels 个采样点）
    let buffer_capacity = (sample_rate as usize) * (channels as usize);
    let shared_buffer = Arc::new(Mutex::new(VecDeque::<f32>::new()));

    // ========================================================================
    // 10. 定义输入回调：将捕获的音频数据写入共享缓冲区
    // ========================================================================
    let buf_in = Arc::clone(&shared_buffer);
    let buffer_capacity_in = buffer_capacity;
    let on_input = move |data: &[f32]| {
        if let Ok(mut buf) = buf_in.lock() {
            for &sample in data {
                // 缓冲区满时丢弃最旧的采样点，实现环形覆盖
                if buf.len() >= buffer_capacity_in {
                    buf.pop_front();
                }
                buf.push_back(sample);
            }
        }
    };

    // ========================================================================
    // 11. 定义输出回调：从共享缓冲区拉取数据填充输出
    // ========================================================================
    let buf_out = Arc::clone(&shared_buffer);
    let running = Arc::new(AtomicBool::new(true));
    let on_output = move |data: &mut [f32]| {
        if let Ok(mut buf) = buf_out.lock() {
            let need = data.len();
            if buf.len() >= need {
                // 缓冲区数据充足 —— 直接拷贝
                for dst in data.iter_mut() {
                    *dst = buf.pop_front().unwrap_or(0.0);
                }
            } else {
                // 缓冲区欠载 —— 填充静音，清空残余数据
                for dst in data.iter_mut() {
                    *dst = 0.0;
                }
                buf.clear();
            }
        }
    };

    // ========================================================================
    // 12. 启动音频管道
    // ========================================================================
    tracing::info!("正在启动音频管道...");

    // 12.1 先创建并启动输出流
    let playback = PlaybackStream::new(&device_out, &output_config, on_output)?;
    tracing::info!("输出流已创建，等待就绪...");

    // 12.2 等待输出流就绪（最多 5 秒）
    if !playback.wait_ready(Duration::from_secs(5)) {
        tracing::error!("输出流在 5 秒内未能就绪，请检查音频设备");
        return Err(crate::error::AudioRouterError::StreamError(
            "输出流就绪超时".to_string(),
        ));
    }
    tracing::info!("输出流已就绪");

    // 12.3 创建并启动输入流
    let capture = CaptureStream::new(&device_in, &input_config, on_input)?;
    tracing::info!("输入流已创建");

    // ========================================================================
    // 13. 打印启动成功信息
    // ========================================================================
    tracing::info!("==========================================");
    tracing::info!("  音频路由器已启动");
    tracing::info!("  输入设备: {}", info_in.name);
    tracing::info!("  输出设备: {}", info_out.name);
    tracing::info!("  采样率: {} Hz", sample_rate);
    tracing::info!("  声道数: {}", channels);
    tracing::info!("  缓冲区容量: {} 采样点 (~1 秒)", buffer_capacity);
    tracing::info!("  按 Enter 键停止路由...");
    tracing::info!("==========================================");

    // ========================================================================
    // 14. 等待用户按下 Enter 键停止
    // ========================================================================
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok();

    // ========================================================================
    // 15. 停止流程
    // ========================================================================
    tracing::info!("正在停止音频路由...");
    running.store(false, Ordering::SeqCst);

    // 输入流和输出流会在 Drop 时自动停止
    // 显式 drop 以确保析构顺序（先停输入，再停输出）
    drop(capture);
    drop(playback);

    tracing::info!("音频路由器已停止");
    Ok(())
}

/// 枚举所有输入和输出设备，以 info 级别打印详细信息
fn enumerate_and_log_devices() -> Result<()> {
    // 枚举输入设备
    let inputs = device::enumerate_input_devices()?;
    tracing::info!("发现 {} 个输入设备:", inputs.len());
    for info in &inputs {
        log_device_info(info, "  [IN] ");
    }

    // 枚举输出设备
    let outputs = device::enumerate_output_devices()?;
    tracing::info!("发现 {} 个输出设备:", outputs.len());
    for info in &outputs {
        log_device_info(info, "  [OUT]");
    }

    Ok(())
}

/// 以统一格式打印单个设备信息
fn log_device_info(info: &DeviceInfo, prefix: &str) {
    let sample_rates_str: Vec<String> = info
        .sample_rates
        .iter()
        .map(|r| format!("{}Hz", r))
        .collect();
    let channels_str: Vec<String> = info
        .channels
        .iter()
        .map(|c| c.to_string())
        .collect();
    tracing::info!(
        "{}{} | 采样率: [{}] | 声道: [{}] | 格式: [{}] | 类型: {:?}",
        prefix,
        info.name,
        sample_rates_str.join(", "),
        channels_str.join(", "),
        info.formats.join(", "),
        info.device_type,
    );
}

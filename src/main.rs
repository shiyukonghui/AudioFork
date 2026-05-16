// 音频路由器入口 — Phase 2: 多输出扇出管道
// 实现输入设备 → [SPSC 环形缓冲区 × N] → 多个输出设备的音频扇出路由
// 使用 SlotArray 管理槽位，Fader 实现平滑淡入淡出，ChannelMapper 处理声道转换
mod error;
mod config;
mod device;
mod audio;
mod cli;
mod channel_map;
mod pipeline;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use clap::Parser;
use ringbuf::traits::Split;
use tracing_subscriber::prelude::*;

use crate::audio::{CaptureStream, PlaybackStream};
use crate::channel_map::ChannelMapper;
use crate::device::DeviceInfo;
use crate::error::{AudioRouterError, Result};
use crate::pipeline::{Fader, SlotArray, MAX_OUTPUTS};

// ============================================================================
// 输出槽位：将播放流及其元数据捆绑在一起，便于统一管理生命周期
// ============================================================================
struct OutputSlot {
    /// 底层 cpal 输出播放流
    playback: PlaybackStream,
    /// 在 SlotArray 中的槽位索引
    index: usize,
    /// 淡入淡出处理器（共享引用，与输出回调共享）
    fader: Arc<Fader>,
    /// 欠载计数器（与输出回调共享相同的 Arc）
    underrun_counter: Arc<AtomicU64>,
    /// 输出设备名称（用于日志/统计）
    device_name: String,
    /// 是否为直通模式（输入/输出声道数相同，无需 ChannelMapper 转换）
    is_passthrough: bool,
    /// 输出音频流的声道数
    output_channels: u16,
}

fn main() {
    // 使用闭包包装主逻辑，方便统一错误处理和日志输出
    if let Err(e) = run() {
        tracing::error!("致命错误，程序退出: {}", e);
        std::process::exit(1);
    }
}

/// Phase 2 主流程入口，返回 Result 便于上层错误处理
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
    // 6. 选择输入设备和输出设备列表（Phase 2：取全部输出设备）
    // ========================================================================
    tracing::info!("正在选择音频设备...");

    // 选择输入设备（按名称匹配或使用系统默认）
    let (device_in, info_in) =
        device::select_input_device(resolved.input_device.as_deref())?;
    tracing::info!("输入设备: {}", info_in.name);

    // 选择所有输出设备（Phase 2 取全部，不再只取第一个）
    let output_devices = device::select_output_devices(&resolved.output_devices)?;
    if output_devices.is_empty() {
        return Err(AudioRouterError::DeviceNotFound(
            "没有可用的输出设备".to_string(),
        ));
    }
    tracing::info!("选择了 {} 个输出设备", output_devices.len());

    // ========================================================================
    // 7. 构建输入流配置 + max-latency-ms 校验
    // ========================================================================
    let buffer_frames = resolved.buffer_frames;
    let sample_rate = resolved.sample_rate.unwrap_or(48000);
    let channels = resolved.channels.unwrap_or(2);

    // max-latency-ms 校验：缓冲区延迟不应超过用户设置的最大值
    let delay_ms = (buffer_frames as f64) * 1000.0 / (sample_rate as f64);
    if delay_ms > resolved.max_latency_ms as f64 {
        tracing::error!(
            "缓冲区延迟 {:.1}ms 超过最大允许延迟 {}ms，请减小 --buffer-frames 或增大 --max-latency-ms",
            delay_ms,
            resolved.max_latency_ms
        );
        return Err(AudioRouterError::ConfigError(format!(
            "缓冲区延迟 {:.1}ms 超过最大允许延迟 {}ms",
            delay_ms,
            resolved.max_latency_ms
        )));
    }

    // 输入流配置（f32 采样格式，buffer_size 使用系统默认）
    let input_config = cpal::StreamConfig {
        channels,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    // ========================================================================
    // 8. Phase 2 采样率校验
    // ========================================================================
    // Phase 2 不支持重采样（Phase 3 加入），但支持不同声道数的输出（通过 ChannelMapper）
    // 对每个输出设备校验采样率是否一致，不一致则跳过并警告
    let mut compatible_outputs: Vec<(cpal::Device, DeviceInfo)> = Vec::new();
    for (dev, info) in output_devices {
        // 如果设备支持列表为空，说明无法确定支持的采样率，尝试使用
        // 如果支持列表非空且不包含指定采样率，跳过并警告
        if !info.sample_rates.is_empty()
            && !info.sample_rates.contains(&sample_rate)
        {
            tracing::warn!(
                "跳过输出设备 '{}'：不支持采样率 {}Hz（支持的采样率: {:?}），将在 Phase 3 重采样支持",
                info.name,
                sample_rate,
                info.sample_rates
            );
            continue;
        }
        compatible_outputs.push((dev, info));
    }

    if compatible_outputs.is_empty() {
        return Err(AudioRouterError::ConfigError(
            "没有输出设备支持当前采样率，请更换采样率或等待 Phase 3 重采样支持".to_string(),
        ));
    }

    tracing::info!(
        "采样率校验通过，{} 个输出设备兼容",
        compatible_outputs.len()
    );

    // ========================================================================
    // 9. 创建 SlotArray 和 overflow_counters
    // ========================================================================
    let slot_array = Arc::new(SlotArray::new());
    let overflow_counters: Arc<[AtomicU64; MAX_OUTPUTS]> =
        Arc::new(std::array::from_fn(|_| AtomicU64::new(0)));

    // ========================================================================
    // 10. 为每个输出设备分配槽位并创建输出流
    // ========================================================================
    let mut output_slots: Vec<OutputSlot> = Vec::new();

    for (device_out, info_out) in &compatible_outputs {
        // 10.1 确定输出声道数：优先使用设备原生声道数，回退到输入声道数
        let output_channels = info_out
            .channels
            .first()
            .copied()
            .unwrap_or(channels);

        // 10.2 创建 SPSC 环形缓冲区
        // 容量 = buffer_frames × 最大声道数 × 4（安全余量）
        let max_channels = channels.max(output_channels) as usize;
        let rb_capacity = buffer_frames as usize * max_channels * 4;
        let rb: Arc<ringbuf::HeapRb<f32>> =
            ringbuf::HeapRb::<f32>::new(rb_capacity).into();
        let (producer, consumer) = rb.split();

        // 10.3 分配槽位
        let slot_index = slot_array
            .allocate_slot(producer)
            .ok_or_else(|| {
                AudioRouterError::Fatal(format!(
                    "无法为输出设备 '{}' 分配槽位：已达到最大槽位数 {}",
                    info_out.name, MAX_OUTPUTS
                ))
            })?;

        // 10.4 创建声道映射器
        let channel_mapper = Arc::new(ChannelMapper::new(channels, output_channels));
        let is_passthrough = channel_mapper.is_passthrough();

        // 10.5 创建淡入淡出处理器（5ms 淡入淡出时长）
        let fade_len = ((5 * sample_rate as usize) / 1000).max(1);
        let fader = Arc::new(Fader::new(fade_len));

        // 10.6 创建欠载计数器
        let underrun_counter = Arc::new(AtomicU64::new(0));

        // 10.7 创建输出回调
        let output_callback = pipeline::create_output_callback(
            consumer,
            Arc::clone(&fader),
            Arc::clone(&channel_mapper),
            channels,
            output_channels,
            sample_rate,
            sample_rate, // Phase 2: 输出采样率与输入相同（无重采样）
            Arc::clone(&underrun_counter),
        );

        // 10.8 构建输出流配置（每个输出使用自己声道数，采样率与输入相同）
        let output_config = cpal::StreamConfig {
            channels: output_channels,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // 10.9 创建输出播放流
        let playback = PlaybackStream::new(device_out, &output_config, output_callback)?;

        let slot = OutputSlot {
            playback,
            index: slot_index,
            fader,
            underrun_counter,
            device_name: info_out.name.clone(),
            is_passthrough,
            output_channels,
        };

        tracing::info!(
            "  输出设备 '{}' 已配置 | 槽位 #{} | {}ch | 模式: {} | 淡入淡出: {} 帧",
            slot.device_name,
            slot.index,
            slot.output_channels,
            if slot.is_passthrough { "直通" } else { "声道转换" },
            fade_len,
        );

        output_slots.push(slot);
    }

    // ========================================================================
    // 11. 等待所有输出流就绪（各自 5 秒超时）
    // ========================================================================
    tracing::info!("正在等待输出流就绪...");

    let total = output_slots.len();
    output_slots.retain(|slot| {
        if slot.playback.wait_ready(Duration::from_secs(5)) {
            tracing::info!(
                "  输出流 '{}'（槽位 #{}）已就绪",
                slot.device_name,
                slot.index
            );
            true
        } else {
            tracing::error!(
                "  输出流 '{}'（槽位 #{}）在 5 秒内未能就绪，已停用该槽位",
                slot.device_name,
                slot.index
            );
            slot_array.deactivate_slot(slot.index);
            false
        }
    });

    let ready_count = output_slots.len();
    if ready_count == 0 {
        return Err(AudioRouterError::Fatal(
            "所有输出流均未能在 5 秒内就绪，请检查音频设备".to_string(),
        ));
    }

    if ready_count < total {
        tracing::warn!(
            "{} 个输出流就绪，{} 个超时被移除",
            ready_count,
            total - ready_count
        );
    }

    // ========================================================================
    // 12. 启动输入流
    // ========================================================================
    let on_input = pipeline::create_input_callback(
        Arc::clone(&slot_array),
        Arc::clone(&overflow_counters),
    );
    let capture = CaptureStream::new(&device_in, &input_config, on_input)?;
    tracing::info!("输入流已启动");

    // ========================================================================
    // 13. 打印启动信息
    // ========================================================================
    tracing::info!("==========================================");
    tracing::info!("  音频路由器 Phase 2 已启动");
    tracing::info!("  输入设备: {}", info_in.name);
    tracing::info!(
        "  输入配置: {}Hz / {}ch",
        sample_rate,
        channels
    );
    tracing::info!("  活跃输出设备: {} 个", ready_count);
    for slot in &output_slots {
        tracing::info!(
            "    [槽位 #{}] {} | {}Hz / {}ch | {}",
            slot.index,
            slot.device_name,
            sample_rate,
            slot.output_channels,
            if slot.is_passthrough {
                "直通"
            } else {
                "声道转换"
            },
        );
    }
    tracing::info!(
        "  缓冲区: {} 帧 (~{:.1}ms)",
        buffer_frames,
        delay_ms
    );
    tracing::info!("  按 Enter 键停止路由...");
    tracing::info!("==========================================");

    // ========================================================================
    // 14. 等待用户按下 Enter 键停止
    // ========================================================================
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).ok();

    // ========================================================================
    // 15. 停止流程（平滑淡出）
    // ========================================================================
    tracing::info!("正在停止音频路由...");

    // 15.1 停止输入流（先停输入，避免新数据进入缓冲区）
    drop(capture);
    tracing::info!("  输入流已停止");

    // 15.2 对所有活跃设备启动淡出
    for slot in &output_slots {
        slot.fader.start_fade_out();
        tracing::info!("  已对 '{}' 启动淡出", slot.device_name);
    }

    // 15.3 等待淡出完成（最多 2 秒）
    // 先等待一小段时间让淡出生效，再等待淡出时长完成
    std::thread::sleep(Duration::from_millis(100));
    // 额外等待，确保淡出缓冲区有足够时间排空
    let fade_wait_ms = (delay_ms * 3.0) as u64 + 200;
    std::thread::sleep(Duration::from_millis(fade_wait_ms));

    // 15.4 打印最终统计信息
    tracing::info!("--- 最终统计 ---");
    for slot in &output_slots {
        let overflows = overflow_counters[slot.index].load(Ordering::Relaxed);
        let underruns = slot.underrun_counter.load(Ordering::Relaxed);
        tracing::info!(
            "  {}（槽位 #{}）| 溢出: {} | 欠载: {}",
            slot.device_name,
            slot.index,
            overflows,
            underruns
        );
    }

    // 15.5 释放所有输出流（Drop 自动停止底层音频流）
    drop(output_slots);

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

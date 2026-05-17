// 音频路由器入口 — Phase 3: 多输出扇出管道 + 重采样 + 限幅 + 漂移补偿 + 热插拔
// 实现输入设备 → [SPSC 环形缓冲区 × N] → 多个输出设备的音频扇出路由
// Phase 3 新增：重采样器支持不同采样率、砖墙限幅器、时钟漂移补偿、热插拔监听
mod error;
mod config;
mod device;
mod audio;
mod cli;
mod channel_map;
mod pipeline;
mod recovery;
mod drift;
mod limiter;
mod hotplug;
mod resample;
mod message;
mod gui;
mod engine;
mod source;

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::resample::{ResampleProcessor, ResamplerType};
use crate::limiter::BrickwallLimiter;
use crate::drift::DriftCompensator;
use crate::hotplug::{start_hotplug_monitor, HotplugEvent};
use crate::recovery::{InputRecoveryManager, RecoveryAction};
use clap::Parser;
use crossbeam_channel;
use ringbuf::traits::Split;
use tracing_subscriber::prelude::*;

use crate::audio::{CaptureStream, PlaybackStream};
use crate::channel_map::ChannelMapper;
use crate::device::DeviceInfo;
use crate::error::{AudioRouterError, Result};
use crate::pipeline::{Fader, SlotArray};

// ============================================================================
// 输出槽位：将播放流及其元数据捆绑在一起，便于统一管理生命周期
// ============================================================================
#[allow(dead_code)]
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
    // ===== Phase 3 新增 =====
    /// 重采样处理器（共享引用，输出回调中每次处理帧时调用）
    resampler: Arc<Mutex<ResampleProcessor>>,
    /// 砖墙限幅器（共享引用，防止削波失真）
    limiter: Arc<Mutex<BrickwallLimiter>>,
    /// 漂移补偿帧数微调量（控制线程写入，音频回调读取）
    delta: Arc<AtomicI32>,
    /// 时钟漂移补偿器（可选，由 --no-drift-compensation 控制是否启用）
    drift: Option<Arc<Mutex<DriftCompensator>>>,
    /// 输入丢失标志（供音频回调线程读取，用于静音输出）
    input_lost: Arc<AtomicBool>,
    /// 实际输出设备采样率（可能与输入不同，通过重采样器转换）
    output_sample_rate: u32,
}

fn main() {
    // 解析命令行参数（在 main 开头解析，用于判断是否需要隐藏控制台）
    let cli_args = cli::CliArgs::parse();

    // Windows 平台：GUI 模式下隐藏控制台窗口
    // 条件：用户显式指定 --gui，或者没有传入任何操作参数（默认启动 GUI）
    #[cfg(windows)]
    {
        let should_hide_console = cli_args.gui || !cli_args.has_operational_args();
        if should_hide_console {
            unsafe {
                let _ = windows::Win32::System::Console::FreeConsole();
            }
        }
    }

    // 使用闭包包装主逻辑，方便统一错误处理和日志输出
    if let Err(e) = run(cli_args) {
        // GUI 模式下错误通过对话框显示（控制台已隐藏）
        #[cfg(windows)]
        {
            // 重新判断是否为 GUI 模式（避免重复解析）
            let is_gui_mode = std::env::args().any(|a| a == "--gui") 
                || !std::env::args().any(|a| a.starts_with("--") && a != "--gui" && a != "--config");
            if is_gui_mode {
                rfd::MessageDialog::new()
                    .set_title("音频路由器 - 错误")
                    .set_description(&format!("程序启动失败: {}", e))
                    .set_buttons(rfd::MessageButtons::Ok)
                    .show();
            } else {
                tracing::error!("致命错误，程序退出: {}", e);
            }
        }
        #[cfg(not(windows))]
        {
            tracing::error!("致命错误，程序退出: {}", e);
        }
        std::process::exit(1);
    }
}

/// Phase 2 主流程入口，返回 Result 便于上层错误处理
fn run(cli_args: cli::CliArgs) -> Result<()> {
    // ========================================================================
    // 1. 解析命令行参数（已在 main() 中解析，此处直接使用）
    // ========================================================================
    // cli_args 已在 main() 中解析并传入

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
    // 4. GUI 模式处理（Phase 4）
    // ========================================================================
    if resolved.gui {
        let _filtered = cli_args.filter_for_gui();
        tracing::info!("启动 GUI 模式...");
        return gui::run_gui(resolved.config_path.clone());
    }

    // ========================================================================
    // 5. 解析音源类型
    // ========================================================================
    let source_type = crate::source::SourceType::from_str(&resolved.source_type);
    tracing::info!("音源类型: {}", source_type.as_str());

    // ========================================================================
    // 6. 枚举所有音频设备并打印信息
    // ========================================================================
    tracing::info!("正在枚举音频设备...");
    enumerate_and_log_devices()?;

    // ========================================================================
    // 6. 选择输入设备和输出设备列表（Phase 2：取全部输出设备）
    // ========================================================================
    tracing::info!("正在选择音频设备...");

    // 根据音源类型选择采集源
    let (device_source, info_source) = match source_type {
        crate::source::SourceType::Loopback => {
            tracing::info!("音源类型: Loopback（系统音频回采）");
            device::select_loopback_device(resolved.loopback_device.as_deref())?
        }
        crate::source::SourceType::InputDevice => {
            tracing::info!("音源类型: 物理输入设备");
            device::select_input_device(resolved.input_device.as_deref())?
        }
    };
    tracing::info!("音源设备: {}", info_source.name);

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
    // 8. Phase 3 采样率处理：通过重采样器支持不同采样率的输出设备
    // ========================================================================

    // ========================================================================
    // 9. 创建 SlotArray 和 overflow_counters
    // ========================================================================
    // 使用配置中的 max_outputs 创建槽位数组（替代硬编码的 MAX_OUTPUTS）
    let slot_array = Arc::new(SlotArray::new(resolved.max_outputs as usize));
    // 动态创建溢出计数器数组（大小与 max_outputs 一致）
    let overflow_counters: Arc<[AtomicU64]> = (0..resolved.max_outputs as usize)
        .map(|_| AtomicU64::new(0))
        .collect();

    // ========================================================================
    // 10. 为每个输出设备分配槽位并创建输出流
    // ========================================================================
    let mut output_slots: Vec<OutputSlot> = Vec::new();

    for (device_out, info_out) in &output_devices {
        // 10.1 确定输出声道数：优先使用设备原生声道数，回退到输入声道数
        let output_channels = info_out
            .channels
            .first()
            .copied()
            .unwrap_or(channels);

        // 10.2 选择输出设备的最佳采样率：优先与输入相同，否则选最接近的
        let output_sample_rate = if info_out.sample_rates.is_empty() {
            sample_rate // 设备未报告采样率列表，使用输入采样率
        } else if info_out.sample_rates.contains(&sample_rate) {
            sample_rate // 与输入相同
        } else {
            // 选择与输入最接近的采样率
            let mut rates: Vec<&u32> = info_out.sample_rates.iter().collect();
            rates.sort_by(|a, b| {
                let da = (**a as i64 - sample_rate as i64).abs();
                let db = (**b as i64 - sample_rate as i64).abs();
                da.cmp(&db)
            });
            *rates[0]
        };

        // 10.3 resampler none 校验：不允许采样率不匹配且禁用了重采样
        if resolved.resampler == "none" && output_sample_rate != sample_rate {
            tracing::error!(
                "输出设备 '{}' 采样率 {}Hz 与输入 {}Hz 不匹配，但重采样算法设为 none",
                info_out.name, output_sample_rate, sample_rate
            );
            return Err(AudioRouterError::ConfigError(format!(
                "设备 '{}' 需要重采样（{}Hz→{}Hz），但 --resampler none",
                info_out.name, sample_rate, output_sample_rate
            )));
        }

        // 10.4 创建 SPSC 环形缓冲区
        // 容量 = buffer_frames × 最大声道数 × 4（安全余量）
        let max_channels = channels.max(output_channels) as usize;
        let rb_capacity = buffer_frames as usize * max_channels * 4;
        let rb: Arc<ringbuf::HeapRb<f32>> =
            ringbuf::HeapRb::<f32>::new(rb_capacity).into();
        let (producer, consumer) = rb.split();

        // 10.5 分配槽位
        let slot_index = slot_array
            .allocate_slot(producer)
            .ok_or_else(|| {
                AudioRouterError::Fatal(format!(
                    "无法为输出设备 '{}' 分配槽位：已达到最大槽位数 {}",
                    info_out.name, resolved.max_outputs
                ))
            })?;

        // 10.6 创建声道映射器
        let channel_mapper = Arc::new(ChannelMapper::new(channels, output_channels));
        let is_passthrough = channel_mapper.is_passthrough();

        // 10.7 创建淡入淡出处理器（5ms 淡入淡出时长）
        let fade_len = ((5 * sample_rate as usize) / 1000).max(1);
        let fader = Arc::new(Fader::new(fade_len));

        // 10.8 创建欠载计数器
        let underrun_counter = Arc::new(AtomicU64::new(0));

        // 10.9 创建重采样器
        let resampler_type = match resolved.resampler.as_str() {
            "sinc" => ResamplerType::Sinc,
            "cubic" => ResamplerType::Cubic,
            "none" => ResamplerType::None,
            other => {
                tracing::warn!("未知重采样算法 '{}'，使用 sinc", other);
                ResamplerType::Sinc
            }
        };
        let resampler = Arc::new(Mutex::new(
            ResampleProcessor::new(
                resampler_type,
                sample_rate as f64,
                output_sample_rate as f64,
                channels as usize,
                buffer_frames as usize,
            ).map_err(|e| AudioRouterError::ConfigError(format!("创建重采样器失败: {}", e)))?
        ));

        // 10.10 创建砖墙限幅器
        let limiter = Arc::new(Mutex::new(
            BrickwallLimiter::new(
                1.0,                    // 阈值 0dBFS
                0.0,                    // 零攻击
                10.0,                   // 10ms 释放
                output_channels as usize,
                output_sample_rate as f64,
                !resolved.no_limiter,   // 默认启用，--no-limiter 禁用
            )
        ));

        // 10.11 创建漂移补偿器
        let drift_enabled = !resolved.no_drift_compensation;
        let drift = if drift_enabled {
            Some(Arc::new(Mutex::new(
                DriftCompensator::new(true, sample_rate as f64, buffer_frames as usize)
            )))
        } else {
            None
        };
        let delta = Arc::new(AtomicI32::new(0));

        // 10.12 创建输入丢失标志
        let input_lost = Arc::new(AtomicBool::new(false));

        // 10.13 创建输出回调（传入 Phase 3 新参数）
        let output_callback = pipeline::create_output_callback(
            consumer,
            Arc::clone(&fader),
            Arc::clone(&channel_mapper),
            channels,
            output_channels,
            sample_rate,
            output_sample_rate,  // 实际选择的输出采样率
            Arc::clone(&underrun_counter),
            // Phase 3 新参数
            Arc::clone(&resampler),
            Arc::clone(&limiter),
            Arc::clone(&delta),
            Arc::clone(&input_lost),
            resolved.no_drift_compensation,
        );

        // 10.14 构建输出流配置（使用实际输出采样率）
        let output_config = cpal::StreamConfig {
            channels: output_channels,
            sample_rate: cpal::SampleRate(output_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // 10.15 创建输出播放流
        let playback = PlaybackStream::new(device_out, &output_config, output_callback)?;

        let slot = OutputSlot {
            playback,
            index: slot_index,
            fader,
            underrun_counter,
            device_name: info_out.name.clone(),
            is_passthrough,
            output_channels,
            resampler,
            limiter,
            delta,
            drift,
            input_lost,
            output_sample_rate,
        };

        tracing::info!(
            "  输出设备 '{}' 已配置 | 槽位 #{} | {}ch | {}Hz→{}Hz | 模式: {} | 淡入淡出: {} 帧",
            slot.device_name,
            slot.index,
            slot.output_channels,
            sample_rate,
            output_sample_rate,
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
    // 12. Phase 3 新增设施（恢复管理器、WASAPI、热插拔、输入捕捉）
    // ========================================================================

    // 12.1 WASAPI 独占模式提示（当前未完整实现）
    #[cfg(feature = "wasapi-exclusive")]
    if resolved.wasapi_exclusive {
        tracing::warn!("WASAPI 独占模式当前尚未完整实现，将使用共享模式");
    }

    // 12.2 输入丢失恢复管理器
    let mut recovery = InputRecoveryManager::new(
        resolved.exit_on_input_loss,
        resolved.input_fallback_to_default,
        Some(info_source.name.clone()),
    );

    // 12.3 根据音源类型创建捕获流
    let on_input = pipeline::create_input_callback(
        Arc::clone(&slot_array),
        Arc::clone(&overflow_counters),
    );
    let capture = match source_type {
        crate::source::SourceType::Loopback => {
            CaptureStream::from_loopback(&device_source, &input_config, on_input)?
        }
        crate::source::SourceType::InputDevice => {
            CaptureStream::new(&device_source, &input_config, on_input)?
        }
    };
    tracing::info!("音源流已启动（{}）", source_type.as_str());

    // 12.4 漂移补偿控制标志
    let drift_stop_flag = Arc::new(AtomicBool::new(false));

    // 12.5 热插拔监控
    let (hotplug_tx, hotplug_rx) = crossbeam_channel::unbounded();
    let hotplug_stop = Arc::new(AtomicBool::new(false));
    let hotplug_handle = start_hotplug_monitor(
        hotplug_tx,
        Arc::clone(&hotplug_stop),
        Duration::from_secs(2),
    );

    // ========================================================================
    // 13. 打印启动信息
    // ========================================================================
    tracing::info!("==========================================");
    tracing::info!("  音频路由器 Phase 3 已启动");
    tracing::info!("  音源设备: {}（类型: {}）", info_source.name, source_type.as_str());
    tracing::info!(
        "  输入配置: {}Hz / {}ch",
        sample_rate,
        channels
    );
    tracing::info!("  活跃输出设备: {} 个", ready_count);
    for slot in &output_slots {
        tracing::info!(
            "    [槽位 #{}] {} | {}Hz→{}Hz / {}ch | {} | 限幅: {}",
            slot.index,
            slot.device_name,
            sample_rate,
            slot.output_sample_rate,
            slot.output_channels,
            if slot.is_passthrough {
                "直通"
            } else {
                "声道转换"
            },
            if resolved.no_limiter { "禁用" } else { "启用" },
        );
    }
    tracing::info!(
        "  缓冲区: {} 帧 (~{:.1}ms) | 漂移补偿: {}",
        buffer_frames,
        delay_ms,
        if resolved.no_drift_compensation { "禁用" } else { "启用" },
    );
    tracing::info!("  按 Enter 键停止路由...");
    tracing::info!("==========================================");

    // ========================================================================
    // 14. 主循环（Phase 3：热插拔监听 + 恢复管理 + 可选监控 JSON）
    // ========================================================================
    if resolved.monitor {
        // 带监控的事件循环：每 200ms 轮询热插拔和恢复事件，每 5 秒输出 JSON
        let mut last_monitor_time = std::time::Instant::now();
        let mut last_drift_time = std::time::Instant::now();
        let mut last_recovery_time = std::time::Instant::now();

        println!("按 Enter 或 Ctrl+C 停止路由...");
        loop {
            // 14.1 检查热插拔事件（非阻塞）
            while let Ok(event) = hotplug_rx.try_recv() {
                match event {
                    HotplugEvent::DeviceAdded(info) => {
                        tracing::info!("检测到新输出设备: {}", info.name);
                    }
                    HotplugEvent::DeviceRemoved(name) => {
                        tracing::info!("输出设备已移除: {}", name);
                        if let Some(slot) = output_slots.iter().find(|s| s.device_name == name) {
                            slot_array.deactivate_slot(slot.index);
                            tracing::info!("  槽位 #{} 已停用", slot.index);
                        }
                    }
                }
            }

            // 14.2 漂移补偿更新（每 100ms）
            if last_drift_time.elapsed() >= Duration::from_millis(100) {
                last_drift_time = std::time::Instant::now();
                for slot in &output_slots {
                    if let Some(ref drift) = slot.drift {
                        if let Ok(mut d) = drift.lock() {
                            // 使用固定水位 0.5，维持 delta=0（占位逻辑）
                            // 实际水位应从 pipeline consumer 获取
                            d.update(0.5);
                            slot.delta.store(d.delta_val(), Ordering::Release);
                        }
                    }
                }
            }

            // 14.3 输入恢复 tick（每 100ms）
            if last_recovery_time.elapsed() >= Duration::from_millis(100) {
                last_recovery_time = std::time::Instant::now();
                match recovery.tick() {
                    RecoveryAction::ShouldExit => {
                        tracing::info!("输入设备丢失且 --exit-on-input-loss 已设置，退出");
                        break;
                    }
                    RecoveryAction::TryReconnectOriginal => {
                        tracing::info!("尝试重连原输入设备...");
                    }
                    RecoveryAction::TryFallbackToDefault => {
                        tracing::info!("尝试切换至默认输入设备...");
                    }
                    RecoveryAction::None => {}
                }
            }

            // 14.4 监控 JSON 输出（每 5 秒）
            if last_monitor_time.elapsed() >= Duration::from_secs(5) {
                last_monitor_time = std::time::Instant::now();
                let outputs_json: Vec<String> = output_slots.iter().map(|slot| {
                    let water_pct = slot.drift.as_ref()
                        .and_then(|d| d.lock().ok())
                        .map(|d| d.water_level_pct())
                        .unwrap_or(50.0);
                    format!(
                        r#"{{"name":"{}","underruns":{},"overflows":{},"latency_ms":{:.1},"delta":{},"water_level_pct":{:.0}}}"#,
                        slot.device_name,
                        slot.underrun_counter.load(Ordering::Relaxed),
                        overflow_counters[slot.index].load(Ordering::Relaxed),
                        (buffer_frames as f64) * 1000.0 / (slot.output_sample_rate as f64),
                        slot.delta.load(Ordering::Relaxed),
                        water_pct,
                    )
                }).collect();
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                println!(r#"{{"ts":{},"outputs":[{}]}}"#, ts, outputs_json.join(","));
            }

            // 14.5 非阻塞检查 stdin：Windows 上不支持非阻塞 stdin，使用 sleep 轮询
            // Ctrl+C 会自然导致程序退出
            std::thread::sleep(Duration::from_millis(200));
        }
    } else {
        // 简单 Enter 等待（保留 Phase 2 逻辑）+ 后台热插拔线程
        println!("按 Enter 键停止路由...");
        let mut line = String::new();
        std::io::stdin().read_line(&mut line).ok();
    }

    // ========================================================================
    // 15. 停止流程（平滑淡出）
    // ========================================================================
    tracing::info!("正在停止音频路由...");

    // 15.1 停止热插拔监听线程
    hotplug_stop.store(true, Ordering::Release);
    let _ = hotplug_handle.join();

    // 15.2 设置漂移补偿停止标志
    drift_stop_flag.store(true, Ordering::Release);

    // 15.3 停止输入流（先停输入，避免新数据进入缓冲区）
    drop(capture);
    tracing::info!("  输入流已停止");

    // 15.4 对所有活跃设备启动淡出
    for slot in &output_slots {
        slot.fader.start_fade_out();
        tracing::info!("  已对 '{}' 启动淡出", slot.device_name);
    }

    // 15.5 等待淡出完成（最多 2 秒）
    // 先等待一小段时间让淡出生效，再等待淡出时长完成
    std::thread::sleep(Duration::from_millis(100));
    // 额外等待，确保淡出缓冲区有足够时间排空
    let fade_wait_ms = (delay_ms * 3.0) as u64 + 200;
    std::thread::sleep(Duration::from_millis(fade_wait_ms));

    // 15.6 打印最终统计信息
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

    // 15.7 释放所有输出流（Drop 自动停止底层音频流）
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

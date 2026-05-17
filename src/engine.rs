// 音频路由器 — 后台音频引擎线程模块
// 在独立线程中运行音频管道，响应 GUI 的启动/停止消息并反馈状态

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use ringbuf::traits::Split;

use crate::audio::{CaptureStream, PlaybackStream};
use crate::channel_map::ChannelMapper;
use crate::device::{self, DeviceInfo};
use crate::error::{AudioRouterError, Result};
use crate::hotplug::{start_hotplug_monitor, HotplugEvent};
use crate::limiter::BrickwallLimiter;
use crate::message::{EngineConfig, EngineToGui, GuiToEngine, OutputSnapshot};
use crate::pipeline::{Fader, SlotArray, MAX_OUTPUTS};
use crate::recovery::{InputRecoveryManager, RecoveryAction};
use crate::resample::{ResampleProcessor, ResamplerType};
use crate::source::SourceType;

/// 输出槽位：将播放流及其元数据捆绑在一起
#[allow(dead_code)]
struct OutputSlot {
    playback: PlaybackStream,
    index: usize,
    fader: Arc<Fader>,
    underrun_counter: Arc<AtomicU64>,
    device_name: String,
    is_passthrough: bool,
    output_channels: u16,
    resampler: Arc<Mutex<ResampleProcessor>>,
    limiter: Arc<Mutex<BrickwallLimiter>>,
    delta: Arc<AtomicI32>,
    drift: Option<Arc<Mutex<crate::drift::DriftCompensator>>>,
    input_lost: Arc<AtomicBool>,
    output_sample_rate: u32,
}

/// 在独立线程中运行音频引擎，监听 GUI 消息并反馈状态
///
/// # 参数
/// * `gui_rx` — 接收 GUI → 引擎 方向的消息（Start/Stop）
/// * `gui_tx` — 发送 引擎 → GUI 方向的消息（Started/Stopped/Error/Log）
pub fn spawn_engine(
    gui_rx: Receiver<GuiToEngine>,
    gui_tx: Sender<EngineToGui>,
) {
    std::thread::spawn(move || {
        tracing::info!("引擎后台线程已启动，等待 GUI 指令...");

        // 阻塞等待第一条 Start 消息
        match gui_rx.recv() {
            Ok(GuiToEngine::Start(config)) => {
                tracing::info!("引擎线程收到启动指令");
                let result = run_pipeline(config, &gui_tx, &gui_rx);
                match result {
                    Ok(stats) => {
                        let _ = gui_tx.send(EngineToGui::Stopped { stats });
                    }
                    Err(e) => {
                        let _ = gui_tx.send(EngineToGui::Error(e.to_string()));
                    }
                }
            }
            Ok(GuiToEngine::Stop) => {
                // 引擎尚未启动，忽略 Stop
                tracing::debug!("引擎线程在启动前收到 Stop，已忽略");
            }
            Err(_) => {
                // 通道已关闭（GUI 退出）
                tracing::debug!("GUI 通道已关闭，引擎线程退出");
            }
        }

        tracing::info!("引擎后台线程退出");
    });
}

/// 运行音频管道主流程
///
/// 从 EngineConfig 构建完整的音频扇出管道并进入主循环。
/// 主循环中非阻塞轮询 gui_rx 接收 Stop 消息。
fn run_pipeline(
    config: EngineConfig,
    gui_tx: &Sender<EngineToGui>,
    gui_rx: &Receiver<GuiToEngine>,
) -> Result<Vec<OutputSnapshot>> {
    // ========================================================================
    // 1. 枚举并记录设备
    // ========================================================================
    enumerate_and_log_devices()?;

    // ========================================================================
    // 2. 选择输入设备和输出设备
    // ========================================================================
    let source_type = SourceType::from_str(&config.source_type);
    tracing::info!("音源类型: {}", source_type.as_str());

    let (device_source, info_source) = match source_type {
        SourceType::Loopback => {
            tracing::info!("音源类型: Loopback（系统音频回采）");
            device::select_loopback_device(config.loopback_device.as_deref())?
        }
        SourceType::InputDevice => {
            tracing::info!("音源类型: 物理输入设备");
            device::select_input_device(config.input_device.as_deref())?
        }
    };
    tracing::info!("音源设备: {}", info_source.name);

    let output_devices = device::select_output_devices(&config.output_devices)?;
    if output_devices.is_empty() {
        let err_msg = "没有可用的输出设备".to_string();
        return Err(AudioRouterError::DeviceNotFound(err_msg));
    }
    tracing::info!("选择了 {} 个输出设备", output_devices.len());

    // ========================================================================
    // 3. 构建输入流配置 + max-latency-ms 校验
    // ========================================================================
    let buffer_frames = config.buffer_frames;
    let sample_rate = config.sample_rate;
    let channels = 2u16;

    let delay_ms = (buffer_frames as f64) * 1000.0 / (sample_rate as f64);
    if delay_ms > config.max_latency_ms as f64 {
        return Err(AudioRouterError::ConfigError(format!(
            "缓冲区延迟 {:.1}ms 超过最大允许延迟 {}ms",
            delay_ms,
            config.max_latency_ms
        )));
    }

    let input_config = cpal::StreamConfig {
        channels,
        sample_rate: cpal::SampleRate(sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    // ========================================================================
    // 4. 创建 SlotArray 和 overflow_counters
    // ========================================================================
    let slot_array = Arc::new(SlotArray::new());
    let overflow_counters: Arc<[AtomicU64; MAX_OUTPUTS]> =
        Arc::new(std::array::from_fn(|_| AtomicU64::new(0)));

    // ========================================================================
    // 5. 为每个输出设备创建管道
    // ========================================================================
    let mut output_slots: Vec<OutputSlot> = Vec::new();

    for (device_out, info_out) in &output_devices {
        let output_channels = info_out
            .channels
            .first()
            .copied()
            .unwrap_or(channels);

        let output_sample_rate = if info_out.sample_rates.is_empty() {
            sample_rate
        } else if info_out.sample_rates.contains(&sample_rate) {
            sample_rate
        } else {
            let mut rates: Vec<&u32> = info_out.sample_rates.iter().collect();
            rates.sort_by(|a, b| {
                let da = (**a as i64 - sample_rate as i64).abs();
                let db = (**b as i64 - sample_rate as i64).abs();
                da.cmp(&db)
            });
            *rates[0]
        };

        // resampler none 校验
        if config.resampler == "none" && output_sample_rate != sample_rate {
            return Err(AudioRouterError::ConfigError(format!(
                "设备 '{}' 需要重采样（{}Hz→{}Hz），但 --resampler none",
                info_out.name, sample_rate, output_sample_rate
            )));
        }

        // 创建 SPSC 环形缓冲区
        let max_channels = channels.max(output_channels) as usize;
        let rb_capacity = buffer_frames as usize * max_channels * 4;
        let rb: Arc<ringbuf::HeapRb<f32>> =
            ringbuf::HeapRb::<f32>::new(rb_capacity).into();
        let (producer, consumer) = rb.split();

        // 分配槽位
        let slot_index = slot_array
            .allocate_slot(producer)
            .ok_or_else(|| {
                AudioRouterError::Fatal(format!(
                    "无法为输出设备 '{}' 分配槽位：已达到最大槽位数 {}",
                    info_out.name, MAX_OUTPUTS
                ))
            })?;

        // 声道映射器
        let channel_mapper = Arc::new(ChannelMapper::new(channels, output_channels));
        let is_passthrough = channel_mapper.is_passthrough();

        // 淡入淡出处理器
        let fade_len = ((5 * sample_rate as usize) / 1000).max(1);
        let fader = Arc::new(Fader::new(fade_len));

        // 欠载计数器
        let underrun_counter = Arc::new(AtomicU64::new(0));

        // 重采样器
        let resampler_type = match config.resampler.as_str() {
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
            )
            .map_err(|e| {
                AudioRouterError::ConfigError(format!("创建重采样器失败: {}", e))
            })?,
        ));

        // 砖墙限幅器
        let limiter = Arc::new(Mutex::new(BrickwallLimiter::new(
            1.0,
            0.0,
            10.0,
            output_channels as usize,
            output_sample_rate as f64,
            !config.no_limiter,
        )));

        // 漂移补偿器
        let drift_enabled = !config.no_drift_compensation;
        let drift = if drift_enabled {
            Some(Arc::new(Mutex::new(
                crate::drift::DriftCompensator::new(
                    true,
                    sample_rate as f64,
                    buffer_frames as usize,
                ),
            )))
        } else {
            None
        };
        let delta = Arc::new(AtomicI32::new(0));

        // 输入丢失标志
        let input_lost = Arc::new(AtomicBool::new(false));

        // 创建输出回调
        let output_callback = crate::pipeline::create_output_callback(
            consumer,
            Arc::clone(&fader),
            Arc::clone(&channel_mapper),
            channels,
            output_channels,
            sample_rate,
            output_sample_rate,
            Arc::clone(&underrun_counter),
            Arc::clone(&resampler),
            Arc::clone(&limiter),
            Arc::clone(&delta),
            Arc::clone(&input_lost),
            config.no_drift_compensation,
        );

        // 构建输出流配置
        let output_config = cpal::StreamConfig {
            channels: output_channels,
            sample_rate: cpal::SampleRate(output_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        // 创建输出播放流
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
            if slot.is_passthrough {
                "直通"
            } else {
                "声道转换"
            },
            fade_len,
        );

        output_slots.push(slot);
    }

    // ========================================================================
    // 6. 等待所有输出流就绪
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
    // 7. 输入丢失恢复管理器
    // ========================================================================
    let mut recovery = InputRecoveryManager::new(
        config.exit_on_input_loss,
        config.input_fallback_to_default,
        Some(info_source.name.clone()),
    );

    // ========================================================================
    // 8. 根据音源类型创建捕获流
    // ========================================================================
    let on_input = crate::pipeline::create_input_callback(
        Arc::clone(&slot_array),
        Arc::clone(&overflow_counters),
    );
    let capture = match source_type {
        SourceType::Loopback => {
            CaptureStream::from_loopback(&device_source, &input_config, on_input)?
        }
        SourceType::InputDevice => {
            CaptureStream::new(&device_source, &input_config, on_input)?
        }
    };
    tracing::info!("音源流已启动（{}）", source_type.as_str());

    // ========================================================================
    // 9. 热插拔监控
    // ========================================================================
    let (hotplug_tx, hotplug_rx) = crossbeam_channel::unbounded();
    let hotplug_stop = Arc::new(AtomicBool::new(false));
    let hotplug_handle = start_hotplug_monitor(
        hotplug_tx,
        Arc::clone(&hotplug_stop),
        Duration::from_secs(2),
    );

    // ========================================================================
    // 10. 通知 GUI 引擎已启动
    // ========================================================================
    let _ = gui_tx.send(EngineToGui::Started);

    // ========================================================================
    // 11. 打印启动信息
    // ========================================================================
    tracing::info!("==========================================");
    tracing::info!("  音频路由器 Phase 3 已启动");
    tracing::info!(
        "  音源设备: {}（类型: {}）",
        info_source.name,
        source_type.as_str()
    );
    tracing::info!("  输入配置: {}Hz / {}ch", sample_rate, channels);
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
            if config.no_limiter { "禁用" } else { "启用" },
        );
    }
    tracing::info!(
        "  缓冲区: {} 帧 (~{:.1}ms) | 漂移补偿: {}",
        buffer_frames,
        delay_ms,
        if config.no_drift_compensation {
            "禁用"
        } else {
            "启用"
        },
    );
    tracing::info!("==========================================");

    // ========================================================================
    // 12. 主循环：非阻塞轮询 GUI Stop 消息
    // ========================================================================
    let mut last_drift_time = Instant::now();
    let mut last_recovery_time = Instant::now();
    let mut last_status_time = Instant::now();

    loop {
        // 12.1 检查 GUI Stop 消息（非阻塞）
        match gui_rx.try_recv() {
            Ok(GuiToEngine::Stop) => {
                tracing::info!("引擎线程收到停止指令");
                break;
            }
            Ok(GuiToEngine::Start(_)) => {
                // 引擎已在运行，忽略重复启动
                tracing::warn!("引擎已在运行，忽略重复启动指令");
            }
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                // GUI 已退出
                tracing::info!("GUI 通道已关闭，引擎退出");
                break;
            }
            Err(crossbeam_channel::TryRecvError::Empty) => {
                // 无消息，正常继续
            }
        }

        // 12.2 检查热插拔事件
        while let Ok(event) = hotplug_rx.try_recv() {
            match event {
                HotplugEvent::DeviceAdded(info) => {
                    tracing::info!("检测到新输出设备: {}", info.name);
                }
                HotplugEvent::DeviceRemoved(name) => {
                    tracing::info!("输出设备已移除: {}", name);
                    if let Some(slot) =
                        output_slots.iter().find(|s| s.device_name == name)
                    {
                        slot_array.deactivate_slot(slot.index);
                        tracing::info!("  槽位 #{} 已停用", slot.index);
                    }
                }
            }
        }

        // 12.3 漂移补偿更新（每 100ms）
        if last_drift_time.elapsed() >= Duration::from_millis(100) {
            last_drift_time = Instant::now();
            for slot in &output_slots {
                if let Some(ref drift) = slot.drift {
                    if let Ok(mut d) = drift.lock() {
                        d.update(0.5);
                        slot.delta.store(d.delta_val(), Ordering::Release);
                    }
                }
            }
        }

        // 12.4 输入恢复 tick（每 100ms）
        if last_recovery_time.elapsed() >= Duration::from_millis(100) {
            last_recovery_time = Instant::now();
            match recovery.tick() {
                RecoveryAction::ShouldExit => {
                    tracing::info!(
                        "输入设备丢失且 --exit-on-input-loss 已设置，退出"
                    );
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

        // 12.5 推送状态到 GUI（每 500ms）
        if last_status_time.elapsed() >= Duration::from_millis(500) {
            last_status_time = Instant::now();
            let statuses: Vec<OutputSnapshot> = output_slots
                .iter()
                .map(|slot| {
                    let water_pct = slot
                        .drift
                        .as_ref()
                        .and_then(|d| d.lock().ok())
                        .map(|d| d.water_level_pct())
                        .unwrap_or(50.0);
                    OutputSnapshot {
                        device_name: slot.device_name.clone(),
                        underrun_count: slot.underrun_counter.load(Ordering::Relaxed),
                        overflow_count: overflow_counters[slot.index]
                            .load(Ordering::Relaxed),
                        latency_ms: (buffer_frames as f64) * 1000.0
                            / (slot.output_sample_rate as f64),
                        delta: slot.delta.load(Ordering::Relaxed),
                        water_level_pct: water_pct,
                    }
                })
                .collect();
            let _ = gui_tx.send(EngineToGui::OutputStatus(statuses));
        }

        // 轮询间隔 50ms
        std::thread::sleep(Duration::from_millis(50));
    }

    // ========================================================================
    // 13. 停止流程（平滑淡出）
    // ========================================================================
    tracing::info!("正在停止音频路由...");

    // 停止热插拔监听线程
    hotplug_stop.store(true, Ordering::Release);
    let _ = hotplug_handle.join();

    // 停止输入流（先停输入，避免新数据进入缓冲区）
    drop(capture);
    tracing::info!("  输入流已停止");

    // 对所有活跃设备启动淡出
    for slot in &output_slots {
        slot.fader.start_fade_out();
        tracing::info!("  已对 '{}' 启动淡出", slot.device_name);
    }

    // 等待淡出完成
    std::thread::sleep(Duration::from_millis(100));
    let fade_wait_ms = (delay_ms * 3.0) as u64 + 200;
    std::thread::sleep(Duration::from_millis(fade_wait_ms));

    // 收集最终统计信息
    let stats: Vec<OutputSnapshot> = output_slots
        .iter()
        .map(|slot| OutputSnapshot {
            device_name: slot.device_name.clone(),
            underrun_count: slot.underrun_counter.load(Ordering::Relaxed),
            overflow_count: overflow_counters[slot.index].load(Ordering::Relaxed),
            latency_ms: (buffer_frames as f64) * 1000.0
                / (slot.output_sample_rate as f64),
            delta: slot.delta.load(Ordering::Relaxed),
            water_level_pct: slot
                .drift
                .as_ref()
                .and_then(|d| d.lock().ok())
                .map(|d| d.water_level_pct())
                .unwrap_or(50.0),
        })
        .collect();

    // 打印最终统计
    tracing::info!("--- 最终统计 ---");
    for stat in &stats {
        tracing::info!(
            "  {} | 溢出: {} | 欠载: {}",
            stat.device_name,
            stat.overflow_count,
            stat.underrun_count,
        );
    }

    // 释放所有输出流
    drop(output_slots);

    tracing::info!("音频路由器已停止");
    Ok(stats)
}

/// 枚举所有输入和输出设备，以 info 级别打印详细信息
fn enumerate_and_log_devices() -> Result<()> {
    let inputs = device::enumerate_input_devices()?;
    tracing::info!("发现 {} 个输入设备:", inputs.len());
    for info in &inputs {
        log_device_info(info, "  [IN] ");
    }

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

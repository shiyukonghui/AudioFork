// 音频输出流封装模块
// 封装 cpal::Stream，提供就绪检测、暂停/恢复、生命周期管理等功能

use cpal::traits::{DeviceTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ============================================================================
// PlaybackStream — 音频输出流封装
// ============================================================================

/// 音频输出流封装，管理 cpal 输出流的完整生命周期和运行状态
pub struct PlaybackStream {
    /// 底层 cpal 音频输出流
    #[allow(dead_code)]
    stream: cpal::Stream,
    /// 流是否已就绪（首次收到音频数据回调后置为 true）
    ready: Arc<AtomicBool>,
    /// 流是否处于暂停状态
    #[allow(dead_code)]
    paused: Arc<AtomicBool>,
}

impl PlaybackStream {
    /// 创建新的音频输出流
    ///
    /// # 参数
    /// * `device` — 音频输出设备
    /// * `config` — 流配置（采样率、通道数、采样格式等）
    /// * `on_data` — 音频数据回调闭包，当系统请求音频数据时被调用，
    ///   传入可变 f32 切片供填充音频样本
    ///
    /// # 返回
    /// * `Ok(Self)` — 流创建成功并已开始运行
    /// * `Err(AudioRouterError::StreamError)` — 流创建失败
    ///
    /// # 实现细节
    /// 用 `Arc<Mutex<F>>` 包装原始回调，解决 `FnMut` 不能同时被多个闭包
    /// 共享的问题。首次收到回调时设置 `ready` 标志，方便上层轮询等待。
    pub fn new<F>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        on_data: F,
    ) -> crate::error::Result<Self>
    where
        F: FnMut(&mut [f32]) + Send + 'static,
    {
        // 就绪标志：流开始产出音频数据后置 true
        let ready = Arc::new(AtomicBool::new(false));
        let ready_clone = Arc::clone(&ready);

        // 暂停状态标志：由 pause()/resume() 方法更新
        let paused = Arc::new(AtomicBool::new(false));

        // FnMut 回调不能被多个闭包共享，需要用 Arc<Mutex<>> 包装
        let on_data = Arc::new(Mutex::new(on_data));
        let on_data_clone = Arc::clone(&on_data);

        // 包装后的数据回调：首次触发时标记就绪，然后调用原始回调
        // cpal 0.15 回调签名为 FnMut(&mut [f32], &OutputCallbackInfo)
        let data_callback = move |data: &mut [f32], _info: &cpal::OutputCallbackInfo| {
            // 首次回调 — 标记流已就绪
            if !ready_clone.load(Ordering::Relaxed) {
                ready_clone.store(true, Ordering::SeqCst);
            }
            // 调用用户提供的原始回调
            if let Ok(mut cb) = on_data_clone.lock() {
                cb(data);
            }
        };

        // 流错误回调：记录错误日志
        let error_callback = move |err: cpal::StreamError| {
            tracing::error!("音频输出流错误: {}", err);
        };

        // 构建 cpal 输出流，失败时转为统一错误类型
        let stream = device
            .build_output_stream(config, data_callback, error_callback, None)
            .map_err(|e: cpal::BuildStreamError| crate::error::AudioRouterError::StreamError(e.to_string()))?;

        // 启动音频流（cpal 创建的流默认处于暂停状态，必须调用 play() 才能触发回调）
        stream
            .play()
            .map_err(|e: cpal::PlayStreamError| crate::error::AudioRouterError::StreamError(format!("无法启动输出流: {}", e)))?;

        Ok(Self {
            stream,
            ready,
            paused,
        })
    }

    /// 查询流是否已就绪（曾收到过至少一次音频数据回调）
    ///
    /// 用于检测流是否真正开始工作，避免在流尚未准备好时进行后续操作。
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Relaxed)
    }

    /// 阻塞等待流就绪，直到超时或就绪
    ///
    /// # 参数
    /// * `timeout` — 最长等待时间，超过此时间仍未就绪则返回 false
    ///
    /// # 返回
    /// * `true` — 流已在超时前就绪
    /// * `false` — 超时仍未就绪
    ///
    /// # 实现细节
    /// 以 50ms 为间隔轮询就绪标志，兼顾响应速度与 CPU 开销。
    pub fn wait_ready(&self, timeout: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if self.is_ready() {
                return true;
            }
            // 轮询间隔 50ms
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    /// 暂停音频输出流
    ///
    /// 调用后系统不再请求音频数据，直到调用 `resume()` 恢复。
    #[allow(dead_code)]
    pub fn pause(&self) -> crate::error::Result<()> {
        self.stream
            .pause()
            .map_err(|e: cpal::PauseStreamError| crate::error::AudioRouterError::StreamError(e.to_string()))?;
        self.paused.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// 恢复音频输出流
    ///
    /// 恢复后系统重新开始请求音频数据。
    #[allow(dead_code)]
    pub fn resume(&self) -> crate::error::Result<()> {
        self.stream
            .play()
            .map_err(|e: cpal::PlayStreamError| crate::error::AudioRouterError::StreamError(e.to_string()))?;
        self.paused.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// 查询流是否处于暂停状态
    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

// ============================================================================
// Drop — 自动停止音频流
// ============================================================================

/// 析构时自动停止底层 cpal 流，确保资源正确释放
impl Drop for PlaybackStream {
    fn drop(&mut self) {
        // cpal::Stream 自身的 Drop 实现会停止并释放流资源，
        // 此处记录调试日志以便追踪流的生命周期
        tracing::debug!("PlaybackStream 析构 — 底层音频流已停止");
    }
}

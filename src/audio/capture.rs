// 音频路由器 — 输入流封装模块
// 封装 cpal::Stream，提供输入流生命周期管理与暂停/恢复控制

use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, StreamTrait};

use crate::error::AudioRouterError;

/// 输入音频流封装
///
/// 封装底层 `cpal::Stream`，提供：
/// - 构造时绑定设备与配置
/// - 暂停/恢复/状态查询
/// - 析构时自动停止流
pub struct CaptureStream {
    /// 底层 cpal 输入流
    #[allow(dead_code)]
    stream: cpal::Stream,
    /// 暂停状态标记（原子变量，支持内部可变性）
    #[allow(dead_code)]
    paused: AtomicBool,
}

impl CaptureStream {
    /// 从输入设备创建新的捕获流
    ///
    /// # 参数
    /// * `device` - 输入音频设备引用
    /// * `config` - 流配置（采样率、通道数、缓冲区大小等）
    /// * `on_data` - 音频数据回调，每次收到采样数据时被调用。
    ///   回调参数为 `&[f32]`，采样值范围约在 [-1.0, 1.0]。
    ///   回调必须实现 `Send + 'static`，因为在独立线程上执行。
    ///
    /// # 错误
    /// 无法从设备创建输入流时返回 `AudioRouterError::StreamError`
    pub fn from_input_device<F>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        mut on_data: F,
    ) -> crate::error::Result<Self>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        // 适配 cpal 0.15 的回调签名：FnMut(&[f32], &InputCallbackInfo)
        // 用闭包包装用户提供的 FnMut(&[f32])，忽略 InputCallbackInfo 参数
        let data_callback = move |data: &[f32], _info: &cpal::InputCallbackInfo| {
            on_data(data);
        };

        let stream = device
            .build_input_stream(
                config,
                data_callback,
                // 流错误回调：通过 tracing 记录错误信息
                |err| {
                    tracing::error!(?err, "捕获流内部错误");
                },
                // 超时时间：None 表示使用默认行为（非独占模式下忽略）
                None,
            )
            .map_err(|e: cpal::BuildStreamError| {
                AudioRouterError::StreamError(e.to_string())
            })?;

        // 启动音频输入流（cpal 创建的流默认处于暂停状态，必须调用 play() 才能触发回调）
        stream
            .play()
            .map_err(|e: cpal::PlayStreamError| {
                AudioRouterError::StreamError(format!("无法启动捕获流: {}", e))
            })?;

        // 流创建后默认处于播放（非暂停）状态
        Ok(Self {
            stream,
            paused: AtomicBool::new(false),
        })
    }

    /// 从输入设备创建新的捕获流（向后兼容别名）
    ///
    /// 实际调用 `from_input_device()`，保留此方法以兼容旧版调用代码。
    pub fn new<F>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        on_data: F,
    ) -> crate::error::Result<Self>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        Self::from_input_device(device, config, on_data)
    }

    /// 从输出设备的 Loopback 回采流创建捕获流（Windows WASAPI）
    ///
    /// 仅在 Windows 平台原生支持，其他平台需安装虚拟声卡。
    /// 通过 WASAPI 的 loopback 模式回采输出设备的音频数据。
    ///
    /// # 参数
    /// * `device` - 输出音频设备引用
    /// * `config` - 流配置（采样率、通道数、缓冲区大小等）
    /// * `on_data` - 音频数据回调，每次收到回采数据时被调用
    ///
    /// # 错误
    /// 非 Windows 平台返回 `AudioRouterError::NotSupported`
    pub fn from_loopback<F>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        mut on_data: F,
    ) -> crate::error::Result<Self>
    where
        F: FnMut(&[f32]) + Send + 'static,
    {
        // 非 Windows 平台：返回 NotSupported 错误
        if !crate::source::is_loopback_native_supported() {
            return Err(crate::error::AudioRouterError::NotSupported(
                crate::source::loopback_unsupported_message().to_string(),
            ));
        }
        // Windows 平台：对输出设备创建输入流（WASAPI 自动处理 loopback）
        // 适配 cpal 0.15 的回调签名
        let data_callback = move |data: &[f32], _info: &cpal::InputCallbackInfo| {
            on_data(data);
        };
        let stream = device
            .build_input_stream(
                config,
                data_callback,
                |err| {
                    tracing::error!(?err, "Loopback 捕获流内部错误");
                },
                None,
            )
            .map_err(|e: cpal::BuildStreamError| {
                crate::error::AudioRouterError::StreamError(format!(
                    "无法创建 Loopback 捕获流: {}",
                    e
                ))
            })?;

        // 启动音频输入流（cpal 创建的流默认处于暂停状态，必须调用 play() 才能触发回调）
        stream
            .play()
            .map_err(|e: cpal::PlayStreamError| {
                crate::error::AudioRouterError::StreamError(format!(
                    "无法启动 Loopback 捕获流: {}",
                    e
                ))
            })?;

        Ok(Self {
            stream,
            paused: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// 暂停输入流
    ///
    /// 暂停后不再触发 `on_data` 回调，直到调用 `resume()` 恢复。
    #[allow(dead_code)]
    pub fn pause(&self) -> crate::error::Result<()> {
        self.stream
            .pause()
            .map_err(|e: cpal::PauseStreamError| {
                AudioRouterError::StreamError(e.to_string())
            })?;
        self.paused.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// 恢复输入流
    ///
    /// 恢复后继续触发 `on_data` 回调接收音频数据。
    #[allow(dead_code)]
    pub fn resume(&self) -> crate::error::Result<()> {
        self.stream
            .play()
            .map_err(|e: cpal::PlayStreamError| {
                AudioRouterError::StreamError(e.to_string())
            })?;
        self.paused.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// 查询当前是否处于暂停状态
    #[allow(dead_code)]
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }
}

impl Drop for CaptureStream {
    /// 析构时 `cpal::Stream` 会自动停止底层音频流，
    /// 确保资源正确释放，不会产生悬空的音频回调。
    fn drop(&mut self) {
        // cpal::Stream 自身的 Drop 实现会调用停止逻辑，
        // 此处无需额外操作，流随结构体一同析构。
    }
}

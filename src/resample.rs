//! 音频重采样器封装模块
//!
//! 基于 rubato v2 库对音频数据进行采样率转换。
//! 支持 Sinc（异步高质量）、Cubic（同步 FFT）和 PassThrough（直通）三种模式。

use rubato::{
    Async, Fft, FixedAsync, FixedSync, Indexing, ResampleError, Resampler,
    ResamplerConstructionError, SincInterpolationParameters, SincInterpolationType,
    WindowFunction,
};
use rubato::audioadapter_buffers::direct::SequentialSliceOfVecs;

// ============================================================================
// 类型别名
// ============================================================================

/// 重采样操作的 Result 别名
pub type ResampleResult = Result<(), String>;

// ============================================================================
// 枚举定义
// ============================================================================

/// 重采样算法类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResamplerType {
    /// 基于 Sinc 插值的高质量异步重采样，支持动态调整采样率
    Sinc,
    /// 基于 FFT 的同步重采样，速度快质量好，不支持动态调整采样率
    Cubic,
    /// 不进行重采样，数据直接通过
    None,
}

/// 使用 Sinc 插值的异步重采样器状态
struct SincState {
    /// rubato 异步重采样器实例
    resampler: Async<f32>,
    /// 当前输入采样率 (Hz)
    input_rate: f64,
    /// 当前输出采样率 (Hz)
    output_rate: f64,
}

/// 使用 FFT 的同步重采样器状态
struct CubicState {
    /// rubato FFT 重采样器实例
    resampler: Fft<f32>,
    /// 输入采样率 (Hz)，创建后固定
    input_rate: f64,
    /// 输出采样率 (Hz)，创建后固定
    output_rate: f64,
}

/// 重采样处理器，包装不同的 rubato 重采样器实现
///
/// 所有变体均实现 Send trait，可安全传入音频回调闭包。
pub enum ResampleProcessor {
    /// 使用 Sinc 插值的异步重采样器
    Sinc(SincState),
    /// 使用 FFT 的同步重采样器
    Cubic(CubicState),
    /// 直通模式，不进行任何重采样处理
    PassThrough,
}

// ============================================================================
// ResampleProcessor 实现
// ============================================================================

impl ResampleProcessor {
    /// 创建新的重采样处理器
    ///
    /// # 参数
    /// - `algorithm`: 重采样算法类型
    /// - `input_rate`: 输入采样率 (Hz)
    /// - `output_rate`: 输出采样率 (Hz)
    /// - `channels`: 声道数
    /// - `chunk_size`: 每次处理的帧数
    ///
    /// # 返回
    /// - `Ok(Self)`: 成功创建的重采样处理器
    /// - `Err(String)`: 创建失败时的错误描述
    pub fn new(
        algorithm: ResamplerType,
        input_rate: f64,
        output_rate: f64,
        channels: usize,
        chunk_size: usize,
    ) -> Result<Self, String> {
        // 如果输入输出采样率相同，或者算法为 None，返回直通模式
        if (input_rate - output_rate).abs() < f64::EPSILON
            || algorithm == ResamplerType::None
        {
            return Ok(ResampleProcessor::PassThrough);
        }

        match algorithm {
            ResamplerType::Sinc => {
                let ratio = output_rate / input_rate;
                let params = SincInterpolationParameters {
                    sinc_len: 256,
                    f_cutoff: 0.95,
                    oversampling_factor: 256,
                    interpolation: SincInterpolationType::Linear,
                    window: WindowFunction::BlackmanHarris2,
                };

                let resampler = Async::new_sinc(
                    ratio,
                    2.0,
                    &params,
                    chunk_size,
                    channels,
                    FixedAsync::Input,
                )
                .map_err(|e: ResamplerConstructionError| {
                    format!("创建 Sinc 重采样器失败: {:?}", e)
                })?;

                Ok(ResampleProcessor::Sinc(SincState {
                    resampler,
                    input_rate,
                    output_rate,
                }))
            }
            ResamplerType::Cubic => {
                let resampler = Fft::new(
                    input_rate as usize,
                    output_rate as usize,
                    chunk_size,
                    1,
                    channels,
                    FixedSync::Input,
                )
                .map_err(|e: ResamplerConstructionError| {
                    format!("创建 Fft(Cubic) 重采样器失败: {:?}", e)
                })?;

                Ok(ResampleProcessor::Cubic(CubicState {
                    resampler,
                    input_rate,
                    output_rate,
                }))
            }
            ResamplerType::None => Ok(ResampleProcessor::PassThrough),
        }
    }

    /// 处理音频数据块，进行重采样
    ///
    /// 将交错格式的输入音频数据重采样后写入交错格式的输出缓冲区。
    ///
    /// # 参数
    /// - `input`: 交错格式的输入音频数据（每帧包含所有声道的采样）
    /// - `output`: 交错格式的输出缓冲区
    ///
    /// # 返回
    /// - `Ok((consumed, produced))`: consumed 为消耗的输入帧数，produced 为产生的输出帧数
    /// - `Err(String)`: 处理失败时的错误描述
    pub fn process(
        &mut self,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<(usize, usize), String> {
        match self {
            ResampleProcessor::PassThrough => {
                // 直通模式：直接拷贝数据
                // 注意：PassThrough 没有存储声道数，这里按最小拷贝处理
                let copy_samples = input.len().min(output.len());
                output[..copy_samples].copy_from_slice(&input[..copy_samples]);
                let channels = 1; // PassThrough 按单声道处理帧计数
                let copy_frames = copy_samples / channels;
                Ok((copy_frames, copy_frames))
            }
            ResampleProcessor::Sinc(state) => {
                let channels = state.resampler.nbr_channels();
                process_with_resampler(
                    &mut state.resampler,
                    channels,
                    input,
                    output,
                )
            }
            ResampleProcessor::Cubic(state) => {
                let channels = state.resampler.nbr_channels();
                process_with_resampler(
                    &mut state.resampler,
                    channels,
                    input,
                    output,
                )
            }
        }
    }

    /// 获取下一次 process 调用所需的输入帧数
    pub fn input_frames_next(&self) -> usize {
        match self {
            ResampleProcessor::Sinc(state) => state.resampler.input_frames_next(),
            ResampleProcessor::Cubic(state) => state.resampler.input_frames_next(),
            ResampleProcessor::PassThrough => 0,
        }
    }

    /// 获取下一次 process 调用将产生的输出帧数
    pub fn output_frames_next(&self) -> usize {
        match self {
            ResampleProcessor::Sinc(state) => state.resampler.output_frames_next(),
            ResampleProcessor::Cubic(state) => state.resampler.output_frames_next(),
            ResampleProcessor::PassThrough => 0,
        }
    }

    /// 获取重采样器的输出延迟（以输出帧为单位）
    ///
    /// 表示输入中的事件在输出中出现之前被延迟了多少帧。
    pub fn output_delay(&self) -> usize {
        match self {
            ResampleProcessor::Sinc(state) => state.resampler.output_delay(),
            ResampleProcessor::Cubic(state) => state.resampler.output_delay(),
            ResampleProcessor::PassThrough => 0,
        }
    }

    /// 重置重采样器状态，清空所有内部缓冲区
    pub fn reset(&mut self) {
        match self {
            ResampleProcessor::Sinc(state) => state.resampler.reset(),
            ResampleProcessor::Cubic(state) => state.resampler.reset(),
            ResampleProcessor::PassThrough => {}
        }
    }

    /// 判断是否为直通模式（不进行任何重采样）
    pub fn is_passthrough(&self) -> bool {
        matches!(self, ResampleProcessor::PassThrough)
    }

    /// 获取当前输入采样率 (Hz)
    pub fn input_sample_rate(&self) -> f64 {
        match self {
            ResampleProcessor::Sinc(state) => state.input_rate,
            ResampleProcessor::Cubic(state) => state.input_rate,
            ResampleProcessor::PassThrough => 0.0,
        }
    }

    /// 获取当前输出采样率 (Hz)
    pub fn output_sample_rate(&self) -> f64 {
        match self {
            ResampleProcessor::Sinc(state) => state.output_rate,
            ResampleProcessor::Cubic(state) => state.output_rate,
            ResampleProcessor::PassThrough => 0.0,
        }
    }

    /// 更新输入采样率（用于热插拔场景）
    ///
    /// 通过调整重采样比率来实现采样率变更。
    /// 注意：仅 Sinc 异步重采样器支持动态调整，Cubic(FFT) 不支持。
    pub fn set_input_sample_rate(&mut self, rate: f64) {
        match self {
            ResampleProcessor::Sinc(state) => {
                if state.output_rate > 0.0 && rate > 0.0 {
                    let new_ratio = state.output_rate / rate;
                    let _ = state.resampler.set_resample_ratio(new_ratio, false);
                    state.input_rate = rate;
                }
            }
            ResampleProcessor::Cubic(_) => {
                // Fft 是同步重采样器，不支持动态调整比率
            }
            ResampleProcessor::PassThrough => {}
        }
    }

    /// 更新输出采样率（用于热插拔场景）
    ///
    /// 通过调整重采样比率来实现采样率变更。
    /// 注意：仅 Sinc 异步重采样器支持动态调整，Cubic(FFT) 不支持。
    pub fn set_output_sample_rate(&mut self, rate: f64) {
        match self {
            ResampleProcessor::Sinc(state) => {
                if state.input_rate > 0.0 && rate > 0.0 {
                    let new_ratio = rate / state.input_rate;
                    let _ = state.resampler.set_resample_ratio(new_ratio, false);
                    state.output_rate = rate;
                }
            }
            ResampleProcessor::Cubic(_) => {
                // Fft 是同步重采样器，不支持动态调整比率
            }
            ResampleProcessor::PassThrough => {}
        }
    }
}

// ============================================================================
// 内部辅助函数
// ============================================================================

/// 使用重采样器处理交错格式的音频数据（Sinc 和 Cubic 共用逻辑）
///
/// 将交错格式 `&[f32]` 转换为 `Vec<Vec<f32>>`（每声道一个 Vec），
/// 调用 rubato 的 `process_into_buffer`，再将结果转回交错格式写入 output。
fn process_with_resampler<R: Resampler<f32>>(
    resampler: &mut R,
    channels: usize,
    input: &[f32],
    output: &mut [f32],
) -> Result<(usize, usize), String> {
    // 计算输入帧数
    let input_frames = input.len() / channels;

    // 将交错格式拆分为每个声道独立的 Vec
    let mut deinterleaved_input: Vec<Vec<f32>> = (0..channels)
        .map(|ch| {
            let mut chan_data = Vec::with_capacity(input_frames);
            for f in 0..input_frames {
                chan_data.push(input[f * channels + ch]);
            }
            chan_data
        })
        .collect();

    // 预分配输出缓冲区（每个声道独立）
    let output_frames_next = resampler.output_frames_next();
    let mut deinterleaved_output: Vec<Vec<f32>> = (0..channels)
        .map(|_| vec![0.0f32; output_frames_next])
        .collect();

    // 创建 rubato 的 Adapter 包装
    let input_adapter =
        SequentialSliceOfVecs::new(&deinterleaved_input, channels, input_frames)
            .map_err(|e| format!("创建输入适配器失败: {:?}", e))?;

    let mut output_adapter =
        SequentialSliceOfVecs::new_mut(&mut deinterleaved_output, channels, output_frames_next)
            .map_err(|e| format!("创建输出适配器失败: {:?}", e))?;

    // 构造 Indexing，使用 partial_len 处理可能的不足帧情况
    let indexing = Indexing {
        input_offset: 0,
        output_offset: 0,
        partial_len: if input_frames < resampler.input_frames_next() {
            Some(input_frames)
        } else {
            None
        },
        active_channels_mask: None,
    };

    // 调用 rubato 的重采样方法
    let (frames_read, frames_written) = resampler
        .process_into_buffer(&input_adapter, &mut output_adapter, Some(&indexing))
        .map_err(|e| format!("重采样处理失败: {:?}", e))?;

    // 将声道独立的输出转回交错格式
    let copy_frames = frames_written.min(output.len() / channels);
    for ch in 0..channels {
        for f in 0..copy_frames {
            output[f * channels + ch] = deinterleaved_output[ch][f];
        }
    }

    Ok((frames_read, frames_written))
}

/// 将 rubato 的 ResampleError 转换为 String 错误描述
#[allow(dead_code)]
fn resample_error_to_string(err: ResampleError) -> String {
    format!("{:?}", err)
}

// ============================================================================
// 测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试直通模式：采样率相同时应返回 PassThrough
    #[test]
    fn test_passthrough_when_rates_equal() {
        let proc = ResampleProcessor::new(
            ResamplerType::Sinc,
            48000.0,
            48000.0,
            2,
            1024,
        )
        .unwrap();
        assert!(proc.is_passthrough());
    }

    /// 测试 None 类型返回 PassThrough
    #[test]
    fn test_none_returns_passthrough() {
        let proc = ResampleProcessor::new(
            ResamplerType::None,
            44100.0,
            48000.0,
            2,
            1024,
        )
        .unwrap();
        assert!(proc.is_passthrough());
    }

    /// 测试 PassThrough 的 process 方法直接拷贝数据
    #[test]
    fn test_passthrough_process() {
        let mut proc = ResampleProcessor::new(
            ResamplerType::None,
            44100.0,
            48000.0,
            2,
            1024,
        )
        .unwrap();

        let input = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mut output = vec![0.0f32; 6];

        let (consumed, produced) = proc.process(&input, &mut output).unwrap();
        assert_eq!(consumed, 6); // PassThrough 按单声道计帧数，6 samples = 6 frames
        assert_eq!(produced, 6);
        assert_eq!(output, input);
    }

    /// 测试 Sinc 重采样器的创建
    #[test]
    fn test_create_sinc_resampler() {
        let proc = ResampleProcessor::new(
            ResamplerType::Sinc,
            44100.0,
            48000.0,
            2,
            1024,
        );
        assert!(proc.is_ok());
        let proc = proc.unwrap();
        assert!(!proc.is_passthrough());
        assert!(proc.input_frames_next() > 0);
        assert_eq!(proc.input_sample_rate(), 44100.0);
        assert_eq!(proc.output_sample_rate(), 48000.0);
    }

    /// 测试 Cubic(Fft) 重采样器的创建
    #[test]
    fn test_create_cubic_resampler() {
        let proc = ResampleProcessor::new(
            ResamplerType::Cubic,
            44100.0,
            48000.0,
            2,
            1024,
        );
        assert!(proc.is_ok());
        let proc = proc.unwrap();
        assert!(!proc.is_passthrough());
        assert!(proc.input_frames_next() > 0);
        assert_eq!(proc.input_sample_rate(), 44100.0);
        assert_eq!(proc.output_sample_rate(), 48000.0);
    }

    /// 测试 output_delay 和 reset 方法
    #[test]
    fn test_output_delay_and_reset() {
        let mut proc = ResampleProcessor::new(
            ResamplerType::Sinc,
            44100.0,
            48000.0,
            2,
            1024,
        )
        .unwrap();

        let delay = proc.output_delay();
        // Sinc 重采样器通常有非零延迟
        assert!(delay > 0);

        // reset 不应 panic
        proc.reset();
    }

    /// 测试 PassThrough 的辅助方法返回值
    #[test]
    fn test_passthrough_helpers() {
        let proc = ResampleProcessor::new(
            ResamplerType::None,
            44100.0,
            48000.0,
            2,
            1024,
        )
        .unwrap();

        assert!(proc.is_passthrough());
        assert_eq!(proc.input_frames_next(), 0);
        assert_eq!(proc.output_frames_next(), 0);
        assert_eq!(proc.output_delay(), 0);
        assert_eq!(proc.input_sample_rate(), 0.0);
        assert_eq!(proc.output_sample_rate(), 0.0);
    }

    /// 测试 ResamplerType 的 Debug 和比较
    #[test]
    fn test_resampler_type_equality() {
        assert_eq!(ResamplerType::Sinc, ResamplerType::Sinc);
        assert_ne!(ResamplerType::Sinc, ResamplerType::Cubic);
        assert_ne!(ResamplerType::Cubic, ResamplerType::None);
    }

    /// 测试 set_input_sample_rate / set_output_sample_rate
    #[test]
    fn test_rate_setters() {
        let mut proc = ResampleProcessor::new(
            ResamplerType::Sinc,
            44100.0,
            48000.0,
            2,
            1024,
        )
        .unwrap();

        // 更新输入采样率
        proc.set_input_sample_rate(22050.0);
        assert_eq!(proc.input_sample_rate(), 22050.0);
        // 输出采样率不变
        assert_eq!(proc.output_sample_rate(), 48000.0);

        // 更新输出采样率
        proc.set_output_sample_rate(96000.0);
        assert_eq!(proc.output_sample_rate(), 96000.0);
        assert_eq!(proc.input_sample_rate(), 22050.0);
    }
}

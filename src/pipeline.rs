// 音频路由器 — Phase 2 扇出管道核心模块
//
// 本模块实现 SPSC 槽位管理、余弦淡入淡出、扇出输入/输出回调。
// 架构：单个输入回调 → [环形缓冲区 × N] → 多个输出回调。
// 使用 UnsafeCell 实现零锁生产者访问（SPSC 天然保证独占写入）。

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use ringbuf::traits::{Consumer as _, Observer as _, Producer as _};

use crate::channel_map::ChannelMapper;

// ringbuf 0.5 的类型别名：split() 内部用 Arc 包装，concrete 类型是 CachingProd/CachingCons
type RbProducer = ringbuf::CachingProd<Arc<ringbuf::HeapRb<f32>>>;
type RbConsumer = ringbuf::CachingCons<Arc<ringbuf::HeapRb<f32>>>;

// ============================================================================
// (A) 常量
// ============================================================================

/// 最大输出设备数（槽位数量上限）
pub const MAX_OUTPUTS: usize = 32;

// ============================================================================
// (B) Slot 与 SlotArray
// ============================================================================

/// 单个 SPSC 槽位，持有一个环形缓冲区生产者端和活跃状态标记
///
/// Producer 包装在 UnsafeCell 中，因为 ringbuf 的 push_slice 需要 &mut self。
/// SPSC 架构保证输入回调是每个槽位的唯一生产者，因此通过 unsafe 获取 &mut 是安全的。
pub struct Slot {
    /// 环形缓冲区生产者端（UnsafeCell 实现内部可变性，零锁访问）
    pub producer: UnsafeCell<Option<RbProducer>>,
    /// 槽位是否活跃（有输出设备连接）
    pub active: AtomicBool,
}

// Slot 需要跨线程共享（Arc<SlotArray>），Producer 本身只实现 Send 不实现 Sync，
// 但通过 UnsafeCell + SPSC 独占写入保证，我们可以安全地标记为 Sync。
unsafe impl Sync for Slot {}

/// SPSC 槽位数组，管理最多 MAX_OUTPUTS 个输出连接
pub struct SlotArray {
    /// 固定长度的槽位列表，使用 Vec 简化初始化
    slots: Vec<Slot>,
}

impl SlotArray {
    /// 创建包含 MAX_OUTPUTS 个空槽位的槽位数组
    pub fn new() -> Self {
        let mut slots = Vec::with_capacity(MAX_OUTPUTS);
        for _ in 0..MAX_OUTPUTS {
            slots.push(Slot {
                producer: UnsafeCell::new(None),
                active: AtomicBool::new(false),
            });
        }
        Self { slots }
    }

    /// 分配一个空闲槽位，将 Producer 填入其中
    ///
    /// 遍历 slots 找到第一个 inactive 槽位，填入 producer 并标记为活跃。
    /// 返回槽位索引；若无空闲槽位则返回 None。
    pub fn allocate_slot(&self, producer: RbProducer) -> Option<usize> {
        for (i, slot) in self.slots.iter().enumerate() {
            // 先用 Acquire 读取，若为 true 则跳过（优化：避免重复 CAS）
            if slot.active.load(Ordering::Acquire) {
                continue;
            }
            // 用 compare_exchange 确保只有一个线程能成功分配该槽位
            if slot
                .active
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Relaxed)
                .is_ok()
            {
                // SAFETY: 槽位刚被标记为活跃，尚无其他生产者访问
                unsafe {
                    *slot.producer.get() = Some(producer);
                }
                return Some(i);
            }
        }
        None
    }

    /// 停用指定槽位（保留 Producer，使其可被重新激活或取出）
    pub fn deactivate_slot(&self, index: usize) {
        self.slots[index].active.store(false, Ordering::Release);
    }

    /// 获取所有活跃槽位的索引列表
    ///
    /// 使用 Acquire 读取 active 标志，确保与 allocate_slot 的 Release 形成同步关系。
    #[allow(dead_code)]
    pub fn active_slots(&self) -> Vec<usize> {
        let mut result = Vec::new();
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.active.load(Ordering::Acquire) {
                result.push(i);
            }
        }
        result
    }
}

// ============================================================================
// (C) 余弦淡入淡出窗表
// ============================================================================

/// 256 点余弦窗查找表，静态初始化一次
static COSINE_WINDOW: LazyLock<[f32; 256]> = LazyLock::new(|| {
    let mut table = [0.0f32; 256];
    for i in 0..256 {
        let phase = i as f32 / 255.0;
        // 余弦淡入淡出窗: 0.5 - 0.5 * cos(π * phase)
        table[i] = 0.5 - 0.5 * (std::f32::consts::PI * phase).cos();
    }
    table
});

/// 查询余弦窗增益值
///
/// phase 钳位到 [0.0, 1.0]，通过线性插值查找 256 点表。
/// 返回 0.0（静音）到 1.0（满幅）之间的增益系数。
#[inline]
pub fn cosine_fade_gain(phase: f32) -> f32 {
    // 钳位到 [0.0, 1.0]
    let p = phase.clamp(0.0, 1.0);

    // 计算浮点索引：0.0 ~ 255.0
    let fidx = p * 255.0;
    let idx0 = fidx as usize;
    let idx1 = (idx0 + 1).min(255);
    let frac = fidx - idx0 as f32;

    // 线性插值
    let table = &*COSINE_WINDOW;
    table[idx0] + (table[idx1] - table[idx0]) * frac
}

// ============================================================================
// (D) Fader 结构体
// ============================================================================

/// 原子淡入淡出处理器
///
/// 在音频回调中逐帧应用余弦淡入淡出增益，无需锁。
/// fade_count < 0 → 淡出中；fade_count > 0 → 淡入中；fade_count == 0 → 正常。
pub struct Fader {
    /// 当前淡入淡出剩余帧数（负数=淡出，正数=淡入，零=正常）
    fade_count: AtomicI32,
    /// 总淡入/淡出长度（帧数），初始化后只读
    fade_len: usize,
}

impl Fader {
    /// 创建新的淡入淡出处理器
    pub fn new(fade_len: usize) -> Self {
        Self {
            fade_count: AtomicI32::new(0),
            fade_len,
        }
    }

    /// 启动淡出（从正常到静音）
    pub fn start_fade_out(&self) {
        self.fade_count
            .store(-(self.fade_len as i32), Ordering::Release);
    }

    /// 启动淡入（从静音到正常）
    #[allow(dead_code)]
    pub fn start_fade_in(&self) {
        self.fade_count
            .store(self.fade_len as i32, Ordering::Release);
    }

    /// 逐帧处理淡入淡出增益，直接修改 buffer
    ///
    /// 以 channels 个样本为一帧，对 buffer 中的每一帧应用当前增益。
    /// 每帧处理后原子递增 fade_count 向 0 靠近。
    ///
    /// # 参数
    /// * `buffer` — 交错格式的音频样本缓冲区
    /// * `channels` — 声道数，决定帧边界
    pub fn process(&self, buffer: &mut [f32], channels: u16) {
        let ch = channels as usize;
        let total_frames = buffer.len() / ch;
        if total_frames == 0 {
            return;
        }

        let fade_len = self.fade_len as i32;
        let mut fc = self.fade_count.load(Ordering::Acquire);

        if fc == 0 {
            return; // 无淡入淡出
        }

        for frame in 0..total_frames {
            fc = self.fade_count.load(Ordering::Acquire);
            if fc == 0 {
                break; // 淡入淡出已结束，剩余帧不再处理
            }

            // 计算线性进度和余弦窗增益
            let abs_count = fc.abs() as usize;
            let progress = abs_count as f32 / fade_len as f32;
            let gain = cosine_fade_gain(progress);

            // 淡出时增益从 1.0 → 0.0，取 gain 本身
            // 淡入时增益从 0.0 → 1.0，取 1.0 - gain（反转窗）
            let actual_gain = if fc < 0 { gain } else { 1.0 - gain };

            // 对当前帧的所有声道样本应用增益
            let start = frame * ch;
            let end = start + ch;
            for sample in &mut buffer[start..end] {
                *sample *= actual_gain;
            }

            // 向 0 靠近
            if fc < 0 {
                self.fade_count.fetch_add(1, Ordering::AcqRel);
            } else {
                self.fade_count.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }
}

// ============================================================================
// (E) 扇出输入回调
// ============================================================================

/// 创建扇出输入回调闭包
///
/// 将捕获的音频数据通过 SPSC 环形缓冲区扇出到所有活跃输出槽位。
/// 零锁：通过 UnsafeCell 获取各 Producer 的 &mut 引用。
/// 零堆分配：回调内仅做简单迭代和原子操作。
///
/// # 参数
/// * `slot_array` — 槽位数组的共享引用
/// * `overflow_counters` — 各槽位的溢出计数器（用于监控/诊断）
pub fn create_input_callback(
    slot_array: Arc<SlotArray>,
    overflow_counters: Arc<[AtomicU64; MAX_OUTPUTS]>,
) -> impl FnMut(&[f32]) + Send + 'static {
    move |data: &[f32]| {
        if data.is_empty() {
            return;
        }
        let data_len = data.len();

        for (i, slot) in slot_array.slots.iter().enumerate() {
            // 先用 Acquire 读取活跃状态
            if !slot.active.load(Ordering::Acquire) {
                continue;
            }

            // SAFETY: SPSC 架构保证此输入回调是每个槽位的唯一生产者。
            // 同一槽位不会被多个线程同时写入。UnsafeCell 允许我们
            // 通过共享引用获取独占的 &mut 访问。
            let prod_opt = unsafe { &mut *slot.producer.get() };
            if let Some(ref mut prod) = prod_opt {
                // 检查缓冲区剩余空间是否足够
                if prod.vacant_len() >= data_len {
                    // 写入全部数据，push_slice 返回实际写入数
                    let written = prod.push_slice(data);
                    debug_assert_eq!(written, data_len);
                } else {
                    // 缓冲区空间不足 → 丢弃数据并记录溢出
                    overflow_counters[i].fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }
}

// ============================================================================
// (F) 输出回调
// ============================================================================

/// 创建输出回调闭包（Phase 3：集成重采样器、砖墙限幅器、漂移补偿、输入丢失标志）
///
/// 从环形缓冲区消费音频数据，经过声道映射、限幅、重采样和淡入淡出处理后写入输出设备。
///
/// # 参数
/// * `consumer`              — 环形缓冲区的消费者端（独占所有权）
/// * `fader`                 — 淡入淡出处理器共享引用
/// * `channel_mapper`        — 声道映射器共享引用
/// * `input_channels`        — 输入音频流的声道数
/// * `output_channels`       — 输出音频流的声道数
/// * `input_sample_rate`     — 输入采样率
/// * `output_sample_rate`    — 输出采样率
/// * `underrun_counter`      — 欠载计数器（用于监控/诊断）
/// * `resampler`             — 重采样处理器（Mutex 包装，线程安全）
/// * `limiter`               — 砖墙限幅器（Mutex 包装，线程安全）
/// * `delta_var`             — 时钟漂移补偿微调量（原子读取，+1/-1/0）
/// * `input_lost`            — 输入设备丢失标志（原子读取）
/// * `no_drift_compensation` — 是否禁用漂移补偿（true = 禁用）
pub fn create_output_callback(
    mut consumer: RbConsumer,
    fader: Arc<Fader>,
    channel_mapper: Arc<ChannelMapper>,
    input_channels: u16,
    output_channels: u16,
    input_sample_rate: u32,
    output_sample_rate: u32,
    underrun_counter: Arc<AtomicU64>,
    // ===== Phase 3 新增参数 =====
    resampler: Arc<Mutex<crate::resample::ResampleProcessor>>,
    limiter: Arc<Mutex<crate::limiter::BrickwallLimiter>>,
    delta_var: Arc<AtomicI32>,
    input_lost: Arc<AtomicBool>,
    no_drift_compensation: bool,
) -> impl FnMut(&mut [f32]) + Send + 'static {
    let ich = input_channels as usize;
    let och = output_channels as usize;

    // Phase 3 直通条件：采样率相同 + 声道数相同 + 漂移补偿禁用
    let is_passthrough = input_sample_rate == output_sample_rate
        && input_channels == output_channels
        && no_drift_compensation;

    // 最小转换路径：采样率和声道相同，但启用了漂移补偿
    // 仅做帧数微调（delta），不经过声道映射和重采样
    let is_minimal_convert = input_sample_rate == output_sample_rate
        && input_channels == output_channels
        && !no_drift_compensation;

    // 预分配临时缓冲区，避免音频回调中堆分配
    // 按最大预期帧数（4096）和最大声道数分配，覆盖绝大多数音频缓冲区大小
    let max_channels = ich.max(och);
    let max_tmp_samples = 4096 * max_channels;
    let mut tmp_buf: Vec<f32> = vec![0.0f32; max_tmp_samples];

    move |output: &mut [f32]| {
        let output_frames = output.len() / och;
        if output_frames == 0 {
            return;
        }

        // ---- a) 检查输入丢失标志 ----
        if input_lost.load(Ordering::Acquire) {
            // 输入设备已丢失 → 输出静音，启动淡出
            for sample in output.iter_mut() {
                *sample = 0.0;
            }
            fader.start_fade_out();
            fader.process(output, output_channels);
            return;
        }

        // ---- 读取漂移补偿 delta ----
        let delta = delta_var.load(Ordering::Acquire);

        // ================================================================
        // 路径 1：直通模式（采样率==声道都相同，且无漂移补偿）
        // ================================================================
        if is_passthrough {
            let needed_samples = output_frames * ich;

            if consumer.occupied_len() >= needed_samples {
                let pop_count = consumer.pop_slice(output);
                debug_assert_eq!(pop_count, needed_samples);
            } else {
                consumer.clear();
                fader.start_fade_out();
                underrun_counter.fetch_add(1, Ordering::Relaxed);
                for sample in output.iter_mut() {
                    *sample = 0.0;
                }
            }
            fader.process(output, output_channels);
            return;
        }

        // ================================================================
        // 路径 2：最小转换路径（采样率和声道相同，但启用了漂移补偿）
        // 仅做帧数微调，不经过声道映射和重采样
        // ================================================================
        if is_minimal_convert {
            let n_target = (output_frames as isize + delta as isize).max(0) as usize;
            let needed = n_target * ich;

            // 确保临时缓冲区足够大
            if tmp_buf.len() < needed {
                tmp_buf.resize(needed, 0.0);
            }

            if consumer.occupied_len() >= needed {
                // 从 SPSC 读取 n_target 帧到临时缓冲区
                let pop_count = consumer.pop_slice(&mut tmp_buf[..needed]);
                debug_assert_eq!(pop_count, needed);

                // 应用限幅器
                if let Ok(mut lim) = limiter.lock() {
                    lim.process(&mut tmp_buf[..needed], output_channels);
                }

                // 写入输出缓冲区：取 output_frames 帧，丢弃多余或填充静音
                let copy_samples = output_frames * och;
                let src_samples = n_target * ich;
                let copy = copy_samples.min(src_samples);
                output[..copy].copy_from_slice(&tmp_buf[..copy]);
                for s in &mut output[copy..] {
                    *s = 0.0;
                }
            } else {
                consumer.clear();
                fader.start_fade_out();
                underrun_counter.fetch_add(1, Ordering::Relaxed);
                for sample in output.iter_mut() {
                    *sample = 0.0;
                }
            }
            fader.process(output, output_channels);
            return;
        }

        // ================================================================
        // 路径 3：完整转换路径
        // 统一流程：SPSC 读取 → [声道映射] → 限幅 → 重采样 → 输出
        // ================================================================

        // 获取重采样器需要的输入帧数
        let resampler_input_frames = if let Ok(ref resampler) = resampler.lock() {
            resampler.input_frames_next()
        } else {
            output_frames // 回退：假设需要与输出相同的帧数
        };

        // 计算需要从 SPSC 读取的输入帧数（含 delta 微调）
        let n_in = resampler_input_frames.max(1) + delta.max(0) as usize;
        let needed_samples = n_in * ich;

        // 确保临时缓冲区足够大（考虑重采样器额外需求 + 声道映射后的可能扩增）
        let max_possible = needed_samples.max(n_in * och);
        if tmp_buf.len() < max_possible {
            tmp_buf.resize(max_possible, 0.0);
        }

        if consumer.occupied_len() >= needed_samples {
            // ---- 步骤 1：从 SPSC 读取数据到临时缓冲区 ----
            let pop_count = consumer.pop_slice(&mut tmp_buf[..needed_samples]);
            debug_assert_eq!(pop_count, needed_samples);

            // ---- 步骤 2：声道映射（如需要）----
            // need_mapping 表示输入/输出声道数不同，需要经过 ChannelMapper 转换
            let need_mapping = !channel_mapper.is_passthrough();

            // 跟踪当前待处理的数据范围（样本数）和声道数
            // processed_samples / processed_channels 在步骤 2 后确定
            let (processed_samples, processed_channels) = if need_mapping {
                let mapped_len = n_in * och;
                // 创建映射缓冲区并执行声道映射
                let mut mapped_buf = vec![0.0f32; mapped_len];
                channel_mapper.map(&tmp_buf[..needed_samples], &mut mapped_buf);
                // 将映射结果写回临时缓冲区
                tmp_buf[..mapped_len].copy_from_slice(&mapped_buf);
                (mapped_len, output_channels)
            } else {
                (needed_samples, input_channels)
            };

            // ---- 步骤 3：砖墙限幅 ----
            if let Ok(mut lim) = limiter.lock() {
                lim.process(&mut tmp_buf[..processed_samples], processed_channels);
            }

            // ---- 步骤 4：重采样 ----
            if let Ok(mut resampler) = resampler.lock() {
                let out_samples = output.len();
                let (_consumed, produced) = resampler
                    .process(&tmp_buf[..processed_samples], output)
                    .unwrap_or((0, 0));
                // 若重采样产出不足，剩余部分填静音
                let produced_samples = produced * och;
                for s in &mut output[produced_samples.min(out_samples)..] {
                    *s = 0.0;
                }
            } else {
                // 无法获取重采样器锁 → 输出静音
                for s in output.iter_mut() {
                    *s = 0.0;
                }
            }
        } else {
            // ---- 欠载处理 ----
            consumer.clear();
            // 重置重采样器内部状态，避免残留数据污染后续处理
            if let Ok(mut resampler) = resampler.lock() {
                resampler.reset();
            }
            fader.start_fade_out();
            underrun_counter.fetch_add(1, Ordering::Relaxed);
            for sample in output.iter_mut() {
                *sample = 0.0;
            }
        }

        // 应用淡入淡出增益（无论数据充足还是欠载静音都需处理）
        fader.process(output, output_channels);
    }
}

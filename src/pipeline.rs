// 音频路由器 — Phase 2 扇出管道核心模块
//
// 本模块实现 SPSC 槽位管理、余弦淡入淡出、扇出输入/输出回调。
// 架构：单个输入回调 → [环形缓冲区 × N] → 多个输出回调。
// 使用 UnsafeCell 实现零锁生产者访问（SPSC 天然保证独占写入）。

use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

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

/// 创建输出回调闭包
///
/// 从环形缓冲区消费音频数据，经过声道映射和淡入淡出处理后写入输出设备。
///
/// # 参数
/// * `consumer` — 环形缓冲区的消费者端（独占所有权）
/// * `fader` — 淡入淡出处理器共享引用
/// * `channel_mapper` — 声道映射器共享引用
/// * `input_channels` — 输入音频流的声道数
/// * `output_channels` — 输出音频流的声道数
/// * `input_sample_rate` — 输入采样率
/// * `output_sample_rate` — 输出采样率
/// * `underrun_counter` — 欠载计数器（用于监控/诊断）
pub fn create_output_callback(
    mut consumer: RbConsumer,
    fader: Arc<Fader>,
    channel_mapper: Arc<ChannelMapper>,
    input_channels: u16,
    output_channels: u16,
    input_sample_rate: u32,
    output_sample_rate: u32,
    underrun_counter: Arc<AtomicU64>,
) -> impl FnMut(&mut [f32]) + Send + 'static {
    let ich = input_channels as usize;
    let och = output_channels as usize;
    let is_passthrough = input_sample_rate == output_sample_rate
        && input_channels == output_channels;

    // 预分配临时缓冲区，避免音频回调中堆分配
    // 按最大预期帧数（4096）分配，覆盖绝大多数音频缓冲区大小
    let max_tmp_samples = 4096 * ich;
    let mut tmp_buf: Vec<f32> = vec![0.0f32; max_tmp_samples];

    move |output: &mut [f32]| {
        let output_frames = output.len() / och;
        if output_frames == 0 {
            return;
        }

        // 计算需要的输入采样数
        // 直通模式：帧数相同
        // 转换模式：按声道比计算（简化处理，不支持采样率转换）
        let n_in_target = if is_passthrough {
            output_frames
        } else {
            // 声道数不同时按比例计算需要的输入帧数
            (output_frames * ich) / och
        };

        let needed_samples = n_in_target * ich;

        // 确保临时缓冲区足够大
        if tmp_buf.len() < needed_samples {
            // 仅在缓冲区不足时才扩容（正常运行时不会进入此分支）
            tmp_buf.resize(needed_samples, 0.0);
        }

        // 检查消费者端数据是否充足
        let occupied = consumer.occupied_len();
        if occupied >= needed_samples {
            // ---- 数据充足 ----
            if is_passthrough {
                // 直通模式：直接拷贝到输出缓冲区
                let pop_count = consumer.pop_slice(output);
                debug_assert_eq!(pop_count, needed_samples);
            } else {
                // 转换模式：先读到临时缓冲区，再经过声道映射写入输出
                let pop_count = consumer.pop_slice(&mut tmp_buf[..needed_samples]);
                debug_assert_eq!(pop_count, needed_samples);
                channel_mapper.map(&tmp_buf[..needed_samples], output);
            }
        } else {
            // ---- 数据不足（欠载）----
            // 清空消费者端残余数据
            consumer.clear();

            // 启动淡出，使欠载过渡平滑
            fader.start_fade_out();

            // 记录欠载事件
            underrun_counter.fetch_add(1, Ordering::Relaxed);

            // 输出填充静音
            for sample in output.iter_mut() {
                *sample = 0.0;
            }
        }

        // 应用淡入淡出增益（无论数据充足还是欠载静音都需处理）
        fader.process(output, output_channels);
    }
}

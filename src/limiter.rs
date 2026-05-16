// 砖墙限幅器（Brickwall Limiter）模块
//
// 实现零攻击（或极短攻击）的峰值限幅，防止音频信号超过指定阈值。
// 适用于音频回调中的实时处理，所有字段均为 Send，可直接放入 Mutex 中跨线程使用。

// ============================================================================
// (A) 结构体定义
// ============================================================================

/// 砖墙限幅器：确保输出信号峰值不超过阈值
///
/// 使用峰值包络跟随 + 压缩增益的方式，只做衰减不做放大。
/// 攻击阶段：当瞬时峰值超过包络时，包络立即上升（attack_coeff 控制速度）。
/// 释放阶段：当瞬时峰值低于包络时，包络按释放系数指数衰减。
///
/// 所有字段均实现 Send trait，可安全地在音频回调中被 Mutex 包装使用。
pub struct BrickwallLimiter {
    /// 阈值（线性值，0dBFS = 1.0），超过此值的信号将被衰减
    threshold: f32,
    /// 释放系数：每样本包络衰减因子（< 1.0）
    release_coeff: f32,
    /// 每个声道的峰值包络（存储当前估计的峰值幅度）
    envelope: Vec<f32>,
    /// 攻击系数：1.0 = 零攻击（瞬时响应），< 1.0 = 平滑攻击
    attack_coeff: f32,
    /// 是否启用限幅处理
    enabled: bool,
}

// 安全标记：BrickwallLimiter 所有字段均为 Send，编译器自动实现 Send trait
// 显式声明以确保未来修改不会意外破坏此约束
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<BrickwallLimiter>();
};

// ============================================================================
// (B) 构造与配置
// ============================================================================

impl BrickwallLimiter {
    /// 创建新的砖墙限幅器
    ///
    /// # 参数
    /// * `threshold`     — 限幅阈值（线性值，0dBFS = 1.0），通常设为 0.95~0.98
    /// * `attack_ms`     — 攻击时间（毫秒），设为 ≤0.0 即零攻击（瞬间响应）
    /// * `release_ms`    — 释放时间（毫秒），控制增益恢复速度
    /// * `channels`      — 声道数（每个声道独立跟踪包络）
    /// * `sample_rate`   — 采样率（Hz），用于计算攻击/释放系数
    /// * `enabled`       — 是否默认启用
    ///
    /// # 公式
    /// * 攻击系数（attack_coeff）：
    ///   - 若 attack_ms ≤ 0.0 → 1.0（零攻击，瞬间追上峰值）
    ///   - 否则 → `1.0 - exp(-1.0 / (sample_rate * attack_ms / 1000.0))`
    /// * 释放系数（release_coeff）：
    ///   - `exp(-1.0 / (sample_rate * release_ms / 1000.0))`
    pub fn new(
        threshold: f32,
        attack_ms: f32,
        release_ms: f32,
        channels: usize,
        sample_rate: f64,
        enabled: bool,
    ) -> Self {
        // 计算攻击系数：零攻击时为 1.0，否则按指数平滑公式计算
        let attack_coeff = if attack_ms <= 0.0 {
            1.0
        } else {
            1.0 - (-1.0 / (sample_rate * attack_ms as f64 / 1000.0)).exp() as f32
        };

        // 计算释放系数：包络按此因子每样本衰减
        let release_coeff = (-1.0 / (sample_rate * release_ms as f64 / 1000.0)).exp() as f32;

        // 每个声道独立维护包络状态，初始化为零（静音）
        let envelope = vec![0.0f32; channels];

        Self {
            threshold,
            release_coeff,
            envelope,
            attack_coeff,
            enabled,
        }
    }
}

// ============================================================================
// (C) 核心处理
// ============================================================================

impl BrickwallLimiter {
    /// 处理音频缓冲区，对超过阈值的样本进行限幅
    ///
    /// 逐帧处理交错格式的缓冲区（每帧 channels 个样本）：
    /// 1. 计算每声道样本的瞬时峰值
    /// 2. 更新峰值包络（攻击或释放）
    /// 3. 根据包络计算增益，只做衰减（gain ≤ 1.0）
    /// 4. 将增益应用到样本
    ///
    /// # 参数
    /// * `buffer`   — 交错格式的音频样本缓冲区（输入/输出，就地修改）
    /// * `channels` — 声道数，决定帧边界
    ///
    /// # 注意
    /// * 若 `enabled == false`，函数直接返回不处理
    /// * 为防止除零，包络最小钳位到 1e-10
    /// * 增益始终 ≤ 1.0（只衰减，不放大）
    pub fn process(&mut self, buffer: &mut [f32], channels: u16) {
        // 未启用时跳过所有处理
        if !self.enabled {
            return;
        }

        let ch = channels as usize;
        let total_frames = buffer.len() / ch;

        // 逐帧处理
        for frame in 0..total_frames {
            let start = frame * ch;

            // 逐声道处理当前帧
            for ci in 0..ch {
                let idx = start + ci;
                let sample = &mut buffer[idx];
                let peak = sample.abs();

                // 更新峰值包络：攻击阶段 vs 释放阶段
                if peak > self.envelope[ci] {
                    // 攻击阶段：包络向瞬时峰值靠拢
                    self.envelope[ci] =
                        self.attack_coeff * peak + (1.0 - self.attack_coeff) * self.envelope[ci];
                } else {
                    // 释放阶段：包络按释放系数指数衰减
                    self.envelope[ci] *= self.release_coeff;
                }

                // 计算增益：阈值 / 包络，钳位到最大 1.0（只衰减不放大）
                // 包络钳位到 1e-10 防止除零
                let gain = 1.0_f32.min(self.threshold / self.envelope[ci].max(1e-10));

                // 应用增益
                *sample *= gain;
            }
        }
    }
}

// ============================================================================
// (D) 状态控制
// ============================================================================

impl BrickwallLimiter {
    /// 切换启用/禁用状态
    ///
    /// 禁用时 `process()` 将直接返回不处理，不影响音频直通。
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// 重置所有声道的峰值包络为零
    ///
    /// 在切换输入源或长时间静音后调用，避免释放残留包络造成不必要的衰减。
    pub fn reset(&mut self) {
        for v in self.envelope.iter_mut() {
            *v = 0.0;
        }
    }

    /// 查询当前启用状态
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

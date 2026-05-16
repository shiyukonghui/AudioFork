// 时钟漂移补偿模块 — PI 控制器 + EMA 水位监控
//
// PI 调参建议：
// ---------------------------------------------------------------------------
// 1. kp（比例增益，默认 0.02）：决定控制器对水位偏差的瞬时响应强度。
//    - 过大会导致 delta 频繁在 -1 / 0 / +1 之间振荡，引起音频卡顿。
//    - 过小则响应迟钝，水位偏离 50% 后需要较长时间恢复。
//    - 建议从 0.01 开始，每次增加 0.005，观察 δ 收敛情况。
//
// 2. ki（积分增益，默认 0.001）：消除稳态误差，使水位长期稳定在 50%。
//    - 过大会导致积分饱和（integral windup），引发大幅超调和振荡。
//    - 过小则稳态误差消除缓慢。
//    - 本实现内置 0.99 弱泄漏因子防积分饱和，允许 ki 适当加大。
//
// 3. ema_alpha（EMA 平滑系数，默认 ≈0.0328）：dt=0.1s, τ=3.0s。
//    - alpha 越大 = τ 越短，EMA 跟踪越灵敏但噪声过滤能力越弱。
//    - alpha 越小 = τ 越长，EMA 更平滑但响应滞后加大。
//    - 不建议直接修改 alpha，应通过调整调用周期 (dt) 与时间常数 (τ) 来控制。
//
// 4. 量化阈值（默认 0.5）：raw > 0.5 → δ=+1, raw < -0.5 → δ=-1。
//    - 阈值越大，δ 变化越不频繁，适合缓冲区水位波动大的场景。
//    - 阈值越小，δ 变化更敏感，适合需要紧密跟踪的场景。
//
// 5. 典型调参流程：
//    a) 先关闭积分项（ki=0），调节 kp 使水位大致在 50% 附近波动。
//    b) 逐步增大 ki，观察水位是否稳定收敛到 50%，关注 δ 的振荡幅度。
//    c) 若出现持续振荡，适当减小 kp 或增大 ema tau。
//    d) 若水位始终偏离目标值，检查是否存在系统性时钟偏差——
//       输入设备采样率与输出设备采样率差异过大时，PI 控制器的纠正能力有限，
//       建议在 Phase 3 引入重采样（rubato）做更根本的时钟同步。

use std::sync::atomic::{AtomicI32, Ordering};

/// 时钟漂移补偿器
///
/// 使用 PI 控制器根据环形缓冲区水位（water level）自动微调音频帧数，
/// 以补偿输入/输出设备之间的时钟频率差异。
///
/// # 架构说明
///
/// - **控制线程**（每 100ms 调用 [`update()`]）：读取水位，计算 EMA，运行 PI 控制器，
///   将量化后的 delta 写入 [`AtomicI32`]。
/// - **音频回调线程**（实时）：通过 [`delta_val()`] 原子读取 delta，
///   在读写指针推进时应用微调。
///
/// # 类型安全性
///
/// 结构体实现 `Send`，可以安全地跨线程移动（参见编译期断言）。
///
/// [`update()`]: DriftCompensator::update
/// [`delta_val()`]: DriftCompensator::delta_val
pub struct DriftCompensator {
    /// 漂移补偿是否启用（运行时动态切换，不影响 PI 内部状态）
    enabled: bool,
    /// EMA 平滑系数 α，由 dt 和 τ 计算得出
    ema_alpha: f64,
    /// 当前 EMA 平滑后的水位值（0.0 ~ 1.0）
    ema_level: f64,
    /// 目标水位，固定为 0.5（对应环形缓冲区半满状态）
    target_level: f64,
    /// PI 控制器比例增益 Kp
    kp: f64,
    /// PI 控制器积分增益 Ki
    ki: f64,
    /// PI 控制器积分累加器
    integral: f64,
    /// 帧数微调量（-1 / 0 / +1），供音频回调原子读取
    ///
    /// - `+1`：多消费一帧，降低缓冲区水位
    /// - `-1`：少消费一帧，提升缓冲区水位
    /// - ` 0`：保持当前消费帧数不变
    pub delta: AtomicI32,
}

impl DriftCompensator {
    /// 创建新的时钟漂移补偿器实例
    ///
    /// # 参数
    ///
    /// * `enabled` - 初始启停状态
    /// * `sample_rate` - 音频采样率（Hz），保留以备将来按采样率自适应调参
    /// * `buffer_frames` - 缓冲区帧数，保留以备将来按缓冲区大小自适应调参
    ///
    /// # 默认参数
    ///
    /// * target_level = `0.5`（环形缓冲区半满）
    /// * ema_alpha ≈ `0.0328`（dt=0.1s, τ=3s 对应的 EMA 系数）
    /// * ema_level 初始值 = `0.5`
    /// * kp = `0.02`
    /// * ki = `0.001`
    /// * integral = `0.0`
    /// * delta = `0`
    pub fn new(enabled: bool, _sample_rate: f64, _buffer_frames: usize) -> Self {
        // EMA 平滑系数：alpha = 1 - exp(-dt / tau)
        // dt = 0.1 秒（控制线程更新周期），tau = 3.0 秒（EMA 时间常数）
        let ema_alpha = 1.0 - (-0.1_f64 / 3.0_f64).exp();

        Self {
            enabled,
            ema_alpha,
            // EMA 水位初始化为目标值，避免冷启动时产生虚假偏差
            ema_level: 0.5,
            target_level: 0.5,
            kp: 0.02,
            ki: 0.001,
            integral: 0.0,
            // delta 初始化为 0，不进行任何帧数微调
            delta: AtomicI32::new(0),
        }
    }

    /// 控制线程每 100ms 调用一次，根据当前水位更新 PI 控制器输出
    ///
    /// # 参数
    ///
    /// * `water_level` - 当前环形缓冲区水位（0.0 ~ 1.0，超出范围会自动钳位）
    ///
    /// # 工作流程
    ///
    /// 1. 钳位水位到 `[0.0, 1.0]`，防止异常值破坏控制器状态
    /// 2. 更新 EMA 平滑值：`EMA = α × 当前值 + (1-α) × 历史 EMA`
    /// 3. 计算误差（放大 100 倍）：`error = (target - EMA) × 100`
    /// 4. 积分累加（含 0.99 弱泄漏防饱和）
    /// 5. 计算 PI 输出：`raw = Kp × error + integral`
    /// 6. 将连续输出量化：`raw > 0.5 → +1`, `raw < -0.5 → -1`, 其余 → `0`
    /// 7. 若漂移补偿被禁用，delta 强制为 0
    /// 8. 原子写入 delta（`Release` 内存序）
    pub fn update(&mut self, water_level: f64) {
        // 步骤 1：钳位水位到 [0.0, 1.0]，防止异常值破坏控制器状态
        let water_level = water_level.clamp(0.0, 1.0);

        // 步骤 2：更新 EMA 平滑值
        // EMA(t) = α × 当前水位 + (1 - α) × EMA(t-1)
        self.ema_level =
            self.ema_alpha * water_level + (1.0 - self.ema_alpha) * self.ema_level;

        // 步骤 3：计算误差（放大 100 倍，使 PI 参数更易调节）
        // error > 0 → 水位高于目标（缓冲区偏满，需要消费更多帧）
        // error < 0 → 水位低于目标（缓冲区偏空，需要消费更少帧）
        let error = (self.target_level - self.ema_level) * 100.0;

        // 步骤 4：积分项累加（0.99 弱泄漏因子防积分饱和）
        // 弱泄漏确保积分项随时间自然衰减，避免长时间单向偏差导致的超调
        self.integral = self.integral * 0.99 + self.ki * error;

        // 步骤 5：PI 控制器输出（比例项 + 积分项）
        let raw = self.kp * error + self.integral;

        // 步骤 6：连续输出量化为离散帧数微调量
        let delta = if raw > 0.5 {
            1 // 消费更多帧 → 降低水位
        } else if raw < -0.5 {
            -1 // 消费更少帧 → 提升水位
        } else {
            0 // 保持当前消费速率不变
        };

        // 步骤 7：若漂移补偿被禁用，delta 强制为 0
        let final_delta = if self.enabled { delta } else { 0 };

        // 步骤 8：原子写入 delta（Release 语义确保对音频回调线程可见）
        self.delta.store(final_delta, Ordering::Release);
    }

    /// 获取当前帧数微调量（供音频回调线程原子读取）
    ///
    /// 使用 `Acquire` 内存序与 [`update()`] 中的 `Release` 配对，
    /// 形成 happens-before 关系，确保读取到的是最新写入的值。
    ///
    /// [`update()`]: DriftCompensator::update
    pub fn delta_val(&self) -> i32 {
        self.delta.load(Ordering::Acquire)
    }

    /// 返回 EMA 水位百分比（0.0 ~ 100.0）
    ///
    /// 用于外部监控和日志输出，100.0 表示缓冲区已满（存在溢出风险），
    /// 0.0 表示缓冲区已空（存在欠载风险）。
    pub fn water_level_pct(&self) -> f64 {
        self.ema_level * 100.0
    }

    /// 返回当前 EMA 平滑后的水位值（0.0 ~ 1.0）
    ///
    /// 原始归一化值，0.5 表示缓冲区恰好半满（理想状态）。
    pub fn ema_level_val(&self) -> f64 {
        self.ema_level
    }

    /// 运行时切换漂移补偿开关
    ///
    /// # 行为说明
    ///
    /// - 关闭时：立即将 delta 清零，停止帧数微调，
    ///   但 PI 内部状态（`ema_level`、`integral`）保持不变。
    /// - 重新启用时：从上次的状态无缝恢复，无需重新收敛。
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            // 关闭时立即清零 delta，确保音频回调不再进行帧数微调
            self.delta.store(0, Ordering::Release);
        }
    }

    /// 查询漂移补偿是否处于启用状态
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

// ============================================================================
// 编译期类型断言：确保 DriftCompensator 实现 Send（跨线程安全性）
// ============================================================================
// DriftCompensator 包含 AtomicI32（天然 Send），其余字段均为 f64 / bool，
// 不存在引用计数或裸指针等非 Send 类型，编译器会自动推导 Send trait。
// 以下函数在编译期进行类型级断言——若 DriftCompensator 未来被错误修改为
// 非 Send，编译将直接失败，及早暴露问题。
#[allow(dead_code)]
fn _assert_drift_compensator_send() {
    fn _is_send<T: Send>() {}
    _is_send::<DriftCompensator>();
}

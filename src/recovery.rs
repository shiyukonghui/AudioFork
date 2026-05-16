// 输入设备丢失恢复状态机
// 实现指数退避重连、降级到默认设备、以及 exit_on_loss 模式
// 供控制线程定时调用 tick() 驱动状态转换，供音频回调线程读取 input_lost 标志

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::error::RecoveryState;

// ============================================================================
// RecoveryAction：控制线程在每次 tick 后根据此枚举执行相应操作
// ============================================================================

/// 恢复动作枚举，tick() 返回值，指示调用者应执行的操作
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryAction {
    /// 无需任何操作
    None,
    /// 尝试重新连接到原始输入设备
    TryReconnectOriginal,
    /// 降级：尝试连接到系统默认输入设备
    TryFallbackToDefault,
    /// 应立即退出进程（exit_on_loss 模式下输入丢失）
    ShouldExit,
}

// ============================================================================
// InputRecoveryManager：输入设备丢失恢复管理器
// ============================================================================

/// 输入设备恢复状态管理器
///
/// 负责跟踪输入设备丢失后的恢复流程：
/// - 阶段1（前30秒）：指数退避重连原设备，间隔从 100ms 翻倍至最大 5s
/// - 阶段2（30秒后）：
///   - 若启用 fallback_to_default：切换至默认设备降级模式，每 5 秒重试一次
///   - 若未启用 fallback_to_default：继续以 5 秒固定间隔重连原设备
/// - exit_on_loss 模式：输入丢失时立即通知退出
pub struct InputRecoveryManager {
    /// 当前恢复状态
    state: RecoveryState,
    /// 是否在输入丢失时直接退出进程
    exit_on_loss: bool,
    /// 是否在超时后降级到系统默认设备
    fallback_to_default: bool,
    /// 输入丢失标志（供音频回调线程读取，用于静音输出等降级操作）
    pub input_lost: Arc<AtomicBool>,
    /// 输入丢失发生的时刻（用于判断是否超过 30 秒阈值）
    lost_time: Option<Instant>,
    /// 累计等待时间（每次 tick 累加 100ms，达到阈值后触发动作并归零）
    accumulated_wait: Duration,
    /// 原始输入设备名称（用于日志/重连定位）
    original_device_name: Option<String>,
}

impl InputRecoveryManager {
    /// 创建新的恢复管理器
    ///
    /// # 参数
    /// - `exit_on_loss`: 输入丢失时是否直接退出进程
    /// - `fallback_to_default`: 超时后是否降级到系统默认设备
    /// - `original_device_name`: 原始输入设备名称（用于日志）
    pub fn new(
        exit_on_loss: bool,
        fallback_to_default: bool,
        original_device_name: Option<String>,
    ) -> Self {
        Self {
            state: RecoveryState::Normal,
            exit_on_loss,
            fallback_to_default,
            // 初始状态输入未丢失
            input_lost: Arc::new(AtomicBool::new(false)),
            lost_time: None,
            accumulated_wait: Duration::ZERO,
            original_device_name,
        }
    }

    // ========================================================================
    // 事件驱动方法
    // ========================================================================

    /// 输入设备丢失时的回调
    ///
    /// - exit_on_loss 模式：设置 input_lost 标志并返回 `ShouldExit`
    /// - 正常模式：设置 input_lost 标志，记录丢失时刻，进入指数退避重连阶段
    pub fn on_input_lost(&mut self) -> RecoveryAction {
        if self.exit_on_loss {
            // 退出模式：设置标志，通知调用者退出
            self.input_lost.store(true, Ordering::Release);
            return RecoveryAction::ShouldExit;
        }

        // 正常恢复模式：标记丢失，记录时间，进入退避重连
        self.input_lost.store(true, Ordering::Release);
        self.lost_time = Some(Instant::now());
        self.accumulated_wait = Duration::ZERO;
        self.state = RecoveryState::ReconnectBackoff {
            attempt: 0,
            // 首次重连间隔 100ms
            next_interval: Duration::from_millis(100),
        };
        RecoveryAction::None
    }

    /// 输入设备恢复时的回调
    ///
    /// 重置所有状态回到 Normal，清除丢失标志
    pub fn on_input_recovered(&mut self) {
        self.input_lost.store(false, Ordering::Release);
        self.state = RecoveryState::Normal;
        self.lost_time = None;
        self.accumulated_wait = Duration::ZERO;
    }

    // ========================================================================
    // 定时驱动方法（控制线程每 100ms 调用一次）
    // ========================================================================

    /// 定时 tick，驱动状态机前进
    ///
    /// 控制线程应每 100ms 调用一次此方法，根据返回值执行对应的恢复操作。
    ///
    /// # 状态转换逻辑
    ///
    /// **Normal**: 无需操作，返回 `None`
    ///
    /// **ReconnectBackoff**（前 30 秒指数退避）:
    /// - 每次 tick 累加 100ms 等待时间
    /// - 未达到 `next_interval` → 返回 `None`
    /// - 达到 `next_interval`:
    ///   - 已超过 30 秒 且 `fallback_to_default` → 切换至 `FallbackToDefault`
    ///   - 已超过 30 秒 且 不降级 → 固定 5 秒间隔继续重连原设备
    ///   - 未超 30 秒 → 翻倍间隔（上限 5 秒），重试原设备
    ///
    /// **FallbackToDefault**（降级重连）:
    /// - 每 5 秒返回一次 `TryFallbackToDefault`
    pub fn tick(&mut self) -> RecoveryAction {
        match &self.state {
            RecoveryState::Normal => RecoveryAction::None,

            RecoveryState::ReconnectBackoff {
                attempt,
                next_interval,
            } => {
                // 每次 tick 累加 100ms 等待时间
                self.accumulated_wait += Duration::from_millis(100);

                // 尚未到达重连间隔，继续等待
                if self.accumulated_wait < *next_interval {
                    return RecoveryAction::None;
                }

                // 判断是否已超过 30 秒总超时
                let over_30s = self
                    .lost_time
                    .map(|t| t.elapsed() >= Duration::from_secs(30))
                    .unwrap_or(false);

                if over_30s {
                    if self.fallback_to_default {
                        // 超过 30 秒，降级到默认设备
                        self.state = RecoveryState::FallbackToDefault { attempt: 0 };
                        self.accumulated_wait = Duration::ZERO;
                        RecoveryAction::TryFallbackToDefault
                    } else {
                        // 超过 30 秒但不降级，继续以固定 5 秒间隔重连原设备
                        self.state = RecoveryState::ReconnectBackoff {
                            attempt: *attempt,
                            next_interval: Duration::from_secs(5),
                        };
                        self.accumulated_wait = Duration::ZERO;
                        RecoveryAction::TryReconnectOriginal
                    }
                } else {
                    // 未超 30 秒：指数退避，间隔翻倍（上限 5 秒）
                    let new_attempt = attempt + 1;
                    let new_interval = (*next_interval * 2).min(Duration::from_secs(5));
                    self.state = RecoveryState::ReconnectBackoff {
                        attempt: new_attempt,
                        next_interval: new_interval,
                    };
                    self.accumulated_wait = Duration::ZERO;
                    RecoveryAction::TryReconnectOriginal
                }
            }

            RecoveryState::FallbackToDefault { attempt } => {
                // 每次 tick 累加 100ms 等待时间
                self.accumulated_wait += Duration::from_millis(100);

                // 固定 5 秒间隔重试默认设备
                if self.accumulated_wait < Duration::from_secs(5) {
                    return RecoveryAction::None;
                }

                // 到达间隔，重试默认设备
                self.state = RecoveryState::FallbackToDefault {
                    attempt: attempt + 1,
                };
                self.accumulated_wait = Duration::ZERO;
                RecoveryAction::TryFallbackToDefault
            }
        }
    }

    // ========================================================================
    // 状态查询方法
    // ========================================================================

    /// 是否应该退出进程
    ///
    /// 当 `exit_on_loss` 为 true 且输入已丢失（input_lost 标志为 true）时返回 true
    pub fn should_exit(&self) -> bool {
        self.exit_on_loss && self.input_lost.load(Ordering::Acquire)
    }

    /// 当前是否处于正常运行状态
    pub fn is_normal(&self) -> bool {
        matches!(self.state, RecoveryState::Normal)
    }

    /// 获取 input_lost 标志的 Arc 克隆
    ///
    /// 供音频回调线程读取，用于判断是否应静音输出
    pub fn input_lost_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.input_lost)
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 exit_on_loss 模式：on_input_lost 立即返回 ShouldExit
    #[test]
    fn test_exit_on_loss() {
        let mut mgr = InputRecoveryManager::new(true, false, None);
        assert!(mgr.is_normal());

        let action = mgr.on_input_lost();
        assert_eq!(action, RecoveryAction::ShouldExit);
        assert!(mgr.should_exit());
        // exit_on_loss 模式下不改变状态（仅设置标志）
        assert!(mgr.input_lost.load(Ordering::Acquire));
    }

    /// 测试正常模式下的指数退避重连
    #[test]
    fn test_reconnect_backoff() {
        let mut mgr = InputRecoveryManager::new(false, false, None);
        assert!(mgr.is_normal());

        // 模拟输入丢失
        let action = mgr.on_input_lost();
        assert_eq!(action, RecoveryAction::None);
        assert!(!mgr.is_normal());
        assert!(mgr.input_lost.load(Ordering::Acquire));

        // 第一次 tick（100ms 后）：立即触发首次重连（attempt 0, interval 100ms）
        let action = mgr.tick();
        assert_eq!(action, RecoveryAction::TryReconnectOriginal);

        // 第二次 tick（再 100ms 后）：间隔应为 200ms，尚未达到
        let action = mgr.tick();
        assert_eq!(action, RecoveryAction::None);

        // 第三次 tick（再 100ms 后 = 累计 200ms）：达到 200ms 间隔
        let action = mgr.tick();
        assert_eq!(action, RecoveryAction::TryReconnectOriginal);
    }

    /// 测试输入恢复
    #[test]
    fn test_on_input_recovered() {
        let mut mgr = InputRecoveryManager::new(false, false, None);
        mgr.on_input_lost();
        assert!(!mgr.is_normal());

        mgr.on_input_recovered();
        assert!(mgr.is_normal());
        assert!(!mgr.input_lost.load(Ordering::Acquire));
    }

    /// 测试 Normal 状态下 tick 始终返回 None
    #[test]
    fn test_normal_tick_returns_none() {
        let mut mgr = InputRecoveryManager::new(false, false, None);
        for _ in 0..10 {
            assert_eq!(mgr.tick(), RecoveryAction::None);
        }
    }

    /// 测试 should_exit 在非 exit_on_loss 模式下始终返回 false
    #[test]
    fn test_should_exit_when_not_exit_on_loss() {
        let mut mgr = InputRecoveryManager::new(false, false, None);
        assert!(!mgr.should_exit());

        mgr.on_input_lost();
        assert!(!mgr.should_exit());
    }

    /// 测试 input_lost_flag 返回可共享的 Arc
    #[test]
    fn test_input_lost_flag_clone() {
        let mgr = InputRecoveryManager::new(false, false, None);
        let flag = mgr.input_lost_flag();
        assert!(!flag.load(Ordering::Acquire));
    }

    /// 测试 FallbackToDefault 阶段的 tick 行为
    #[test]
    fn test_fallback_tick_timing() {
        let mut mgr = InputRecoveryManager::new(false, true, None);
        mgr.on_input_lost();

        // 手动推进到 FallbackToDefault 状态（模拟超过 30 秒）
        // 设置 lost_time 为 30 秒前
        mgr.lost_time = Some(Instant::now() - Duration::from_secs(31));

        // 让 accumulated_wait 达到 next_interval 以触发超时检查
        // 当前是 ReconnectBackoff { attempt: 0, next_interval: 100ms }
        // tick 一次让 accumulated_wait = 100ms，刚好触发
        let action = mgr.tick();
        // 因为 lost_time 已经超过 30 秒且 fallback_to_default 为 true
        assert_eq!(action, RecoveryAction::TryFallbackToDefault);

        // 现在应该在 FallbackToDefault 状态
        // 第一次 tick：accumulated_wait = 100ms < 5s → None
        let action = mgr.tick();
        assert_eq!(action, RecoveryAction::None);

        // 后续 tick 也应为 None，直到累计 5 秒
        for _ in 0..48 {
            mgr.tick();
        }
        // 第 50 次 tick（累计 5 秒）
        let action = mgr.tick();
        assert_eq!(action, RecoveryAction::TryFallbackToDefault);
    }
}

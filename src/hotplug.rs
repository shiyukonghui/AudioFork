// 音频路由器 — 设备热插拔监听模块
// 周期性轮询系统音频设备列表，检测设备的插入与移除，并通过消息通道发送事件

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use crossbeam_channel::Sender;

use crate::device::DeviceInfo;

/// 热插拔事件枚举
///
/// 当系统音频设备发生变化时产生对应的事件：
/// - DeviceAdded: 新设备被接入
/// - DeviceRemoved: 已有设备被移除（携带设备名称）
#[derive(Debug, Clone)]
pub enum HotplugEvent {
    /// 新设备接入，携带设备详细信息
    DeviceAdded(DeviceInfo),
    /// 设备被移除，携带设备名称作为标识
    DeviceRemoved(String),
}

/// 启动热插拔监听线程
///
/// 在独立线程中周期性轮询输出设备列表，与上一次的快照对比，
/// 检测新增和移除的设备，并通过 `tx` 通道发送相应的事件。
///
/// # 参数
///
/// * `tx` - crossbeam 发送端，用于将热插拔事件发送给消费者
/// * `stop_flag` - 原子布尔标志，设为 true 时监听线程退出循环
/// * `poll_interval` - 轮询间隔，通常使用 `Duration::from_secs(2)`
///
/// # 返回
///
/// 返回线程句柄 `JoinHandle<()>`，调用方可借此等待线程结束
pub fn start_hotplug_monitor(
    tx: Sender<HotplugEvent>,
    stop_flag: Arc<AtomicBool>,
    poll_interval: Duration,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        tracing::info!(
            "热插拔监听已启动，轮询间隔: {:?}",
            poll_interval
        );

        // 维护上一轮快照中的设备名称集合
        let mut known_devices: HashSet<String> = HashSet::new();

        // 标记是否已完成首次快照（首次不发送事件，仅初始化基准）
        let mut first_snapshot = true;

        loop {
            // 检查停止标志，若已设置则退出循环
            if stop_flag.load(Ordering::Relaxed) {
                tracing::info!("热插拔监听收到停止信号，线程退出");
                break;
            }

            // 枚举当前输出设备列表
            match crate::device::enumerate_output_devices() {
                Ok(current_devices) => {
                    // 构建当前设备名称集合
                    let current_names: HashSet<String> = current_devices
                        .iter()
                        .map(|info| info.name.clone())
                        .collect();

                    if first_snapshot {
                        // 首次快照：仅初始化已知设备集合，不发送事件
                        known_devices = current_names;
                        first_snapshot = false;
                        tracing::debug!(
                            "热插拔监听：首次快照完成，已知设备数: {}",
                            known_devices.len()
                        );
                    } else {
                        // 检测新增设备（在当前列表但不在已知列表中）
                        for info in &current_devices {
                            if !known_devices.contains(&info.name) {
                                tracing::info!(
                                    "检测到新设备接入: {}",
                                    info.name
                                );
                                if tx.send(HotplugEvent::DeviceAdded(info.clone())).is_err() {
                                    tracing::warn!(
                                        "热插拔事件通道已关闭，无法发送 DeviceAdded 事件"
                                    );
                                }
                            }
                        }

                        // 检测移除设备（在已知列表但不在当前列表中）
                        for name in &known_devices {
                            if !current_names.contains(name) {
                                tracing::info!(
                                    "检测到设备移除: {}",
                                    name
                                );
                                if tx.send(HotplugEvent::DeviceRemoved(name.clone())).is_err() {
                                    tracing::warn!(
                                        "热插拔事件通道已关闭，无法发送 DeviceRemoved 事件"
                                    );
                                }
                            }
                        }

                        // 更新已知设备集合为当前快照
                        known_devices = current_names;
                    }
                }
                Err(e) => {
                    // 枚举出错时不崩溃，记录警告并跳过本轮
                    tracing::warn!(
                        "热插拔监听：枚举输出设备失败，跳过本轮: {}",
                        e
                    );
                }
            }

            // 等待一个轮询间隔再进入下一轮
            std::thread::sleep(poll_interval);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 HotplugEvent 枚举实现了 Debug + Clone
    #[test]
    fn test_hotplug_event_debug_clone() {
        let added = HotplugEvent::DeviceAdded(DeviceInfo {
            name: "测试设备".to_string(),
            sample_rates: vec![],
            channels: vec![],
            formats: vec![],
            device_type: crate::device::DeviceType::Unknown,
        });
        let cloned = added.clone();
        assert!(
            format!("{:?}", cloned).contains("测试设备"),
            "DeviceAdded 应支持 Debug 和 Clone"
        );

        let removed = HotplugEvent::DeviceRemoved("测试移除".to_string());
        let cloned = removed.clone();
        assert!(
            format!("{:?}", cloned).contains("测试移除"),
            "DeviceRemoved 应支持 Debug 和 Clone"
        );
    }

    /// 验证监听线程能响应停止标志正常退出
    #[test]
    fn test_monitor_stops_on_flag() {
        let (tx, _rx) = crossbeam_channel::bounded::<HotplugEvent>(16);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let flag_clone = Arc::clone(&stop_flag);

        let handle = start_hotplug_monitor(
            tx,
            Arc::clone(&stop_flag),
            Duration::from_millis(100),
        );

        // 短暂等待线程启动
        std::thread::sleep(Duration::from_millis(50));

        // 设置停止标志
        flag_clone.store(true, Ordering::Relaxed);

        // 等待线程退出，设置超时以防止无限等待
        handle
            .join()
            .expect("热插拔监听线程应正常退出");
    }
}

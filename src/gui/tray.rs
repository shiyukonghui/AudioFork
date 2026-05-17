// 系统托盘模块 — 托盘图标和菜单管理
// 使用 tray-icon crate 实现跨平台系统托盘功能
// 托盘线程直接调用 Windows ShowWindow API 控制窗口可见性
// 退出处理也在托盘线程中直接完成（不依赖 eframe update()）

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crossbeam_channel::Sender;
use tray_icon::{
    menu::{Menu, MenuEvent, MenuId, MenuItem},
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
};

use crate::message::GuiToEngine;

// ============================================================================
// TrayState
// ============================================================================

/// 系统托盘状态管理
/// 托盘图标生命周期由此结构体持有，drop 时图标消失
pub struct TrayState {
    #[allow(dead_code)]
    tray_icon: TrayIcon,
}

impl TrayState {
    /// 创建并初始化系统托盘
    ///
    /// # 参数
    /// * `engine_tx` — 发送 Stop 消息给引擎的通道，退出时使用
    pub fn new(engine_tx: Sender<GuiToEngine>) -> Result<Self, String> {
        let menu = Menu::new();

        let menu_toggle = MenuItem::new("隐藏窗口", true, None);
        let menu_toggle_id = menu_toggle.id().clone();
        menu.append(&menu_toggle).map_err(|e| format!("添加菜单项失败: {}", e))?;

        let menu_quit = MenuItem::new("退出", true, None);
        let menu_quit_id = menu_quit.id().clone();
        menu.append(&menu_quit).map_err(|e| format!("添加菜单项失败: {}", e))?;

        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("音频路由器")
            .with_icon(create_app_icon())
            .build()
            .map_err(|e| format!("创建托盘图标失败: {}", e))?;

        let window_visible = Arc::new(AtomicBool::new(true));

        // 启动托盘事件监听线程
        let window_visible_clone = Arc::clone(&window_visible);
        let menu_toggle_id_clone = menu_toggle_id.clone();
        let menu_quit_id_clone = menu_quit_id.clone();

        std::thread::spawn(move || {
            let hwnd = find_window_with_retry();
            tracing::info!("[TRAY] 事件线程已启动, hwnd={:?}", hwnd);

            loop {
                if let Ok(event) = TrayIconEvent::receiver().try_recv() {
                    handle_tray_click(event, &window_visible_clone, hwnd);
                }

                if let Ok(event) = MenuEvent::receiver().try_recv() {
                    handle_menu_click(
                        event,
                        &window_visible_clone,
                        hwnd,
                        &engine_tx,
                        &menu_toggle_id_clone,
                        &menu_quit_id_clone,
                    );
                }

                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });

        Ok(Self { tray_icon })
    }
}

// ============================================================================
// 窗口句柄
// ============================================================================

fn find_window_with_retry() -> Option<isize> {
    for _ in 0..100 {
        if let Some(hwnd) = find_window_hwnd() {
            return Some(hwnd);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    tracing::warn!("未找到窗口句柄");
    None
}

#[cfg(windows)]
fn find_window_hwnd() -> Option<isize> {
    extern "system" {
        fn FindWindowW(lpClassName: *const u16, lpWindowName: *const u16) -> isize;
    }
    let title: Vec<u16> = "音频路由器\0".encode_utf16().collect();
    unsafe {
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if hwnd == 0 { None } else { Some(hwnd) }
    }
}

#[cfg(not(windows))]
fn find_window_hwnd() -> Option<isize> { None }

// ============================================================================
// 事件处理
// ============================================================================

fn handle_tray_click(
    event: TrayIconEvent,
    visible: &Arc<AtomicBool>,
    hwnd: Option<isize>,
) {
    let toggle = match event {
        TrayIconEvent::DoubleClick { button, .. } => button == MouseButton::Left,
        TrayIconEvent::Click { button, button_state, .. } => {
            button == MouseButton::Left && button_state == MouseButtonState::Up
        }
        _ => false,
    };
    if toggle {
        toggle_window(visible, hwnd);
    }
}

fn handle_menu_click(
    event: MenuEvent,
    visible: &Arc<AtomicBool>,
    hwnd: Option<isize>,
    engine_tx: &Sender<GuiToEngine>,
    menu_toggle_id: &MenuId,
    menu_quit_id: &MenuId,
) {
    if event.id == *menu_toggle_id {
        toggle_window(visible, hwnd);
    } else if event.id == *menu_quit_id {
        tracing::info!("[TRAY] 退出：停止引擎...");
        let _ = engine_tx.send(GuiToEngine::Stop);
        tracing::info!("[TRAY] 退出：等待引擎停止...");
        std::thread::sleep(std::time::Duration::from_millis(500));
        tracing::info!("[TRAY] 退出：process::exit(0)");
        std::io::Write::flush(&mut std::io::stdout()).ok();
        std::process::exit(0);
    }
}

fn toggle_window(visible: &Arc<AtomicBool>, hwnd: Option<isize>) {
    if visible.load(Ordering::SeqCst) {
        hide_window(hwnd, visible);
    } else {
        show_window(hwnd, visible);
    }
}

// ============================================================================
// Windows 窗口控制
// ============================================================================

#[cfg(windows)]
fn hide_window(hwnd: Option<isize>, visible: &Arc<AtomicBool>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE};
    if let Some(h) = hwnd {
        unsafe { let _ = ShowWindow(HWND(h as *mut core::ffi::c_void), SW_HIDE); }
        visible.store(false, Ordering::SeqCst);
        tracing::info!("窗口已隐藏到系统托盘");
    }
}

#[cfg(windows)]
fn show_window(hwnd: Option<isize>, visible: &Arc<AtomicBool>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};
    if let Some(h) = hwnd {
        let hwnd = HWND(h as *mut core::ffi::c_void);
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        visible.store(true, Ordering::SeqCst);
        tracing::info!("窗口已从系统托盘恢复");
    }
}

#[cfg(not(windows))]
fn hide_window(_hwnd: Option<isize>, visible: &Arc<AtomicBool>) {
    visible.store(false, Ordering::SeqCst);
}
#[cfg(not(windows))]
fn show_window(_hwnd: Option<isize>, visible: &Arc<AtomicBool>) {
    visible.store(true, Ordering::SeqCst);
}

// ============================================================================
// 图标
// ============================================================================

fn create_app_icon() -> Icon {
    let width = 32u32;
    let height = 32u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let cx = width as f32 / 2.0;
            let cy = height as f32 / 2.0;
            let radius = (width.min(height) as f32 / 2.0) - 2.0;
            let dist = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            if dist <= radius {
                rgba.extend_from_slice(&[30, 100, 200, 255]);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    Icon::from_rgba(rgba, width, height).expect("创建图标失败")
}

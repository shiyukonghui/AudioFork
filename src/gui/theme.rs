//! GUI 主题管理模块
//!
//! 提供浅色/深色主题的自动检测与切换功能。
//! Windows 下通过读取注册表检测系统主题设置，
//! 非 Windows 平台默认使用浅色主题。

use std::process::Command;

/// 主题模式：浅色或深色
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    /// 浅色主题
    Light,
    /// 深色主题
    Dark,
}

/// 检测当前系统的主题模式
///
/// Windows 平台：通过查询注册表键值
/// `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`
/// 来判断系统使用的是浅色还是深色主题。
///
/// 非 Windows 平台：默认返回浅色主题。
pub fn detect_system_theme() -> ThemeMode {
    #[cfg(target_os = "windows")]
    {
        detect_windows_theme()
    }

    #[cfg(not(target_os = "windows"))]
    {
        ThemeMode::Light
    }
}

/// 在 Windows 平台通过注册表查询系统主题
#[cfg(target_os = "windows")]
fn detect_windows_theme() -> ThemeMode {
    // 使用 reg query 命令查询 AppsUseLightTheme 注册表值
    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            // 将命令输出转换为字符串
            let stdout = String::from_utf8_lossy(&output.stdout);

            // 检查返回值：0x1 表示浅色，0x0 表示深色
            // 注册表输出格式通常为: "    AppsUseLightTheme    REG_DWORD    0x1"
            if stdout.contains("0x1") || stdout.contains('1') {
                ThemeMode::Light
            } else if stdout.contains("0x0") || stdout.contains('0') {
                ThemeMode::Dark
            } else {
                // 无法解析时默认返回浅色主题
                ThemeMode::Light
            }
        }
        _ => {
            // 命令执行失败时默认返回浅色主题
            ThemeMode::Light
        }
    }
}

/// 将指定主题应用到 egui 上下文中
///
/// # 参数
///
/// * `ctx` - egui 的上下文引用
/// * `mode` - 要应用的主题模式
///
/// # 示例
///
/// ```ignore
/// let theme = detect_system_theme();
/// apply_theme(&ctx, theme);
/// ```
pub fn apply_theme(ctx: &egui::Context, mode: ThemeMode) {
    match mode {
        ThemeMode::Light => {
            // 应用 egui 内置的浅色视觉样式
            ctx.set_visuals(egui::Visuals::light());
        }
        ThemeMode::Dark => {
            // 应用 egui 内置的深色视觉样式
            ctx.set_visuals(egui::Visuals::dark());
        }
    }
}

# Checklist

- [x] Cargo.toml 中已添加 tray-icon 依赖，cargo check 通过
- [x] src/gui/tray.rs 模块已创建，包含 TrayState 结构体和托盘初始化函数
- [x] src/gui/mod.rs 中已声明 tray 子模块
- [x] run_gui() 函数中已初始化托盘图标
- [x] 窗口关闭事件已拦截，改为隐藏窗口而非退出
- [x] 使用 egui ViewportCommand::Visible 控制窗口可见性
- [x] 双击托盘图标可恢复显示窗口
- [x] 托盘右键菜单包含"显示窗口"和"退出"选项
- [x] 托盘菜单根据窗口状态动态显示"显示窗口"或"隐藏窗口"
- [x] 窗口隐藏后音频引擎继续正常运行
- [x] 选择"退出"菜单项时程序正常退出（引擎先停止）
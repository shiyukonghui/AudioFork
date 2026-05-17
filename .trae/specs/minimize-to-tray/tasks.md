# Tasks

- [x] Task 1: 添加 tray-icon crate 依赖
  - [x] SubTask 1.1: 在 Cargo.toml 中添加 tray-icon 依赖（版本 0.19 或最新稳定版）
  - [x] SubTask 1.2: 运行 cargo check 验证依赖正确下载

- [x] Task 2: 创建系统托盘模块 src/gui/tray.rs
  - [x] SubTask 2.1: 定义 TrayState 结构体，管理托盘图标、菜单项和窗口可见状态
  - [x] SubTask 2.2: 实现 create_tray_icon() 函数，创建托盘图标和菜单
  - [x] SubTask 2.3: 实现 tray event handler，处理双击和菜单点击事件
  - [x] SubTask 2.4: 使用 Arc<Mutex<bool>> 或 AtomicBool 共享窗口可见状态

- [x] Task 3: 在 src/gui/mod.rs 中集成托盘功能
  - [x] SubTask 3.1: 在 mod.rs 中声明 tray 子模块
  - [x] SubTask 3.2: 在 run_gui() 函数中初始化托盘图标
  - [x] SubTask 3.3: 获取窗口 HWND（Windows 平台）用于 ShowWindow API
  - [x] SubTask 3.4: 实现 on_close_event() 拦截关闭事件，改为隐藏窗口

- [x] Task 4: 实现窗口隐藏/恢复逻辑
  - [x] SubTask 4.1: 使用 windows crate 的 ShowWindow API 隐藏窗口（SW_HIDE）
  - [x] SubTask 4.2: 使用 ShowWindow API 恢复窗口（SW_SHOWDEFAULT）
  - [x] SubTask 4.3: 确保窗口隐藏后 egui context 继续运行（避免 CPU 高占用问题)
- [x] Task 5: 实现托盘菜单状态同步
  - [x] SubTask 5.1: 根据窗口可见状态动态更新菜单项文本（显示/隐藏窗口）
  - [x] SubTask 5.2: 根据引擎运行状态更新托盘图标颜色或样式

- [x] Task 6: 测试和验证
  - [x] SubTask 6.1: 测试点击最小化按钮隐藏窗口到托盘
  - [x] SubTask 6.2: 测试点击关闭按钮隐藏窗口到托盘
  - [x] SubTask 6.3: 测试双击托盘图标恢复窗口
  - [x] SubTask 6.4: 测试托盘菜单"显示窗口"和"退出"功能
  - [x] SubTask 6.5: 测试窗口隐藏后引擎继续运行

# Task Dependencies
- [Task 2] depends on [Task 1]
- [Task 3] depends on [Task 2]
- [Task 4] depends on [Task 3]
- [Task 5] depends on [Task 4]
- [Task 6] depends on [Task 5]
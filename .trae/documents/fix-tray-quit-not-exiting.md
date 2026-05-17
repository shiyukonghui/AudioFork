# 修复：托盘退出后进程不终止

## 问题描述
点击托盘图标右键菜单的"退出"后：
- 托盘图标不消失
- 从任务管理器看进程/线程仍在运行
- 音频路由功能仍在继续

## 根因分析

退出流程当前是：
```
托盘菜单点击"退出"
  → handle_menu_click() 发送 TrayAction::Quit 到通道
  → mod.rs update() 收到 Quit
     → 设置 self.quitting = true
     → 发送 GuiToEngine::Stop 给引擎 ✓
     → 发送 ViewportCommand::Close 给 egui ✓
     → return
  → eframe::run_native 返回
  → run_gui() 返回
  → main() 执行完毕
  → 【但是进程不退出，因为还有线程在运行！】
```

有三个分离线程（detached threads）阻止进程正常退出：

| 线程 | 创建位置 | 问题 |
|------|----------|------|
| 托盘事件线程 | `tray.rs:74` `std::thread::spawn` | **无限循环 `loop {}`，永不退出** |
| 引擎线程 | `engine.rs:50` `std::thread::spawn` | `Stop` 后回到外层 `loop` 等待下一个 `Start`，不退出 |
| 热插拔线程 | `engine.rs:437` `start_hotplug_monitor` | 引擎线程在 Stop 后会 join 这个线程 ✓ |

## 修复方案

### 方案一：传递停止信号给托盘线程（推荐）

**tray.rs 修改：**
1. 在 `TrayState::new()` 中创建 `Arc<AtomicBool>` 停止标志
2. 传给托盘事件线程，在 `loop` 中检查此标志
3. 提供 `stop_tray()` 方法，设置停止标志

**mod.rs 修改：**
1. 在退出流程中调用 `tray_state.stop_tray()`

### 方案二：进程兜底退出

**main.rs 修改：**
1. `run_gui()` 返回后调用 `std::process::exit(0)` 强制退出

---

## 实施步骤

### Step 1: tray.rs — 添加停止机制
- 在 `TrayState` 中添加 `stop_flag: Arc<AtomicBool>` 字段
- `new()` 中创建此标志并克隆给事件线程
- 事件线程 `loop` 中检查 `stop_flag`，为 true 时 break
- 添加 `stop(&self)` 方法设置标志

### Step 2: mod.rs — 退出时停止托盘
- 在 `AudioRouterApp` 中添加 `stop_tray_on_exit()` 方法
- 在收到 `TrayAction::Quit` 后先调用 `tray_state.stop()`
- 然后再发送 `ViewportCommand::Close`

### Step 3: engine.rs — 优雅退出
- 引擎收到 `Stop` 后，回到外层循环时检查通道是否断开
- 通道已断开则 break 退出线程

### Step 4: 验证
- `cargo build --release` 编译通过
- 运行验证点击退出后进程正常终止

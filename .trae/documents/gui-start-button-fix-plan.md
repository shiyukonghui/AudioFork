# GUI 启动按钮无响应问题排查与修复计划

## 问题描述
点击 GUI 界面的启动按钮后，GUI 没有日志输出，终端也没有任何输出，软件没有任何反应。

## 问题分析

通过代码审查，发现以下潜在问题：

### 1. 引擎线程 panic 未被捕获
在 `engine.rs` 的 `spawn_engine` 函数中，引擎线程在独立线程中运行。如果 `run_pipeline` 函数发生 panic，GUI 不会收到任何通知，因为：
- panic 发生在独立线程中
- GUI 的 panic hook 只能捕获 GUI 线程的 panic
- 引擎线程 panic 后，线程直接终止，不会发送任何消息给 GUI

### 2. 引擎线程日志可能不可见
在 GUI 模式下，tracing subscriber 在 `main.rs` 中初始化，但引擎线程的日志可能因为线程隔离而不可见。

### 3. 设备选择可能静默失败
如果 `selected_output_devices` 为空，引擎会返回错误，但这个错误应该能正确传递给 GUI。

### 4. 音频流创建可能失败
如果音频流创建失败（如设备被占用），错误应该传递给 GUI，但如果在传递过程中发生 panic，GUI 不会收到。

## 修复方案

### 步骤 1: 为引擎线程添加 panic 捕获
在 `spawn_engine` 函数中使用 `std::panic::catch_unwind` 包装 `run_pipeline` 调用，捕获 panic 并发送错误消息给 GUI。

**修改文件**: `src/engine.rs`

```rust
pub fn spawn_engine(
    gui_rx: Receiver<GuiToEngine>,
    gui_tx: Sender<EngineToGui>,
) {
    std::thread::spawn(move || {
        tracing::info!("引擎后台线程已启动，等待 GUI 指令...");

        // 阻塞等待第一条 Start 消息
        match gui_rx.recv() {
            Ok(GuiToEngine::Start(config)) => {
                tracing::info!("引擎线程收到启动指令");
                // 使用 catch_unwind 捕获 panic
                let result = std::panic::catch_unwind(|| {
                    run_pipeline(config, &gui_tx, &gui_rx)
                });
                match result {
                    Ok(Ok(stats)) => {
                        let _ = gui_tx.send(EngineToGui::Stopped { stats });
                    }
                    Ok(Err(e)) => {
                        let _ = gui_tx.send(EngineToGui::Error(e.to_string()));
                    }
                    Err(panic_info) => {
                        // 捕获到 panic，发送错误消息给 GUI
                        let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                            format!("引擎线程 panic: {}", s)
                        } else if let Some(s) = panic_info.downcast_ref::<String>() {
                            format!("引擎线程 panic: {}", s)
                        } else {
                            "引擎线程发生未知 panic".to_string()
                        };
                        tracing::error!("{}", msg);
                        let _ = gui_tx.send(EngineToGui::Error(msg));
                    }
                }
            }
            Ok(GuiToEngine::Stop) => {
                tracing::debug!("引擎线程在启动前收到 Stop，已忽略");
            }
            Err(_) => {
                tracing::debug!("GUI 通道已关闭，引擎线程退出");
            }
        }

        tracing::info!("引擎后台线程退出");
    });
}
```

### 步骤 2: 在 GUI 启动时添加调试日志
在 `gui/mod.rs` 的 `run_gui` 函数中添加更多调试日志，帮助追踪问题。

**修改文件**: `src/gui/mod.rs`

在 `run_gui` 函数中添加：
```rust
tracing::info!("正在创建消息通道...");
// ... 创建通道
tracing::info!("正在启动引擎线程...");
crate::engine::spawn_engine(gui_to_engine_rx, engine_to_gui_tx);
tracing::info!("引擎线程已启动，正在创建 GUI 应用...");
```

### 步骤 3: 在启动按钮点击时添加调试日志
在 `gui/mod.rs` 的 `update` 函数中，处理 `ToolbarAction::Start` 时添加日志：

```rust
toolbar::ToolbarAction::Start => {
    tracing::info!("用户点击了启动按钮");
    // ... 构建配置
    tracing::info!("正在发送启动指令给引擎，输出设备: {:?}", config.output_devices);
    let _ = self.engine_tx.send(GuiToEngine::Start(config));
    tracing::info!("启动指令已发送");
}
```

### 步骤 4: 添加引擎线程就绪确认机制
修改 `spawn_engine` 函数，在引擎线程启动后发送一个就绪消息给 GUI，确保线程已正确启动。

**修改文件**: `src/message.rs`

添加新的消息类型：
```rust
pub enum EngineToGui {
    /// 引擎线程已就绪，等待启动指令
    Ready,
    // ... 其他消息
}
```

**修改文件**: `src/gui/mod.rs`

在消息处理中添加：
```rust
EngineToGui::Ready => {
    self.log_panel.push(chrono_now(), "引擎线程已就绪".to_string());
}
```

### 步骤 5: 添加输出设备为空的检查
在发送启动指令前，检查是否选择了输出设备。

**修改文件**: `src/gui/mod.rs`

```rust
toolbar::ToolbarAction::Start => {
    if self.devices.selected_output_devices.is_empty() {
        self.log_panel.push(chrono_now(), "错误: 请至少选择一个输出设备".to_string());
        self.status_bar.engine_status = status::EngineStatus::Error(
            "未选择输出设备".to_string()
        );
        return;
    }
    // ... 其余代码
}
```

## 测试验证

1. 编译并运行 GUI 模式
2. 观察终端输出，确认引擎线程启动日志
3. 点击启动按钮，观察日志输出
4. 如果有错误，检查 GUI 日志面板和终端输出

## 文件修改清单

1. `src/engine.rs` - 添加 panic 捕获和就绪消息
2. `src/gui/mod.rs` - 添加调试日志和输出设备检查
3. `src/message.rs` - 添加 Ready 消息类型

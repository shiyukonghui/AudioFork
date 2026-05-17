# GUI 界面点击启动无响应 — 排查报告与修复计划

## 一、问题现象

在 GUI 图形界面中点击「启动」按钮后：
- 没有任何日志输出
- 引擎不启动，状态指示灯保持灰色「已停止」
- 无任何错误提示

---

## 二、根因分析

### 核心原因：**音频引擎后台线程未实现，启动消息无人消费**

消息通道的架构设计是正确的：
- `gui_to_engine_tx`（GUI 端发送器）— 正确传入 `AudioRouterApp::new()`，点击启动时能成功发送 `GuiToEngine::Start(config)`
- `gui_to_engine_rx`（引擎端接收器）— **被直接丢弃，没有任何线程监听**

关键证据在 [gui/mod.rs:L468-L470](file:///f:\RustProjects\AudioFork\src\gui\mod.rs#L468-L470)：

```rust
// 提示：gui_to_engine_rx 预留给未来的引擎线程使用
// 当前 GUI 模式下仅搭建通道框架，引擎端将在后续阶段实现
let _ = gui_to_engine_rx;
```

### 完整调用链路分析

```
用户点击「启动」按钮
  ↓
toolbar.rs:70 — ui.button("▶ 启动").clicked() → 返回 ToolbarAction::Start
  ↓
gui/mod.rs:234-255 — 匹配 ToolbarAction::Start：
  1. 从设备面板 / 参数面板收集配置
  2. 构建 EngineConfig
  3. self.engine_tx.send(GuiToEngine::Start(config))  ← 消息成功发送
  ↓
❌ 没有任何线程在 gui_to_engine_rx 上 recv() 等待
  ↓
消息积压在通道中，永不被消费 → 引擎不启动，GUI 不更新
```

### 同样缺失的部分

同样，`engine_to_gui_tx`（引擎→GUI 发送器）也未被传入任何线程，因此即使引擎启动了，GUI 也无法收到 `EngineToGui::Started` / `EngineToGui::Stopped { stats }` / `EngineToGui::Error(msg)` 等状态消息。

---

## 三、修复方案

### 总体策略

在 `gui::run_gui()` 中，**启动 eframe 之前**，spawn 一个后台引擎线程：
- 该线程持有 `gui_to_engine_rx`（接收 Start/Stop 指令）
- 该线程持有 `engine_to_gui_tx`（向 GUI 推送状态）
- 复用 `main.rs::run()` 中已有的核心音频管道代码（设备枚举 → 管道创建 → 主循环）

### 核心设计

```
┌─────────────────┐          ┌─────────────────────────┐
│   GUI 主线程     │          │   引擎后台线程            │
│  (eframe 事件循环)│          │                         │
│                 │  Start   │                         │
│  engine_tx ─────┼─────────→│ gui_to_engine_rx.recv()  │
│                 │          │   ↓                     │
│  engine_rx ←────┼──────────│ engine_to_gui_tx.send()  │
│                 │  Status  │   (Started/Stopped/Err)  │
└─────────────────┘          └─────────────────────────┘
```

### 具体步骤

#### 步骤 1：修复 `run_gui()` —— 保留发送器、spawn 引擎线程

**文件**: `src/gui/mod.rs`，函数 `run_gui()`

当前代码：
```rust
let (_engine_to_gui_tx, engine_to_gui_rx) =
    crossbeam_channel::unbounded::<EngineToGui>();
let (gui_to_engine_tx, gui_to_engine_rx) =
    crossbeam_channel::unbounded::<GuiToEngine>();
let _ = gui_to_engine_rx;  // ← 丢弃接收器
```

修改为：
1. `_engine_to_gui_tx` 改名为 `engine_to_gui_tx`（移除下划线前缀，保留变量）
2. 将 `engine_to_gui_tx` 和 `gui_to_engine_rx` 传给新创建的引擎线程
3. 不再丢弃 `gui_to_engine_rx`

#### 步骤 2：新建 `src/engine.rs` —— 引擎线程主函数

**文件**: 新建 `src/engine.rs`

从 `main.rs` 的 `run()` 函数中提取核心音频管道代码，改造为可被 GUI 消息驱动的后台线程：

```rust
use std::thread;
use crossbeam_channel::Receiver;
use crate::message::{EngineConfig, EngineToGui, GuiToEngine};

/// 在独立线程中运行音频引擎，监听 GUI 消息并反馈状态
pub fn run_engine_thread(
    gui_rx: Receiver<GuiToEngine>,
    gui_tx: crossbeam_channel::Sender<EngineToGui>,
) {
    thread::spawn(move || {
        // 等待 Start 消息
        match gui_rx.recv() {
            Ok(GuiToEngine::Start(config)) => {
                // 启动音频管道
                let result = start_audio_pipeline(config, &gui_tx);
                match result {
                    Ok(stats) => {
                        let _ = gui_tx.send(EngineToGui::Stopped { stats });
                    }
                    Err(e) => {
                        let _ = gui_tx.send(EngineToGui::Error(e.to_string()));
                    }
                }
            }
            Ok(GuiToEngine::Stop) => {
                // 引擎尚未启动，忽略
            }
            Err(_) => {
                // 通道已关闭（GUI 退出）
            }
        }
    });
}

fn start_audio_pipeline(
    config: EngineConfig,
    gui_tx: &crossbeam_channel::Sender<EngineToGui>,
) -> crate::error::Result<Vec<crate::message::OutputSnapshot>> {
    // 1. 设备枚举与选择
    // 2. 创建管道（复用 main.rs 中的逻辑）
    // 3. 发送 EngineToGui::Started
    // 4. 主循环：同时监听 gui_rx（非阻塞 try_recv）和音频处理
    // 5. 收到 Stop 或管道结束 → 淡出 → 返回最终统计
    todo!()
}
```

**关键设计考量**：
- 引擎线程需要在"等待音频设备就绪"、"处理音频回调"的同时能够响应 GUI 的 `Stop` 消息
- 使用 `gui_rx.try_recv()` 在主循环中非阻塞轮询
- 音频管道的主循环目前是阻塞式 `stdin().read_line()`——在 GUI 模式下改为循环+sleep+try_recv 模式

#### 步骤 3：注册 `engine` 模块

**文件**: `src/lib.rs` 和 `src/main.rs`

在 `lib.rs` 添加 `pub mod engine;`
在 `main.rs` 添加 `mod engine;`

#### 步骤 4：提取共享的管道启动逻辑

当前 `main.rs::run()` 中包含约 600 行的音频管道启动代码（设备枚举、管道创建、主循环等）。需要将这部分逻辑提取为可复用的函数，供 CLI 模式和 GUI 引擎线程共同调用。

可以通过以下方式之一实现：
- **方案 A**: 将 `main.rs` 中的管道逻辑提取为 `pipeline::run_audio_pipeline(config, ...)` 公共函数
- **方案 B**: 在 `engine.rs` 中复制必要逻辑（不推荐，会导致代码重复）

**推荐方案 A**，提取公共函数到 `src/audio/` 或 `src/pipeline.rs` 模块。

---

## 四、影响范围

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| `src/gui/mod.rs` | 修改 | `run_gui()` — spawn 引擎线程，保留通道发送器 |
| `src/engine.rs` | **新建** | 引擎后台线程实现 |
| `src/lib.rs` | 修改 | 添加 `pub mod engine;` |
| `src/main.rs` | 修改 | 提取公共管道逻辑，添加 `mod engine;` |
| `src/pipeline.rs` 或 `src/audio/mod.rs` | 可能修改 | 提取可复用的管道启动函数 |

**不影响的模块**：
- `src/cli.rs` — CLI 参数定义，无需修改
- `src/config.rs` — 配置读写，无需修改
- `src/message.rs` — 消息类型定义，无需修改
- `src/gui/toolbar.rs` / `devices.rs` / `params.rs` / `status.rs` / `logs.rs` / `theme.rs` — GUI 面板，无需修改

---

## 五、验证方式

修复后，验证以下场景：

1. **正常启动**: 在 GUI 中选择输入/输出设备 → 点击「启动」→ 状态指示灯变绿 → 日志显示"引擎已启动"
2. **正常停止**: 运行中点击「停止」→ 状态指示灯变灰 → 日志显示"引擎已停止" → 状态栏显示最终统计
3. **无设备启动**: 未选择任何输出设备 → 点击「启动」→ 日志显示错误信息
4. **GUI 关闭时停止**: 引擎运行中关闭窗口 → 引擎自动停止（通过 `Drop` 或显式 Stop 消息）
5. **错误隔离**: 引擎线程 panic 不影响 GUI 主线程（已有的 panic hook 会捕获）

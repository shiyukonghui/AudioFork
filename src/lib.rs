// 音频路由器库 — 模块声明入口
// 支持 `cargo check --lib` 对库目标进行编译检查
pub mod error;
pub mod config;
pub mod device;
pub mod audio;
pub mod cli;
pub mod channel_map;
pub mod pipeline;
pub mod recovery;
pub mod drift;
pub mod limiter;
pub mod hotplug;
pub mod resample;
pub mod message;
pub mod source;
pub mod gui;
pub mod engine;

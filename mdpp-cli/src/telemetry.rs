//! 日志：把 `tracing` 的订阅器装起来。
//!
//! ## 只有 `--debug` 才装
//!
//! 默认**什么都不装** —— 没有订阅器时 `tracing` 的宏逐条都是一次「没订阅者」的
//! 分支，什么都不打。这是命令行该有的默认：一件一次性的事，不该在正常输出旁边
//! 多出几行与任务无关的字。
//!
//! `--debug` 装一个 DEBUG 级的 `fmt` 订阅器，写到**标准错误** —— 日志与产物分开，
//! 于是 `mdpp --debug 正文.md > 输出.html` 出来的文件里没有一行日志。
//!
//! ## 为什么不读 `RUST_LOG`
//!
//! `EnvFilter` 会把 `matchers` 与 `regex` 拖进来（传递依赖里多两个 crate），而这条
//! 命令行本来是零依赖的取舍。要调档就改 `--debug` 的实现，那是一行的事。

use std::sync::Once;

static INSTALLED: Once = Once::new();

/// 按需装订阅器。`debug` 为假时**什么都不做**。
///
/// 重复调用无害（`Once`）—— 那是「谁先装上算谁的」的语义，而这里装的是同一个东西，
/// 所以先到先得正合适。
pub fn install(debug: bool) {
    if !debug {
        return;
    }
    INSTALLED.call_once(|| {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(std::io::stderr)
            .with_target(false)
            .without_time()
            .init();
    });
}

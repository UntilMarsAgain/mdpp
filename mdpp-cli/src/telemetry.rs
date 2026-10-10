//! 日志：把 `tracing` 的订阅器装起来。
//!
//! 默认**什么都不装** —— 没有订阅器时 `tracing` 的宏逐条都是一次「没订阅者」的分支，
//! 什么都不打。`--debug` 装一个 DEBUG 级的 `fmt` 订阅器，写到**标准错误** —— 于是
//! `mdpp --debug 正文.md > 输出.html` 出来的文件里没有一行日志。

use std::sync::Once;

static INSTALLED: Once = Once::new();

/// 按需装订阅器。`debug` 为假时**什么都不做**。重复调用无害（`Once`）。
pub fn install(debug: bool) {
    if !debug {
        return;
    }
    // 不读 RUST_LOG：EnvFilter 会把 matchers 与 regex 拖进来（传递依赖多两个 crate），
    // 而这条命令行是零依赖的取舍。要调档改下面这一行。
    INSTALLED.call_once(|| {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_writer(std::io::stderr)
            .with_target(false)
            .without_time()
            .init();
    });
}

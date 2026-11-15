//! 日志：把 `tracing` 的订阅器装起来。
//!
//! 默认装一个 **INFO** 级的 `fmt` 订阅器，写到**标准错误** —— 于是
//! `mdpp 正文.mdpp > 输出.html` 出来的文件里没有一行日志，而「建了哪些页」「红链几条」
//! 这类报告仍看得见。`--debug` 把级别降到 DEBUG，渲染规模、扫描结果这些排查信息也出来。

use std::sync::Once;

static INSTALLED: Once = Once::new();

/// 装订阅器。`debug` 决定最低级别；重复调用无害（`Once`）。
pub fn install(debug: bool) {
    // 不读 RUST_LOG：EnvFilter 会把 matchers 与 regex 拖进来（传递依赖多两个 crate），
    // 而这条命令行是零依赖的取舍。要调档改下面这一行。
    use tracing_subscriber::filter::{LevelFilter, Targets};
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let level = if debug {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    let ansi = {
        use std::io::IsTerminal;
        std::io::stderr().is_terminal()
    };
    INSTALLED.call_once(|| {
        // 只给自己这两个 crate 放宽到 DEBUG：html5ever 这类依赖在 DEBUG 级会逐
        // token 打树构建过程，`--debug` 一开就淹没真正想看的行。
        let filter = Targets::new()
            .with_default(LevelFilter::INFO)
            .with_target("mdpp", level)
            .with_target("mdpp_cli", level);
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_writer(std::io::stderr)
                    .with_ansi(ansi)
                    .with_target(false)
                    .without_time(),
            )
            .with(filter)
            .init();
    });
}

//! 日志：本库发日志的地方，都从**这一个文件**取宏。
//!
//! ## 为什么单独起一个文件
//!
//! 库里的 `tracing::debug!` 全写出来看着像在喊 slog，但换门面（换 `log`、换
//! 自己的宏）时得改几十处。集中到一处就只改一处，而且 `grep 日志` 一网打尽。
//!
//! ## 级别怎么定的
//!
//! - **`debug!`** —— 「发生了什么，而它可能不是作者写的」：一次渲染的规模、
//!   一个没注册的块名、一个读不出来的模板页。排查问题时要看的就��这一档。
//! - **`trace!`** —— 每一块、每一个内部链接。默认关着；开它就是想要逐行看。
//! - **`warn!` / `error!`** —— 只留给「作者大概不知道自己这么写了」的那种事：
//!   模板块套得太深、`<markdown>` 套得太深。它们已经渲染成框给人看了，日志是
//!   第二条路（给看 HTML 的人之外的场合：日志采集、测试断言）。
//! - **`info!`** —— 本库**不发**。那是宿主程序的事（命令行会发，见 `src/main.rs`）。
//!
//! ## 订阅器不归本库管
//!
//! 库只发不装。装订阅器是**二进制**的事（`mdpp --debug`），或者使用方在自己的
//! 入口装 —— 这样「日志往哪去、什么格式、要不要过滤」都在使用方手里。
//!
//! 什么也不装的情况下，这些宏几乎不花钱：`tracing` 编译进去的是一个静态
//! callsite，调用点是空的，没有订阅者时逐条都是一次「没订阅者」的分支。

pub use tracing::{debug, error, info, trace, warn};

/// 一段正文的规模。发日志时多处要用，写出来免得每次想起那三个字段叫什么。
#[derive(Debug)]
pub struct Size {
    /// 输入的字节数
    pub in_bytes: usize,
    /// 输出的字节数
    pub out_bytes: usize,
    /// 花掉的微秒数
    pub micros: u128,
}

impl Size {
    /// 量一段「输入 → 输出」的规模
    pub fn measure(source: &str, elapsed: std::time::Duration, output: &str) -> Self {
        Self {
            in_bytes: source.len(),
            out_bytes: output.len(),
            micros: elapsed.as_micros(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Size;
    use std::time::Duration;

    #[test]
    fn a_size_is_just_three_numbers() {
        let size = Size::measure("甲乙", Duration::from_micros(1500), "<p>甲乙</p>");
        assert_eq!(size.in_bytes, "甲乙".len());
        assert_eq!(size.out_bytes, "<p>甲乙</p>".len());
        assert_eq!(size.micros, 1500);
    }
}
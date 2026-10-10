//! 本库发日志的宏全部从这里取。
//!
//! 本库**只发不装** —— 订阅器由宿主装（`mdpp --debug`，或使用方自己的入口）。
//!
//! 级别：`debug!` 是排查问题要看的（渲染规模、没注册的块名、读不出来的模板页）；
//! `trace!` 逐块逐链接，默认关着；`warn!` 给作者大概不知道自己这么写了的事
//! （嵌套超过上限）；`info!` 本库不发。

pub use tracing::{debug, error, info, trace, warn};

/// 一次渲染的规模。
#[derive(Debug)]
pub struct Size {
    /// 输入字节数
    pub in_bytes: usize,
    /// 输出字节数
    pub out_bytes: usize,
    /// 耗时（微秒）
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

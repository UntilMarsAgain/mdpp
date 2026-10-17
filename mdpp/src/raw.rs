//! 源码里的「原始区」：围栏代码块与 HTML 块。
//!
//! 两者的共同点是**markdown 的记号在里面不算记号**，于是 `![a](b c.png)` 的括号、
//! `[标签]` 的方括号都只是一段字。两处要问同一类问题：
//!
//! - [`crate::targets`] 问「这行的字能不能动」—— 围栏里的不能动，改了就成了说谎
//!   （代码片段里的写法是给人看的例子）；
//! - [`crate::block::scanner`] 问「这一行能不能当成 `[标签]` 的开头」—— 原始区里
//!   的不能，一行 `[这不是标签]` 会被当成新的一节，于是例子被劈成两半。
//!
//! 两处共用一个状态机，是因为**各写一份判定的话，两套规则迟早会对不上** —— 而
//! 「对不上」的表现是静悄悄的：代码里的一句 `[x]` 忽然变成了一个选项卡。
//!
//! 判定照 CommonMark 的两条：连续三个以上算一道围栏；**闭合的那道不短于开启的那道**，
//! 短的那一行只是围栏里的内容。
//!
//! ## HTML 块只认到「空行为止」
//!
//! CommonMark 把 HTML 块分了七类，逐类精确复刻不值当 —— 这里只认最常见的一类：
//! 行首是 `<` 加一个标签名，**到下一个空行为止**。这正是 `<div>…</div>` 那种成对
//! 写法的行为；`<table>` 那类需要专门条件才能起块的情形偶尔会差一行，那一行的
//! 后果只是「`[x]` 没被认成标签」，偏保守，不会吃掉内容。

/// 一行在围栏这件事上的身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FenceLine {
    /// 围栏的分隔行（开启或闭合的那一行本身）
    Delimiter,
    /// 在围栏里，但不是分隔行 —— 内容照原样，一个字都不该动
    Inside,
    /// 在围栏外面
    Outside,
}

/// 围栏的开关状态。
///
/// 喂一行进 [`Fences::feed`] 就得到它当下的身份；`0` 是「不在围栏里」。
#[derive(Debug, Clone, Copy, Default)]
pub struct Fences {
    /// 当前开着的那道围栏：`(记号, 长度)`；`None` = 没在围栏里
    open: Option<(char, usize)>,
}

impl Fences {
    /// 现在在围栏里吗
    pub fn is_open(&self) -> bool {
        self.open.is_some()
    }

    /// 喂一行，拿到它当下的身份。**分隔行自己会翻状态**，所以每一行都要喂。
    pub fn feed(&mut self, line: &str) -> FenceLine {
        let plain = if self.is_open() {
            FenceLine::Inside
        } else {
            FenceLine::Outside
        };

        let trimmed = line.trim_start();
        // 不是围栏记号开头 —— 只是一行普通内容
        let Some(marker) = trimmed.chars().next().filter(|ch| *ch == '`' || *ch == '~') else {
            return plain;
        };
        let run = trimmed.chars().take_while(|ch| *ch == marker).count();
        // 不足三个 —— 同样只是内容
        if run < 3 {
            return plain;
        }

        match self.open {
            // 还没开着：这一行开启一道
            None => {
                self.open = Some((marker, run));
                FenceLine::Delimiter
            }
            // 同一种记号，且不短于开启时那道 —— 闭合
            Some((open, width)) if open == marker && run >= width => {
                self.open = None;
                FenceLine::Delimiter
            }
            // 另一种记号，或同一种但更短：都是围栏里的内容。围栏不嵌套，
            // 所以另一种记号也**不开**新的一道 —— 它就是内容
            Some(_) => FenceLine::Inside,
        }
    }
}

/// 原始区：围栏代码块**加上** HTML 块。
///
/// 分节那一步用它 —— 它只需要知道「这一行能不能是 `[标签]` 的开头」。
#[derive(Debug, Clone, Copy, Default)]
pub struct Raw {
    fences: Fences,
    /// 在一个 HTML 块里吗（到空行为止）
    html: bool,
}

impl Raw {
    /// 在围栏里吗
    pub fn is_in_fence(&self) -> bool {
        self.fences.is_open()
    }

    /// 喂一行，回答「这一行是不是原始区的内容」。
    ///
    /// **分隔行本身也算**（它当然不是标签，但那样这条就不必再单独判一次）。
    pub fn feed(&mut self, line: &str) -> bool {
        match self.fences.feed(line) {
            FenceLine::Inside => return true,
            FenceLine::Delimiter => {
                // 围栏的分隔行不可能在 HTML 块里，状态收掉免得串味
                self.html = false;
                return true;
            }
            FenceLine::Outside => {}
        }

        if self.html {
            // 空行结束 HTML 块 —— 这一行本身不算内容
            if line.trim().is_empty() {
                self.html = false;
                return false;
            }
            return true;
        }
        if starts_html_block(line) {
            self.html = true;
            return true;
        }
        false
    }
}

/// 这一行会不会**开一个** HTML 块。
///
/// 认的是「`<` + 标签名」，且标签名之后必须是空白、`>`、`/` 或行尾 —— 少了最后
/// 那个条件，`<https://example.com>` 这种自动链接会被当成标签（`https` 后面跟的是
/// `:`），于是一段本该正常解析的正文被当成了 HTML 块。
fn starts_html_block(line: &str) -> bool {
    let Some(rest) = line.strip_prefix('<') else {
        return false;
    };
    // 闭合标签、注释、声明、处理指令
    match rest.chars().next() {
        Some(ch) if ch == '/' || ch == '!' || ch == '?' => return true,
        Some(ch) if ch.is_ascii_alphabetic() => {}
        _ => return false,
    }
    let mut name_len = 1;
    for (at, ch) in rest.char_indices().skip(1) {
        if !(ch.is_ascii_alphanumeric() || ch == '-') {
            break;
        }
        name_len = at + ch.len_utf8();
    }
    matches!(
        rest[name_len..].chars().next(),
        None | Some(' ') | Some('\t') | Some('>') | Some('/')
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(lines: &[&str]) -> Vec<FenceLine> {
        let mut fences = Fences::default();
        lines.iter().map(|line| fences.feed(line)).collect()
    }

    fn raw_trace(lines: &[&str]) -> Vec<bool> {
        let mut raw = Raw::default();
        lines.iter().map(|line| raw.feed(line)).collect()
    }

    /// 一道围栏的基本走位：开、内容、关，关了之后的行回到「外面」
    #[test]
    fn a_fence_opens_and_closes() {
        assert_eq!(
            trace(&["```", "内容", "```", "外面"]),
            [
                FenceLine::Delimiter,
                FenceLine::Inside,
                FenceLine::Delimiter,
                FenceLine::Outside
            ]
        );
    }

    /// 反引号与波浪线各归各 —— 里面的另一种记号既不开新围栏，也不关已开的那道
    #[test]
    fn the_two_markers_do_not_pair_up() {
        // 波浪围栏里的一道反引号，内容而已
        assert_eq!(
            trace(&["~~~", "```", "~~~", "外面"]),
            [
                FenceLine::Delimiter,
                FenceLine::Inside,
                FenceLine::Delimiter,
                FenceLine::Outside
            ]
        );
        // 反过来也一样：里面的另一种记号**不开**新的一道（围栏不嵌套），
        // 所以后面那行仍然在围栏里 —— 它没被关上过
        assert_eq!(
            trace(&["```", "~~~", "外面"]),
            [FenceLine::Delimiter, FenceLine::Inside, FenceLine::Inside]
        );
    }

    /// **闭合的那道不短于开启的那道** —— 短的那一行只是内容。
    ///
    /// 这条不写的话，围栏里三个反引号（常见的行内记号被拉到行首）会把围栏关掉，
    /// 后面所有内容都被当成了外面。
    #[test]
    fn a_shorter_run_does_not_close_the_fence() {
        // 用 4 个开启：里面那两道 3 个的都关不上
        assert_eq!(
            trace(&["````", "```", "```", "外面"]),
            [
                FenceLine::Delimiter,
                FenceLine::Inside,
                FenceLine::Inside,
                FenceLine::Inside
            ]
        );
        // 而一道**不短于**开启时的就闭合
        assert_eq!(
            trace(&["````", "`````", "外面"]),
            [
                FenceLine::Delimiter,
                FenceLine::Delimiter,
                FenceLine::Outside
            ]
        );
        // 开启时 3 个，后面 3 个 —— 正好齐平，闭合
        assert_eq!(
            trace(&["```", "a", "```", "外面"]),
            [
                FenceLine::Delimiter,
                FenceLine::Inside,
                FenceLine::Delimiter,
                FenceLine::Outside
            ]
        );
    }

    /// 不到三个记号的行不是分隔行 —— 一两个反引号是正文里的记号，别把后面整段吞进围栏
    #[test]
    fn fewer_than_three_is_never_a_delimiter() {
        assert_eq!(
            trace(&["``", "`", "~~", "外面"]),
            [
                FenceLine::Outside,
                FenceLine::Outside,
                FenceLine::Outside,
                FenceLine::Outside
            ]
        );
    }

    /// 记号前面的空白不算数：缩进过的那道照样开、照样关
    #[test]
    fn leading_space_does_not_hide_the_delimiter() {
        let mut fences = Fences::default();
        assert_eq!(fences.feed("   ```rust"), FenceLine::Delimiter);
        assert!(fences.is_open());
        assert_eq!(fences.feed("   ```"), FenceLine::Delimiter);
        assert!(!fences.is_open());
    }

    /// 回到 `0` —— 这样它就能一直喂下去
    #[test]
    fn it_survives_a_whole_document() {
        let mut fences = Fences::default();
        let mut inside = 0;
        for line in ["```", "a", "b", "```", "外面", "```", "c", "```"] {
            if fences.feed(line) == FenceLine::Inside {
                inside += 1;
            }
        }
        assert_eq!(inside, 3);
        assert!(!fences.is_open(), "文档结束时不该还开着围栏");
    }

    // ---- 原始区（围栏 + HTML 块）----

    /// HTML 块吃到空行为止 —— 空行之后的行回到普通内容，不能一路吞下去
    #[test]
    fn an_html_block_runs_to_the_next_blank_line() {
        assert_eq!(
            raw_trace(&["<div>", "[x]", "</div>", "", "[y]"]),
            [true, true, true, false, false]
        );
    }

    /// 注释与声明也开 HTML 块 —— 不只有 `<标签` 那一种开头
    #[test]
    fn a_comment_or_declaration_also_starts_one() {
        assert_eq!(raw_trace(&["<!-- 注 -->", "[x]"]), [true, true]);
        assert_eq!(raw_trace(&["<?php ...", "[x]"]), [true, true]);
    }

    /// **自动链接不是标签。** 少了这条，`<https://example.com>` 那类行会被当成标签
    /// （`https` 后面跟的是 `:`），于是后面一整段本该正常解析的正文都被吞成
    /// 「HTML 块里的内容」。
    #[test]
    fn an_autolink_is_not_an_html_block() {
        assert!(!starts_html_block("<https://example.com>"));
        assert!(!starts_html_block("<a@b.com>"));
        assert!(starts_html_block("<div>"));
        assert!(starts_html_block("</div>"));
        assert!(starts_html_block("<my-widget class='x'>"));
        assert!(starts_html_block("<br/>"));
        // 缩进更深的不是行首 —— 与标签那条「从行首开始」是同一个道理
        assert!(!starts_html_block("  <div>"));
    }

    /// 围栏里出现 HTML 记号也不开 HTML 块 —— 围栏不嵌套，两套状态互不相干
    #[test]
    fn a_fence_swallows_html_looking_lines() {
        let mut raw = Raw::default();
        assert!(raw.feed("```html"), "分隔行");
        assert!(raw.feed("<div>"), "围栏里，这一行只是内容");
        assert!(raw.feed("```"), "闭合分隔行");
        // 围栏关了 —— 但 `<div>` 确实会**另开**一个 HTML 块，所以这一行仍是原始区。
        // 关键是没有把状态带进围栏之前：`</div>` 之后到空行为止是一整块。
        assert!(raw.feed("<div>"));
        assert!(raw.feed("</div>"));
        assert!(raw.feed("[x]"));
        // 空行结束 HTML 块，之后回到普通内容
        assert!(!raw.feed(""));
        assert!(!raw.feed("[y]"), "HTML 块关了，这一行该回到普通内容");
    }
}

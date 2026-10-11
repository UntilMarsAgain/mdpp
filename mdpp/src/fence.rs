//! 围栏代码块的开关：`` ``` `` 与 `~~~`。
//!
//! 「这一行在围栏这件事上是什么身份」有两个地方要问，且问法不同：
//!
//! - [`crate::targets`] 问的是「这行的字能不能动」—— 围栏**里**的不能动，围栏的
//!   **分隔行本身**也不能动，两者都要照搬；
//! - [`crate::block::scanner`] 问的是「这一行是不是分隔行」—— 是的话它上面的内容
//!   仍然只是围栏里的内容（代码片段里的 `[标签]` 是给人看的例子，不是标签）。
//!
//! 两个问题共用一个状态机，是因为**两处各写一份判定的话，两套规则迟早会对不上** ——
//! 而「对不上」的那种表现是静悄悄的：代码里的一句 `[x]` 忽然变成了一个选项卡。
//!
//! 判定照 CommonMark 的两条：连续三个以上算一道围栏；**闭合的那道不短于开启的那道**，
//! 短的那一行只是围栏里的内容。

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

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(lines: &[&str]) -> Vec<FenceLine> {
        let mut fences = Fences::default();
        lines.iter().map(|line| fences.feed(line)).collect()
    }

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
}

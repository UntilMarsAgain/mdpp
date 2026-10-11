//! 导航框：`::navbox` —— 页面底部那一张「左标签、右内容」的竖向分栏表。
//!
//! 形状与 [`super::text`] 里的 `::fields` 同源（那一块也是一张两列的表：左边标签、
//! 右边内容），差别是分栏的内容按 markdown 解析（能写链接、列表与嵌套块），而且
//! 多了标题栏与折叠。
//!
//! ```text
//! ::navbox title=求问百科 state=collapsed
//!   [above]
//!   何为方针 · 方针列表
//!
//!   [header1]
//!   基本原则
//!
//!   [基本原则]
//!   求问百科是**百科全书** · 中国价值观基础上的客观观点
//!
//!   [收录标准]
//!   通用收录标准 · 人物 · 组织 · 网站
//!
//!   [header2]
//!   内容方针
//!
//!   [基本原则]
//!   可供查证 · 可靠来源 · 作者传记
//! ```
//!
//! ## `[标签]` 就是分组名
//!
//! `[基本原则]` 里的那四个字**既是节的开头，也是这一行的分组标题** —— 不再另配一个
//! `group1=` 之类的参数。维基那边要 `|group1=` 与 `|list1=` 两个参数配一对，纯粹是
//! 模板参数装不下多行内容；这里 `[标签]` 节本来就装得下。
//!
//! 分组名**可以重复**：同一个标签在不同的 `[headerN]` 下面各写一次是正常的
//! （「基本原则 / 收录标准 / 格式手册」那一组在每个分组下重复，正是维基那些「方针类」
//! 导航框的样子）。标签是**显示用的字**，不是标识，所以重复不构成冲突。
//!
//! ## `[link 页面]`：分组名指向一条内部链接
//!
//! ```text
//! [link 桥梁]           → 分组名是一条指向「桥梁」的链接
//! [link 道路|路这一类]   → 同上，显示的文字换成「路这一类」
//! ```
//!
//! **不做 `[分组名 [[链接]]]` 那种写法**，理由见 [`group_label_html`]。
//!
//! ## 三个颜色参数
//!
//! `group-color=` 每一栏标题的字色、`title-color=` 大标题的字色、`header-bg=`
//! 通栏底色；都只认 `#rgb` / `#rrggbb`，与 `::banner` 的 `color=` 同一条规矩。
//! 它们落成**自定义属性**（`--navbox-group-fg` / `--navbox-title-fg` /
//! `--navbox-head-bg`），样式表那几处各自带默认值，所以「不给」与「给了」走的
//! 是同一段 CSS。
//!
//! ## 三处保留的名字
//!
//! - `[above]` / `[below]` —— 维基的那两行通栏，位置固定：一条紧贴标题栏之下，
//!   一条在最底下；
//! - `[headerN]` —— **通栏标题条**，落在作者写它的那两行之间（维基的 `|headern=`）。
//!
//! 这三个名字因此当不了分组名 —— 要写一个叫「above」的分组，写 `[header1]` 再把它
//! 摆在对的地方，或者换个叫法。
//!
//! ## 标题栏与折叠
//!
//! `title=` 有值才有标题栏；**没有 `title=` 就是嵌进去的样子**（没有标题栏、没有开关），
//! 那正是写子 group 的方式：
//!
//! ```text
//! ::navbox title=桥的构件
//!   [上部结构]
//!   * 桥面
//!
//!   ::navbox
//!     [桥墩]
//!     * 石墩
//! ```
//!
//! 子导航框要与那一节的内容**同一缩进**，且**前面空一行** —— 不空行的话那一行会被
//! markdown 当成列表项的续行，而块规则要独占一行，于是整个子导航框变成一段文字。
//!
//! 折叠只吐结构（`data-state` + `hidden`），开合由使用方接上，理由与 `::tabs` 相同：
//! 渲染器塞不了 `onclick`，塞了也过不了 CSP。**开关是标题栏右边那个写着字的小按钮**，
//! 不是标题栏本身 —— 点标题去开合是个不明确的交互。
//!
//! ## 样式一概不做
//!
//! 维基那几十个 `titlestyle` / `oddstyle` 一个都不实现 —— 它们要写进 `style` 属性，
//! 宽松了就等于允许塞任意声明，而 `::banner` 只给 `color=` 就是同一个道理。要改样子
//! 用 [`::css`](crate::block::stdlib)。

use std::collections::BTreeMap;

use super::super::dispatch::{render_problem, sections};
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// 按钮上那两个字。渲染器给**初始**的字，之后由脚本换。
const SHOW: &str = "显示";
const HIDE: &str = "隐藏";

/// 一个 `[标签]` 的意思。
enum Slot {
    /// `[above]` —— 紧贴标题栏之下那一行通栏
    Above,
    /// `[below]` —— 最底下那一行通栏
    Below,
    /// `[headerN]` —— 落在作者写它那两行之间的通栏标题条
    Header(usize),
    /// 别的字就是**分组名**，它同时也是这一节的标题
    Group(String),
}

/// 主体里的一段。**按写下来的次序**排 —— 通栏要落在它该落的那几行之间，而那个
/// 位置只有次序说得清。
///
/// `Group` 带的是分组名在 [`render_navbox`] 那个 `groups` 里的**下标**，而不是引用 ——
/// 标签自己那份字符串活不过认标签那一轮循环，而行标签要一直用到渲染完。
enum Segment<'a> {
    /// 一条通栏
    Header(&'a Node),
    /// 一行分组：`(分组名的下标, 这一行的内容)`
    Group(usize, &'a Node),
}

/// 折起来的状态。三档，与 `::theme` 认 `light`/`dark` 是同一个做法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// 展开着，且可以折起来（默认）
    Expanded,
    /// 一上来就折着
    Collapsed,
    /// 展开着，且**不能**折（没有那个按钮）
    Plain,
}

impl State {
    /// 落进 `data-state` 的那三个字
    fn attr(self) -> &'static str {
        match self {
            State::Expanded => "expanded",
            State::Collapsed => "collapsed",
            State::Plain => "plain",
        }
    }
}

/// `state=` 的三种写法 + 「没写」这一种。
///
/// 错的值**整块拒掉**：默认落到 expanded 与作者写 `plain` 的意思正好相反，而那一档
/// 的差别是作者看得见的。
fn state_of(block: &Block) -> Result<State, String> {
    match block
        .param("state")
        .map(str::trim)
        .map(str::to_lowercase)
        .as_deref()
    {
        None | Some("") | Some("expanded") => Ok(State::Expanded),
        Some("collapsed") => Ok(State::Collapsed),
        Some("plain") | Some("off") => Ok(State::Plain),
        Some(other) => Err(format!(
            "state= 只认 expanded（展开）、collapsed（一上来折着）、plain（展开但不能折）\
             这三个，这一个写的是 {other}"
        )),
    }
}

/// 认一个 `[标签]`。
///
/// 除了 `above` / `below` / `headerN`，**其余一律当分组名** —— 不去猜「像不像编号」
/// 之类的东西，所以写 `[2024 年的条目]` 也是个合法的分组名。
fn slot_of(label: &str) -> Result<Slot, String> {
    match label {
        "above" => return Ok(Slot::Above),
        "below" => return Ok(Slot::Below),
        _ => {}
    }
    // 只有 `header` 开头才按编号认。`header` 单独一个（没有数字）是个写得不对的
    // 通栏 —— 说清它要什么，而不是当成一个叫「header」的分组。
    if let Some(rest) = label.strip_prefix("header") {
        return match rest.parse::<usize>() {
            Ok(index) if index > 0 => Ok(Slot::Header(index)),
            _ => Err(format!(
                "[{label}] 不认：通栏要带一个 1 起的编号，比如 [header1]、\
                 写成一个分组的话换个名字就好"
            )),
        };
    }
    Ok(Slot::Group(label.to_string()))
}

/// 按 markdown 渲染一小段，但**剥掉最外层那个 `<p>`**。
///
/// 参数走的是整块解析，所以 `求问百科` 出来是 `<p>求问百科</p>`；而标题栏与按钮里要
/// 的是 `求问百科` 那一段 —— `<button>` 里塞 `<p>` 既不合法也不好看。
fn inline_html(text: &str) -> String {
    let rendered = crate::engine::render_nested(text);
    let trimmed = rendered.trim();
    match trimmed
        .strip_prefix("<p>")
        .and_then(|rest| rest.strip_suffix("</p>"))
    {
        Some(inner) if !inner.contains("</p>") => inner.to_string(),
        _ => trimmed.to_string(),
    }
}

/// 分组名那一格里写什么。
///
/// **`[link 页面]`** 让分组名变成一条指向内部链接的链。走的是 markdown 的
/// `[[…]]`（交给 `inline_html` 再渲染一次），所以**链接解析器是同一个**：递了解析器
/// 就出 `href`，没递或解析器说没有就落成红链 —— 与正文里的 `[[…]]` 一模一样。
///
/// 为什么不给 `[分组名 [[链接]]]` 那种写法：分节那一步只认「整行一对方括号」
/// （见 `block::scanner` 的 `section_label`），而它为了不把 `[[甲]]` 认成标签，
/// 会把**任何含方括号**的标签都退回去。放开它就得去数嵌套的方括号——而一个写错
/// 半边的 `[a [b]` 会让整个分节错位，页面从此莫名其妙。分开写没这个风险。
fn group_label_html(label: &str) -> Result<String, String> {
    let Some(rest) = label.strip_prefix("link") else {
        return Ok(inline_html(label));
    };
    // `linkage` / `linked` 那些是普通分组名，不是链接写法
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return Ok(inline_html(label));
    }
    let page = rest.trim();
    if page.is_empty() {
        return Err(format!(
            "[{label}] 后面没写页面名：写 [link 某页]，也可以写 [link 某页|显示的字]"
        ));
    }
    Ok(inline_html(&format!("[[{page}]]")))
}

/// 那三个颜色参数。只认 `#rgb` / `#rrggbb` —— 与 [`::banner`](super::text) 的
/// `color=` 是同一条规矩：值要写进 `style` 属性，宽松了就等于允许往里塞任意声明。
struct Palette {
    /// `group-color=` —— 每一栏标题的字色
    group_fg: Option<String>,
    /// `title-color=` —— 大标题的字色
    title_fg: Option<String>,
    /// `header-bg=` —— 通栏的底色
    head_bg: Option<String>,
}

impl Palette {
    /// 落成一条 `style`。一个都没给时不写这个属性 —— 多余的 `style=""` 是噪音。
    fn style(&self) -> Option<String> {
        let mut out = String::new();
        for (name, value) in [
            ("--navbox-group-fg", &self.group_fg),
            ("--navbox-title-fg", &self.title_fg),
            ("--navbox-head-bg", &self.head_bg),
        ] {
            if let Some(value) = value {
                out.push_str(&format!("{name}: {value};"));
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

/// 认那三个颜色参数。错的值**整块拒掉** —— 与 `size_rule` / `normalize_hex` 的
/// 用法一致：渲染一半才发现颜色非法，那一半已经是错的了。
fn palette_of(block: &Block) -> Result<Palette, String> {
    Ok(Palette {
        group_fg: hex_of(block, "group-color")?,
        title_fg: hex_of(block, "title-color")?,
        head_bg: hex_of(block, "header-bg")?,
    })
}

/// 取一个颜色参数，认不出就说明它该长什么样
fn hex_of(block: &Block, key: &str) -> Result<Option<String>, String> {
    match block.param(key) {
        None => Ok(None),
        Some(value) => super::normalize_hex(value)
            .map(Some)
            .ok_or_else(|| format!("{key}= 只认 #rgb 或 #rrggbb 这两种写法，比如 {key}=#0055a4")),
    }
}

/// `::navbox` —— 见模块头。
pub(super) fn render_navbox(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    // 没有 `title=` 就是**嵌进去的样子**：没有标题栏、没有边框、没有开关。
    let title = block
        .param("title")
        .map(str::trim)
        .filter(|t| !t.is_empty());

    let state = match state_of(block) {
        Ok(state) => state,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };
    // 嵌进去的没有开关，写 `state=` 是白写 —— 说出来，好过默默忽略
    if title.is_none() && block.has("state") {
        render_problem(
            block,
            fmt,
            "没有 title= 就是嵌进去的样子，没有开关，所以 state= 在这儿没用：\
             写 title= 才是一个完整的导航框",
        );
        return;
    }

    let palette = match palette_of(block) {
        Ok(palette) => palette,
        Err(why) => {
            render_problem(block, fmt, &why);
            return;
        }
    };

    // 先**整块**认一遍再渲染：标签写错了就整块报出来，免得渲染半截
    let mut above: Option<&Node> = None;
    let mut below: Option<&Node> = None;
    let mut headers: BTreeMap<usize, &Node> = BTreeMap::new();
    let mut segments: Vec<Segment> = Vec::new();
    // 分组名的活字放这儿：`sections(node)` 给的那个 `&str` 活不过循环，而行标签要
    // 一直用到渲染完
    let mut groups: Vec<String> = Vec::new();
    for (label, section) in sections(node) {
        if label.is_empty() {
            render_problem(
                block,
                fmt,
                "有内容写在第一个 [标签] 之前：导航框的每一节都要以 [标签] 开头",
            );
            return;
        }
        let slot = match slot_of(label) {
            Ok(slot) => slot,
            Err(why) => {
                render_problem(block, fmt, &why);
                return;
            }
        };
        match slot {
            Slot::Above => {
                if above.replace(section).is_some() {
                    render_problem(block, fmt, "`[above]` 写了两处：一个导航框里它只能有一处");
                    return;
                }
            }
            Slot::Below => {
                if below.replace(section).is_some() {
                    render_problem(block, fmt, "`[below]` 写了两处：一个导航框里它只能有一处");
                    return;
                }
            }
            Slot::Header(index) => {
                if headers.insert(index, section).is_some() {
                    render_problem(
                        block,
                        fmt,
                        &format!("`[header{index}]` 写了两处：一个编号只能有一条"),
                    );
                    return;
                }
                // 通栏就是它的标题，空着等于没有标题 —— 不如报错，也不该画一条空条
                if section.children.is_empty() {
                    render_problem(
                        block,
                        fmt,
                        &format!("`[header{index}]` 是空的：通栏就是它的标题"),
                    );
                    return;
                }
                segments.push(Segment::Header(section));
            }
            // 分组名**可以重复**：它是显示用的字，不是标识
            Slot::Group(name) => {
                groups.push(name);
                segments.push(Segment::Group(groups.len() - 1, section));
            }
        }
    }

    let rows = segments
        .iter()
        .filter(|segment| matches!(segment, Segment::Group(..)))
        .count();
    if rows == 0 {
        render_problem(
            block,
            fmt,
            "导航框里一行也没有：写一个 [分组名]，下面写那一行的内容",
        );
        return;
    }

    let collapsed = state == State::Collapsed;

    let mut class = "navbox".to_string();
    if title.is_none() {
        class.push_str(" navbox--nested");
    }
    let mut root: Vec<(&str, String)> =
        vec![("class", class), ("data-state", state.attr().to_string())];
    // 颜色落成**自定义属性**而不是直接给元素上色：样式表那几处各自带默认值
    // （`var(--navbox-group-fg, var(--mdpp-text-dim))`），于是「不给」与「给了」
    // 走的是同一段 CSS，不必在这里分叉
    if let Some(style) = palette.style() {
        root.push(("style", style));
    }
    fmt.cr();
    fmt.open("div", &root);

    // 标题栏：标题是标题，开关是**另一个**写着字的小按钮
    if let Some(title) = title {
        fmt.cr();
        fmt.open("div", &[("class", "navbox__bar".to_string())]);
        fmt.open("span", &[("class", "navbox__title".to_string())]);
        fmt.text_raw(&inline_html(title));
        fmt.close("span");
        if state != State::Plain {
            fmt.cr();
            fmt.open(
                "button",
                &[
                    ("class", "navbox__toggle".to_string()),
                    ("type", "button".to_string()),
                    (
                        "aria-expanded",
                        if collapsed { "false" } else { "true" }.to_string(),
                    ),
                    // 按钮上写的是**点了会发生什么**，不是「现在什么样」。那两个字由
                    // 渲染器落在这儿，脚本换字时从这儿取 —— 中文不在脚本里硬编码
                    ("data-label-show", SHOW.to_string()),
                    ("data-label-hide", HIDE.to_string()),
                ],
            );
            fmt.text(if collapsed { SHOW } else { HIDE });
            fmt.close("button");
        }
        fmt.cr();
        fmt.close("div");
    }

    // 折起来时用 `hidden` 而不是 CSS —— 打印规则要能把它放出来（见
    // `mdpp-cli/assets/print.css`），而 `display: none` 打印时救不回来
    let mut body: Vec<(&str, String)> = vec![("class", "navbox__body".to_string())];
    if collapsed {
        body.push(("hidden", "hidden".to_string()));
    }
    fmt.cr();
    fmt.open("div", &body);
    fmt.cr();

    // 一张两列的表：左边分组名、右边内容 —— 与 `::fields` 同一个骨架
    fmt.open("table", &[("class", "navbox__table".to_string())]);
    fmt.open("tbody", &[]);

    if let Some(section) = above {
        full_row(fmt, "navbox__above", section);
    }

    for segment in &segments {
        match segment {
            // 通栏横跨两列
            Segment::Header(section) => full_row(fmt, "navbox__head", section),
            // 分组名就是标签本身
            Segment::Group(name, section) => {
                let label = match group_label_html(&groups[*name]) {
                    Ok(label) => label,
                    Err(why) => {
                        render_problem(block, fmt, &why);
                        return;
                    }
                };
                fmt.cr();
                fmt.open("tr", &[]);
                fmt.open(
                    "th",
                    &[
                        ("class", "navbox__group".to_string()),
                        // `th` 是这一行的表头，屏幕阅读器据此把单元格与行标签对上
                        ("scope", "row".to_string()),
                    ],
                );
                fmt.text_raw(&label);
                fmt.close("th");
                fmt.open("td", &[("class", "navbox__cell".to_string())]);
                fmt.contents(&section.children);
                fmt.close("td");
                fmt.close("tr");
            }
        }
    }

    if let Some(section) = below {
        full_row(fmt, "navbox__below", section);
    }

    fmt.cr();
    fmt.close("tbody");
    fmt.cr();
    fmt.close("table");

    fmt.cr();
    fmt.close("div");
    fmt.cr();
    fmt.close("div");
    fmt.cr();
}

/// 一条横跨两列的通栏（`[above]` / `[below]` / `[headerN]`）。
///
/// 用 `<td>` 而不是 `<th>`：这几行是**内容**（「何为方针 · 方针列表」是一句话），
/// 而这一列的表头另有其人（左边那个分组名）。`[headerN]` 是「下面几行的分组名」，
/// 但 HTML 里没有「横跨一行的行组」这种表头，用 `th` 反而会让读屏软件把整行念成一个
/// 单元格标题 —— 那是好事，不过 CSS 上要另给一份样式，不如统一当内容处理。
fn full_row(fmt: &mut dyn Renderer, class: &str, section: &Node) {
    fmt.cr();
    fmt.open("tr", &[]);
    fmt.open(
        "td",
        &[("class", class.to_string()), ("colspan", "2".to_string())],
    );
    fmt.contents(&section.children);
    fmt.close("td");
    fmt.close("tr");
}

#[cfg(test)]
mod tests {
    use crate::engine::Mdpp;

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 一张两列的表：左边分组名、右边内容 —— 与 `::fields` 同一个骨架
    #[test]
    fn it_is_a_two_column_table_like_fields() {
        let html = render("::navbox title=桥梁\n  [分类]\n  * [[拱桥]]\n");
        assert!(html.contains(r#"<table class="navbox__table">"#), "{html}");
        assert!(
            html.contains(r#"<th class="navbox__group" scope="row">分类</th>"#),
            "{html}"
        );
        assert!(html.contains(r#"<td class="navbox__cell">"#), "{html}");
        assert!(html.contains("<li>"), "{html}");
    }

    /// **`[标签]` 就是分组名** —— 不另配参数
    #[test]
    fn the_label_is_the_group_title() {
        let html = render("::navbox title=桥梁\n  [收录标准]\n  甲\n");
        assert!(html.contains("收录标准"), "{html}");
        // 组名**不**从参数里来
        assert!(
            html.contains(r#"<th class="navbox__group" scope="row">"#),
            "{html}"
        );
        assert!(!html.contains("template__param"), "{html}");
    }

    /// 分组名可以**乱写**：不像编号也不像保留字，就是显示用的那几个字
    #[test]
    fn a_group_name_is_whatever_the_author_wrote() {
        let html = render("::navbox title=x\n  [2024 年的条目]\n  甲\n  [B 组 · 补充]\n  乙\n");
        assert!(!html.contains("template--problem"), "{html}");
        assert!(html.contains("2024 年的条目"), "{html}");
        assert!(html.contains("B 组 · 补充"), "{html}");
    }

    /// 分组名**可以重复** —— 它是显示用的字，不是标识。维基那些「方针类」导航框
    /// 就是同一个标签在每个 `[headerN]` 下面各写一次。
    #[test]
    fn a_group_name_may_repeat_under_each_header() {
        let html = render(
            "::navbox title=x\n  [header1]\n  基本原则\n  [基本原则]\n  甲\n  [header2]\n  内容方针\n  [基本原则]\n  乙\n",
        );
        assert!(!html.contains("template--problem"), "{html}");
        assert_eq!(html.matches("navbox__head").count(), 2, "{html}");
        // 数的是**分组行**上的那个标签（通栏里也写着「基本原则」，不能混进来）
        assert_eq!(
            html.matches(r#"<th class="navbox__group" scope="row">基本原则</th>"#)
                .count(),
            2,
            "该出现两次：{html}"
        );
        assert!(html.contains("甲") && html.contains("乙"), "{html}");
    }

    /// `[headerN]` 是一条横跨两列的通栏，落在作者写它的那两行之间
    #[test]
    fn a_header_is_a_full_width_row_between_them() {
        let html = render(
            "::navbox title=求问百科\n  [header1]\n  基本原则\n  [甲组]\n  甲\n  [乙组]\n  乙\n",
        );
        assert!(html.contains(r#"colspan="2""#), "{html}");
        assert!(html.contains("基本原则"), "{html}");
        // 通栏横跨两列，里面**没有**分组名也没有列表 —— 按 `</tr>` 切出那一行来断，
        // 固定长度的窗口会溢出到下一行去（那一行有分组名，于是误判）
        let row = |class: &str| -> String {
            let from = html.find(class).unwrap_or_else(|| panic!("{class} 该在"));
            let rest = &html[from..];
            let to = rest.find("</tr>").expect("行要有个头");
            format!("{}{}", class, &rest[..to])
        };
        let head = row("navbox__head");
        assert!(!head.contains("navbox__group"), "通栏不该带分组名：{head}");
        assert!(!head.contains("<li>"), "通栏不该带列表：{head}");
        assert!(head.contains("基本原则"), "{head}");

        // 次序：通栏真的在第一行之前
        assert!(
            html.find("基本原则").unwrap() < html.find("甲组").unwrap(),
            "{html}"
        );
    }

    /// 通栏编号要 1 起；`[header]` 光秃秃一个要说得清要什么
    #[test]
    fn a_header_needs_a_number() {
        for bad in ["header", "header0", "headerX"] {
            let html = render(&format!("::navbox title=x\n  [{bad}]\n  甲\n"));
            assert!(html.contains("template--problem"), "{bad} -> {html}");
            assert!(html.contains("1 起的编号"), "{html}");
        }
        let ok = render("::navbox title=x\n  [header1]\n  甲\n  [分组]\n  乙\n");
        assert!(!ok.contains("template--problem"), "{ok}");
    }

    /// 通栏重号是冲突；空通栏也报错（它就是它的标题，没有标题等于没有）
    #[test]
    fn a_repeated_or_empty_header_is_refused() {
        let twice =
            render("::navbox title=x\n  [header1]\n  甲\n  [分组]\n  乙\n  [header1]\n  丙\n");
        assert!(twice.contains("template--problem"), "{twice}");
        assert!(twice.contains("写了两处"), "{twice}");

        let empty = render("::navbox title=x\n  [header1]\n  [分组]\n  甲\n");
        assert!(empty.contains("template--problem"), "{empty}");
        assert!(empty.contains("是空的"), "{empty}");
        assert!(!empty.contains("navbox__table"), "不渲染半截：{empty}");
    }

    /// `[above]` 紧贴标题栏之下，`[below]` 在最底下 —— 位置是**定的**，不由作者写在哪决定
    #[test]
    fn above_sits_at_the_top_and_below_at_the_bottom() {
        let html = render(
            "::navbox title=桥梁\n  [above]\n  上面那句\n  [分类]\n  甲\n  [below]\n  下面那句\n",
        );
        let top = html.find("上面那句").expect("above");
        let group = html.find(">分类<").expect("分组");
        let bottom = html.find("下面那句").expect("below");
        assert!(top < group && group < bottom, "{html}");
        assert!(html.contains(r#"class="navbox__above""#), "{html}");
        assert!(html.contains(r#"class="navbox__below""#), "{html}");
        // 两处是内容不是表头
        assert_eq!(html.matches("navbox__above").count(), 1, "{html}");
    }

    /// `[above]` / `[below]` 各只能有一处
    #[test]
    fn above_and_below_may_appear_once() {
        for (label, other) in [("above", "below"), ("below", "above")] {
            let html = render(&format!(
                "::navbox title=x\n  [{label}]\n  甲\n  [分组]\n  乙\n  [{label}]\n  丙\n"
            ));
            assert!(html.contains("template--problem"), "{label} -> {html}");
            assert!(html.contains("写了两处"), "{html}");
            let _ = other;
        }
    }

    /// **没有 `title=` 就是嵌进去的样子**：没有标题栏、没有边框、没有开关
    #[test]
    fn no_title_means_nested() {
        let html = render("::navbox\n  [桥墩]\n  * 甲\n");
        assert!(html.contains("navbox--nested"), "{html}");
        assert!(!html.contains("navbox__bar"), "没有标题栏：{html}");
        assert!(!html.contains("navbox__toggle"), "{html}");
        assert!(
            html.contains(r#"<th class="navbox__group" scope="row">桥墩</th>"#),
            "{html}"
        );
    }

    /// 嵌进去的没有开关，写 `state=` 是白写 —— 说出来
    #[test]
    fn state_on_a_nested_navbox_is_pointless() {
        let html = render("::navbox state=collapsed\n  [分组]\n  甲\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("state="), "{html}");
    }

    /// 每个分组里可以嵌一个 `::navbox` 当子 group
    #[test]
    fn a_group_can_hold_a_nested_navbox() {
        let html = render(
            "::navbox title=桥的构件\n  [上部结构]\n  * 桥面\n\n  ::navbox\n    [桥墩]\n    * 石墩\n    * 木墩\n",
        );
        assert!(!html.contains("template--problem"), "{html}");
        assert_eq!(html.matches("navbox--nested").count(), 1, "{html}");
        // 子导航框落在**父导航框的内容格子里**
        assert!(html.contains("桥面"), "{html}");
        assert!(html.contains("石墩") && html.contains("木墩"), "{html}");
        assert!(html.contains("桥墩"), "{html}");
        // 父导航框只有自己的标题栏
        assert_eq!(html.matches("navbox__bar").count(), 1, "{html}");
    }

    /// 嵌套**必须空一行**，否则会被 markdown 当成列表项的续行。
    ///
    /// 不空行时那一行连同后面几行一起变成 `<li>` 里的普通文字 —— 子导航框整个消失，
    /// 页面上只看得见一句 `::navbox`。
    #[test]
    fn nesting_without_a_blank_line_swallows_the_block() {
        let swallowed =
            render("::navbox title=外层\n  [组]\n  * 甲\n  ::navbox\n    [子组]\n    * 乙\n");
        assert!(
            !swallowed.contains("navbox--nested"),
            "不空行时子导航框整个被列表吞成文字了：{swallowed}"
        );
        assert!(
            swallowed.contains("::navbox"),
            "原文成了列表项里的字：{swallowed}"
        );

        let separate =
            render("::navbox title=外层\n  [组]\n  * 甲\n\n  ::navbox\n    [子组]\n    * 乙\n");
        assert_eq!(separate.matches("navbox--nested").count(), 1, "{separate}");
    }

    /// 折叠：`state=collapsed` 落成 `hidden` + `aria-expanded="false"`，**不给 onclick**
    #[test]
    fn collapse_is_structure_not_behaviour() {
        let collapsed = render("::navbox title=桥梁 state=collapsed\n  [分类]\n  * 甲\n");
        assert!(
            collapsed.contains(r#"data-state="collapsed""#),
            "{collapsed}"
        );
        assert!(
            collapsed.contains(r#"aria-expanded="false""#),
            "{collapsed}"
        );
        assert!(
            collapsed.contains(r#"<div class="navbox__body" hidden="hidden">"#),
            "{collapsed}"
        );
        // 按钮上写的是「点了会发生什么」：折着时写「显示」
        assert!(collapsed.contains(">显示</button>"), "{collapsed}");

        let expanded = render("::navbox title=桥梁\n  [分类]\n  * 甲\n");
        assert!(expanded.contains(r#"aria-expanded="true""#), "{expanded}");
        assert!(expanded.contains(">隐藏</button>"), "{expanded}");
        assert!(!expanded.contains("hidden"), "{expanded}");
        assert!(!expanded.contains("onclick"), "点击由使用方接：{expanded}");
    }

    /// 按钮上带着两个词，脚本换的时候不必把中文硬编码进 JS
    #[test]
    fn the_button_carries_both_labels() {
        let html = render("::navbox title=桥梁\n  [分类]\n  * 甲\n");
        assert!(html.contains(r#"data-label-show="显示""#), "{html}");
        assert!(html.contains(r#"data-label-hide="隐藏""#), "{html}");
    }

    /// `state=plain`：展开且没有开关
    #[test]
    fn plain_has_no_toggle() {
        let html = render("::navbox title=桥梁 state=plain\n  [分类]\n  * 甲\n");
        assert!(html.contains(r#"data-state="plain""#), "{html}");
        assert!(!html.contains("navbox__toggle"), "{html}");
        assert!(html.contains("桥梁"), "标题仍在：{html}");
    }

    /// `state=` 写错就整块拒掉
    #[test]
    fn a_bad_state_is_refused() {
        let html = render("::navbox title=x state=autocollapse\n  [分类]\n  * 甲\n");
        assert!(html.contains("template--problem"), "{html}");
        for known in ["expanded", "collapsed", "plain"] {
            assert!(html.contains(known), "提示里该说得出 {known}：{html}");
        }
        assert!(!html.contains(r#"class="navbox""#), "不渲染半截：{html}");
    }

    /// **`[link 页面]` 让分组名变成一条内部链接。**
    ///
    /// 走的是 markdown 的 `[[…]]`，所以**链接解析器是同一个**：递了解析器就出 `href`，
    /// 没递就落成红链 —— 与正文里的 `[[…]]` 一模一样。
    #[test]
    fn a_group_label_can_be_an_internal_link() {
        use crate::RenderContext;
        use crate::link::{LinkResolver, LinkTarget, ResolvedLink};
        use std::sync::Arc;

        struct Vault;
        impl LinkResolver for Vault {
            fn resolve(&self, target: &str) -> Option<ResolvedLink> {
                Some(ResolvedLink::Link(LinkTarget::new(format!(
                    "/notes/{target}"
                ))))
            }
        }
        let html = Mdpp::new().render_with(
            "::navbox title=x\n  [link 桥梁]\n  内容\n",
            &RenderContext::new(),
        );
        // 标签格里是一条 `<a>`，不是文字
        let row = html.find("navbox__group").expect("要有标签");
        let row = &html[row..row + 200];
        assert!(row.contains("<a "), "分组名该是链接：{row}");
        assert!(row.contains(">桥梁</a>"), "{row}");

        // 递了解析器就出地址
        let resolver: Arc<dyn LinkResolver> = Arc::new(Vault);
        let with = Mdpp::new().render_with(
            "::navbox title=x\n  [link 桥梁]\n  内容\n",
            &RenderContext::new().with_links(resolver),
        );
        assert!(with.contains(r#"href="/notes/桥梁""#), "{with}");

        // `|` 那一半换显示文字
        let alias = Mdpp::new().render_with(
            "::navbox title=x\n  [link 桥梁|拱桥一览]\n  内容\n",
            &RenderContext::new(),
        );
        assert!(alias.contains(">拱桥一览</a>"), "{alias}");
        assert!(alias.contains(r#"data-doc="桥梁""#), "{alias}");
    }

    /// `[linkage]` 之类是**普通分组名** —— 不能因为开头那几个字母就当成链接写法
    #[test]
    fn only_the_bare_word_link_starts_a_link() {
        for label in ["linkage", "linked", "links"] {
            let html = render(&format!("::navbox title=x\n  [{label}]\n  甲\n"));
            assert!(!html.contains("template--problem"), "{label}: {html}");
            assert!(html.contains(label), "{label}");
            assert!(!html.contains("<a "), "{label} 不该是链接：{html}");
        }
    }

    /// `[link]` 没写页面名：说清要什么，而不是画一个空格子
    #[test]
    fn a_link_label_needs_a_page() {
        for label in ["link", "link "] {
            let html = render(&format!("::navbox title=x\n  [{label}]\n  甲\n"));
            assert!(html.contains("template--problem"), "{label:?} -> {html}");
            assert!(html.contains("没写页面名"), "{html}");
        }
    }

    /// **`[分组名 [[链接]]]` 那种写法不做**，而且是有理由的不做。
    ///
    /// 分节只认「整行一对方括号」，而它为了不把 `[[甲]]` 认成标签，把**任何含方括号**
    /// 的标签都退回去。放开就得去数嵌套方括号 —— 一个写错半边的 `[a [b]` 会让整个
    /// 分节错位。分开写没这个风险。
    #[test]
    fn brackets_inside_a_label_are_not_a_link() {
        let html = render("::navbox title=x\n  [分组 [[甲]]]\n  内容\n");
        // 不是链接，而是「标签写歪了」的那一类
        assert!(!html.contains(r#"<a href"#), "不该认成链接：{html}");
        assert!(
            html.contains("template--problem") || html.contains("分组"),
            "{html}"
        );
    }

    /// 三个颜色参数落成自定义属性，交给样式表去用
    #[test]
    fn the_palette_becomes_custom_properties() {
        let html = render(
            "::navbox title=x group-color=#0055a4 title-color=#ffe680 header-bg=#123456\n  [分类]\n  甲\n",
        );
        assert!(!html.contains("template--problem"), "{html}");
        for (name, value) in [
            ("--navbox-group-fg", "#0055a4"),
            ("--navbox-title-fg", "#ffe680"),
            ("--navbox-head-bg", "#123456"),
        ] {
            assert!(
                html.contains(&format!("{name}: {value};")),
                "少了 {name}：{html}"
            );
        }
        // `#rgb` 那种简写要规范成六位，与 `::banner` 同一套
        let short = render("::navbox title=x group-color=#0af\n  [分类]\n  甲\n");
        assert!(short.contains("--navbox-group-fg: #00aaff;"), "{short}");
    }

    /// 一个颜色都没给时**不写** `style` —— 多余的 `style=""` 是噪音
    #[test]
    fn no_colour_means_no_style_attribute() {
        let html = render("::navbox title=x\n  [分类]\n  甲\n");
        assert!(!html.contains("style="), "{html}");
        // 给了别的属性也不该冒出 style
        let nested = render("::navbox\n  [分类]\n  甲\n");
        assert!(!nested.contains("style="), "{nested}");
    }

    /// 颜色值只认十六进制 —— 它要写进 `style` 属性，宽松了就等于允许塞任意声明
    #[test]
    fn a_bad_colour_is_refused() {
        for (param, value) in [
            ("group-color", "red"),
            ("title-color", "#12345"),
            ("header-bg", "#fff; position: fixed"),
        ] {
            let html = render(&format!(
                "::navbox title=x {param}={value}\n  [分类]\n  甲\n"
            ));
            assert!(
                html.contains("template--problem"),
                "{param}={value}: {html}"
            );
            assert!(html.contains("#rgb"), "{html}");
            // 整块拒掉，不渲染半截
            assert!(!html.contains(r#"class="navbox""#), "{html}");
        }
    }

    /// 一个分组都没有
    #[test]
    fn an_empty_navbox_says_so() {
        let html = render("::navbox title=x\n");
        assert!(html.contains("template--problem"), "{html}");
        assert!(html.contains("一行也没有"), "{html}");

        // 只有通栏也不算数 —— 它自己撑不起一个框
        let headers = render("::navbox title=x\n  [header1]\n  只有通栏\n");
        assert!(headers.contains("template--problem"), "{headers}");
    }

    /// 第一个标签之前有内容
    #[test]
    fn content_before_the_first_slot_is_refused() {
        let html = render("::navbox title=x\n  开场白\n  [分类]\n  甲\n");
        assert!(html.contains("template--problem"), "{html}");
    }

    /// 标题按 markdown 渲染，但外层那个 `<p>` 剥掉
    #[test]
    fn the_title_is_markdown_without_the_wrapping_paragraph() {
        let html = render("::navbox title=**桥梁**\n  [分类]\n  甲\n");
        assert!(html.contains("<strong>桥梁</strong>"), "{html}");
        assert!(!html.contains("<p><strong>桥梁</strong></p>"), "{html}");
    }

    /// **列表横排是纯 CSS 的事**：渲染器吐的就是普通的 `<ul><li>`。
    ///
    /// 这条钉的是那条分工 —— 一旦渲染器开始为布局吐标记，结构与样式就绑死了，
    /// 而换一份样式表的人会立刻看到列表塌回竖排，输出里却没有任何东西能解释为什么。
    #[test]
    fn horizontality_is_pure_css() {
        let html = render("::navbox title=x\n  [分类]\n  * 甲\n  * 乙\n");
        assert!(html.contains("<ul>"), "{html}");
        assert_eq!(html.matches("<li>").count(), 2, "{html}");
        for marker in ["hlist", "horizontal", "inline"] {
            assert!(!html.contains(marker), "渲染器不该吐 {marker}：{html}");
        }
        // 分隔点只该出现在样式表里
        assert!(!html.contains('\u{b7}'), "分隔点是 CSS 的 ::after：{html}");
    }

    /// 列表里的内部链接交给链接解析器 —— 导航框整块都是链接
    #[test]
    fn links_inside_a_navbox_go_through_the_resolver() {
        use crate::RenderContext;
        use crate::link::{LinkResolver, LinkTarget, ResolvedLink};
        use std::sync::Arc;

        struct Vault;
        impl LinkResolver for Vault {
            fn resolve(&self, target: &str) -> Option<ResolvedLink> {
                Some(ResolvedLink::Link(LinkTarget::new(format!(
                    "/notes/{target}"
                ))))
            }
        }
        let resolver: Arc<dyn LinkResolver> = Arc::new(Vault);
        let context = RenderContext::new().with_links(resolver);
        let html = Mdpp::new().render_with("::navbox title=x\n  [分类]\n  * [[拱桥]]\n", &context);
        assert!(html.contains(r#"href="/notes/拱桥""#), "{html}");
    }
}

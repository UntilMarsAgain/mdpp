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
//!   [header]
//!   基本原则
//!
//!   [基本原则]
//!   求问百科是**百科全书** · 中国价值观基础上的客观观点
//!
//!   [收录标准]
//!   通用收录标准 · 人物 · 组织 · 网站
//!
//!   [header]
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
//! 分组名**可以重复**：同一个标签在不同的 `[header]` 下面各写一次是正常的
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
//! ## 三个底色参数
//!
//! `group-bg=` 每一栏标题的底色、`title-bg=` 标题栏的底色、`header-bg=` 通栏底色；
//! 都只认 `#rgb` / `#rrggbb`，与 `::banner` 的 `color=` 同一条规矩。
//!
//! **字色不给作者写**：底色定了之后字色由 [`super::text_on`] 按亮度算出来（深底配
//! 浅字），与 `::banner`、提示框同一套 —— 深底配深字是最常见的自挖坑。两者都落成
//! **自定义属性**（`--navbox-group-bg` / `--navbox-group-fg` 等），样式表那几处各自
//! 带默认值，所以「不给」与「给了」走的是同一段 CSS，而字色仍可以用 `::css` 改。
//!
//! ## 三处保留的名字
//!
//! - `[above]` / `[below]` —— 维基的那两行通栏，位置固定：一条紧贴标题栏之下，
//!   一条在最底下；
//! - `[header]` —— **通栏标题条**，落在作者写它的那两行之间（维基的 `|headern=`）。
//!   **不带编号，可以写很多条** —— 位置由次序决定，编号是多余的。
//!
//! 这三个名字因此当不了分组名 —— 要写一个叫「above」的分组，写 `[header]` 再把它
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

use super::super::dispatch::{BlockNode, no_leading_content, render_problem, sections};
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
    /// `[header]` —— 落在作者写它那两行之间的通栏标题条。
    ///
    /// **没有编号，可以写很多条** —— 它落在哪儿由写下来的次序决定，编号是多余的。
    Header,
    /// 别的字就是**分组名**，它同时也是这一节的标题
    Group(String),
}

/// 主体里的一段。**按写下来的次序**排 —— 通栏要落在它该落的那几行之间，而那个
/// 位置只有次序说得清。
///
/// `Group` 带的是那个分组名在 [`render_navbox`] 的 `groups` 里的**下标**，而不是引用 ——
/// 标签自己那份字符串活不过认标签那一轮循环，而行标签要一直用到渲染完。
/// `groups` 里存的是**渲染好的标签 HTML**：写错了在认标签那一轮就报出来
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
/// 除了 `above` / `below` / `header`，**其余一律当分组名** —— 不去猜「像不像编号」
/// 之类的东西，所以写 `[2024 年的条目]` 也是个合法的分组名。
fn slot_of(label: &str) -> Result<Slot, String> {
    match label {
        "above" => return Ok(Slot::Above),
        "below" => return Ok(Slot::Below),
        "header" => return Ok(Slot::Header),
        _ => {}
    }
    // `header` 带尾巴的全部不认。老写法（`header1`）要单独给一句 —— 它是从维基那边
    // 照搬过来的，而「不认」两个字看不出该写什么。
    if let Some(rest) = label.strip_prefix("header") {
        let hint = if rest.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
            "维基那边是 |headern=，这里不用编号：写 [header] 就好，写几条都是它"
        } else {
            "写成一个分组的话换个名字就好"
        };
        return Err(format!("[{label}] 不认：{hint}"));
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

/// 那三个**底色**参数。只认 `#rgb` / `#rrggbb` —— 与 [`::banner`](super::text) 的
/// `color=` 是同一条规矩：值要写进 `style` 属性，宽松了就等于允许往里塞任意声明。
struct Palette {
    /// `group-bg=` —— 每一栏标题的底色
    group_bg: Option<String>,
    /// `title-bg=` —— 标题栏的底色
    title_bg: Option<String>,
    /// `header-bg=` —— 通栏的底色
    head_bg: Option<String>,
}

impl Palette {
    /// 落成一条 `style`。一个都没给时不写这个属性 —— 多余的 `style=""` 是噪音。
    ///
    /// **每个底色连字色一起给**：字色由 [`super::text_on`] 按亮度算出来。深底配深字
    /// 是最常见的「自己给自己挖坑」，而 `::banner` 与提示框那一族都是这么做的，
    /// 这里跟着同一条。字色也落成自定义属性，所以谁想改仍然可以用 `::css` 改。
    fn style(&self) -> Option<String> {
        let mut out = String::new();
        for (background, foreground, value) in [
            ("--navbox-group-bg", "--navbox-group-fg", &self.group_bg),
            ("--navbox-title-bg", "--navbox-title-fg", &self.title_bg),
            ("--navbox-head-bg", "--navbox-head-fg", &self.head_bg),
        ] {
            if let Some(value) = value {
                out.push_str(&format!(
                    "{background}: {value}; {foreground}: {};",
                    super::text_on(value)
                ));
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

/// 认那三个底色参数。错的值**整块拒掉** —— 与 `size_rule` / `normalize_hex` 的
/// 用法一致：渲染一半才发现颜色非法，那一半已经是错的了。
fn palette_of(block: &Block) -> Result<Palette, String> {
    Ok(Palette {
        group_bg: bg_of(block, "group-bg")?,
        title_bg: bg_of(block, "title-bg")?,
        head_bg: bg_of(block, "header-bg")?,
    })
}

/// 取一个底色参数，认不出就说明它该长什么样
fn bg_of(block: &Block, key: &str) -> Result<Option<String>, String> {
    match block.param(key) {
        None => Ok(None),
        Some(value) => super::normalize_hex(value)
            .map(Some)
            .ok_or_else(|| super::hex_why(key, Some("#0055a4"))),
    }
}

/// 这一节的子节点里有没有一个 `::navbox` —— 即这一格装着一个子导航框。
///
/// 只数**直接**子节点：写在列表项里的（懒延续那种）不算，那整个子导航框会先被
/// markdown 收成列表项的文字（见 [`crate::block::scanner`] 的测试），这里认了
/// 反而会在一个「压根没嵌进去」的格子上打记号。
fn holds_nested_navbox(section: &Node) -> bool {
    section.children.iter().any(|child| {
        child
            .cast::<BlockNode>()
            .is_some_and(|b| b.0.name == "navbox")
    })
}

/// [`prepare`] 的产物：渲染那一半要的每一样，**都已经认过一遍**。
///
/// 打成一个包而不让渲染函数自己去取七八样 —— 这个类型就是两半的分工：
/// 能装满它的是 [`prepare`]，而它装不满时，渲染那一步连头都不该开
struct Prepared<'a> {
    /// `title=`（trim 过、空的算没给）
    title: Option<&'a str>,
    state: State,
    /// 折起来没有 —— 由 [`State`] 定，不在渲染那半另算一遍
    collapsed: bool,
    palette: Palette,
    /// `[above]` / `[below]`：位置是**定的**，不看作者把它们写在哪儿
    above: Option<&'a Node>,
    below: Option<&'a Node>,
    /// 通栏与分组行，按写下来的次序（`above` / `below` 不在这儿）
    segments: Vec<Segment<'a>>,
    /// 每个分组行一份**已渲染好的标签 HTML**：`Segment::Group` 存的是它在这里的
    /// 下标。标签写错在认这一遍时就报了，轮不到渲染
    groups: Vec<String>,
}

/// 认一遍：参数、每一节的标签、次序 —— **要报的错全报在这儿**。
///
/// 每个分支都画完一个**完整的**问题框再返回 `None`。这条线是硬的：标签写错要是
/// 渲染到半截才发现，开标签已经写出去了，页面上就是一个烂掉的框（`[link]` 后面
/// 没写页面名就是最常见的一种）。
///
/// 返回 `None` = 框已经替调用方画好了，这次渲染到此为止
fn prepare<'a>(block: &'a Block, node: &'a Node, fmt: &mut dyn Renderer) -> Option<Prepared<'a>> {
    // 没有 `title=` 就是**嵌进去的样子**：没有标题栏、没有边框、没有开关。
    let title = block
        .param("title")
        .map(str::trim)
        .filter(|t| !t.is_empty());

    let state = match state_of(block) {
        Ok(state) => state,
        Err(why) => {
            render_problem(block, fmt, &why);
            return None;
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
        return None;
    }

    let palette = match palette_of(block) {
        Ok(palette) => palette,
        Err(why) => {
            render_problem(block, fmt, &why);
            return None;
        }
    };

    // 先**整块**认一遍再渲染：标签写错了就整块报出来，免得渲染半截
    let list = sections(node);
    // 标签前有正文 = 方括号漏写了：那时连「分组名」都无从谈起，得先挡在一切之前
    if let Err(why) = no_leading_content(&list, "导航框") {
        render_problem(block, fmt, &why);
        return None;
    }
    let mut above: Option<&Node> = None;
    let mut below: Option<&Node> = None;
    let mut segments: Vec<Segment> = Vec::new();
    // 分组名的活字放这儿：`sections(node)` 给的那个 `&str` 活不过循环，而行标签要
    // 一直用到渲染完。存的是**已经渲染好的 HTML** —— 标签写错了这一轮就报，
    // 而不是轮到渲染时才发现（那会渲染出半截导航框）
    let mut groups: Vec<String> = Vec::new();
    for (label, section) in list {
        let slot = match slot_of(label) {
            Ok(slot) => slot,
            Err(why) => {
                render_problem(block, fmt, &why);
                return None;
            }
        };
        match slot {
            Slot::Above => {
                if above.replace(section).is_some() {
                    render_problem(block, fmt, "`[above]` 写了两处：一个导航框里它只能有一处");
                    return None;
                }
            }
            Slot::Below => {
                if below.replace(section).is_some() {
                    render_problem(block, fmt, "`[below]` 写了两处：一个导航框里它只能有一处");
                    return None;
                }
            }
            Slot::Header => {
                // 通栏就是它的标题，空着等于没有标题 —— 不如报错，也不该画一条空条
                if section.children.is_empty() {
                    render_problem(block, fmt, "`[header]` 是空的：通栏就是它的标题");
                    return None;
                }
                // **可以写很多条**，不去重：它落在哪儿由写下来的次序决定
                segments.push(Segment::Header(section));
            }
            // 分组名**可以重复**：它是显示用的字，不是标识
            Slot::Group(name) => {
                // 标签写错（`[link]` 后面没写页面名）在这里就整块报出来 —— 见下面那句
                let label = match group_label_html(&name) {
                    Ok(label) => label,
                    Err(why) => {
                        render_problem(block, fmt, &why);
                        return None;
                    }
                };
                groups.push(label);
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
        return None;
    }

    Some(Prepared {
        title,
        state,
        collapsed: state == State::Collapsed,
        palette,
        above,
        below,
        segments,
        groups,
    })
}

/// `::navbox` —— 见模块头。
///
/// [`prepare`] 认完之后这里只做一件事：**写**。中间那条线是硬的 —— 写到一半才
/// 发现参数不对，页面上已经留下半个框了
pub(super) fn render_navbox(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
    let Some(ready) = prepare(block, node, fmt) else {
        return;
    };
    let Prepared {
        title,
        state,
        collapsed,
        palette,
        above,
        below,
        segments,
        groups,
    } = ready;

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
                // 标签在上面认标签那一轮就渲染好了 —— 那时写错就整块报出来，
                // 走不到这儿（否则开标签已经写出去一半了）
                let label = groups[*name].as_str();
                // 装着子导航框的格子留个记号：CSS 选不了父元素，而嵌套布局要的是
                // 「父格文字大间距、子导航框贴左小间距」（维基模式），两套间距必须
                // 分开给 —— 记号由看得见结构的这一侧给
                let cell_class = if holds_nested_navbox(section) {
                    "navbox__cell navbox__cell--branch"
                } else {
                    "navbox__cell"
                }
                .to_string();
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
                fmt.text_raw(label);
                fmt.close("th");
                fmt.open("td", &[("class", cell_class)]);
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

/// 一条横跨两列的通栏（`[above]` / `[below]` / `[header]`）。
///
/// 用 `<td>` 而不是 `<th>`：这几行是**内容**（「何为方针 · 方针列表」是一句话），
/// 而这一列的表头另有其人（左边那个分组名）。`[header]` 是「下面几行的分组名」，
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
    /// 就是同一个标签在每个 `[header]` 下面各写一次。
    #[test]
    fn a_group_name_may_repeat_under_each_header() {
        let html = render(
            "::navbox title=x\n  [header]\n  基本原则\n  [基本原则]\n  甲\n  [header]\n  内容方针\n  [基本原则]\n  乙\n",
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

    /// `[header]` 是一条横跨两列的通栏，落在作者写它的那两行之间
    #[test]
    fn a_header_is_a_full_width_row_between_them() {
        let html = render(
            "::navbox title=求问百科\n  [header]\n  基本原则\n  [甲组]\n  甲\n  [乙组]\n  乙\n",
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

    /// `[header]` 不带编号；带编号的是老写法，要指出该写什么
    #[test]
    fn a_header_takes_no_number() {
        let ok = render("::navbox title=x\n  [header]\n  甲\n  [分组]\n  乙\n");
        assert!(!ok.contains("template--problem"), "{ok}");

        // 带数字的（从维基照搬过来的）不认，且提示要说清改成什么
        for bad in ["header1", "header0", "header2"] {
            let html = render(&format!("::navbox title=x\n  [{bad}]\n  甲\n"));
            assert!(html.contains("template--problem"), "{bad} -> {html}");
            assert!(html.contains("不用编号"), "{bad} -> {html}");
            assert!(html.contains("[header]"), "{bad} -> {html}");
        }
    }

    /// **`[header]` 可以写很多条** —— 它没有编号，也就不存在「重号」这回事。
    ///
    /// 这是「不带编号」换来的：编号一旦没有，去重就没有了依据，而通栏本来就是
    /// 「写在哪儿就落在哪儿」，重复并不构成冲突。
    #[test]
    fn a_header_may_appear_many_times() {
        let html = render(
            "::navbox title=x\n  [header]\n  甲\n  [一组]\n  一\n  [header]\n  乙\n  [二组]\n  二\n",
        );
        assert!(!html.contains("template--problem"), "{html}");
        assert_eq!(html.matches("navbox__head").count(), 2, "{html}");
        assert!(html.contains("甲") && html.contains("乙"), "{html}");
        // 次序：甲 → 一组 → 乙 → 二组
        let at = |needle: &str| html.find(needle).unwrap_or_else(|| panic!("{needle}"));
        assert!(
            at("甲") < at("一") && at("一") < at("乙") && at("乙") < at("二组"),
            "{html}"
        );
    }

    /// 空通栏报错（它就是它的标题，没有标题等于没有）
    #[test]
    fn an_empty_header_is_refused() {
        let empty = render("::navbox title=x\n  [header]\n  [分组]\n  甲\n");
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

    /// 装着子导航框的格子带 `navbox__cell--branch` 记号 —— CSS 靠它把
    /// 「父格文字的大间距」与「子导航框贴左的小间距」分开
    #[test]
    fn a_cell_holding_a_nested_navbox_is_marked() {
        let html =
            render("::navbox title=x\n  [分组]\n  * 甲\n\n  ::navbox\n    [子组]\n    * 乙\n");
        assert!(
            html.contains(r#"<td class="navbox__cell navbox__cell--branch">"#),
            "{html}"
        );
        // 只有装着子框的那一格带记号
        assert_eq!(html.matches("navbox__cell--branch").count(), 1, "{html}");
    }

    /// 三层嵌套时**每层**各带各的记号 —— 第三层格子里没子框，不带
    #[test]
    fn nested_cells_are_marked_at_every_level() {
        let html = render(
            "::navbox title=x\n  [甲]\n  一句\n\n  ::navbox\n    [乙]\n    一句\n\n    ::navbox\n      [丙]\n      一句\n",
        );
        assert_eq!(html.matches("navbox__cell--branch").count(), 2, "{html}");
    }

    /// 写在列表项里的不算（那是懒延续，子导航框整个被吞成文字）
    #[test]
    fn a_navbox_swallowed_by_a_list_is_not_marked() {
        let html = render("::navbox title=x\n  [分组]\n  * 甲\n  ::navbox\n    [子]\n    乙\n");
        assert!(!html.contains("navbox__cell--branch"), "{html}");
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
    ///
    /// **整块报，不渲染半截** —— 标签是在认标签那一轮认的，那时一行开标签都还没写
    #[test]
    fn a_link_label_needs_a_page() {
        for label in ["link", "link "] {
            let html = render(&format!("::navbox title=x\n  [{label}]\n  甲\n"));
            assert!(html.contains("template--problem"), "{label:?} -> {html}");
            assert!(html.contains("没写页面名"), "{html}");
            assert!(!html.contains(r#"class="navbox""#), "不渲染半截：{html}");
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

    /// 三个**底色**参数落成自定义属性，交给样式表去用；**字色一并给出**
    #[test]
    fn the_three_backgrounds_become_custom_properties() {
        let html = render(
            "::navbox title=x group-bg=#0055a4 title-bg=#123456 header-bg=#0af\n  [分类]\n  甲\n",
        );
        assert!(!html.contains("template--problem"), "{html}");
        for (name, value) in [
            ("--navbox-group-bg", "#0055a4"),
            ("--navbox-title-bg", "#123456"),
            ("--navbox-head-bg", "#00aaff"),
        ] {
            assert!(
                html.contains(&format!("{name}: {value};")),
                "少了 {name}：{html}"
            );
        }
        // `#rgb` 那种简写要规范成六位，与 `::banner` 同一套
        let short = render("::navbox title=x group-bg=#0af\n  [分类]\n  甲\n");
        assert!(short.contains("--navbox-group-bg: #00aaff;"), "{short}");
    }

    /// **深底自动配浅字、浅底自动配深字** —— 与 `::banner`、提示框同一套。
    ///
    /// 作者只管给底色；深底配深字是最常见的「自己给自己挖坑」，而底色定了之后
    /// 字色就该由这里算，而不是让作者自己配一遍。
    #[test]
    fn the_text_colour_follows_the_background_brightness() {
        let dark = render("::navbox title=x group-bg=#0055a4\n  [分类]\n  甲\n");
        assert!(
            dark.contains("--navbox-group-bg: #0055a4; --navbox-group-fg: #f5f5f5;"),
            "深底该配浅字：{dark}"
        );

        let light = render("::navbox title=x title-bg=#ffe680\n  [分类]\n  甲\n");
        assert!(
            light.contains("--navbox-title-bg: #ffe680; --navbox-title-fg: #101010;"),
            "浅底该配深字：{light}"
        );
    }

    /// 一个底色都没给时**不写** `style` —— 多余的 `style=""` 是噪音
    #[test]
    fn no_background_means_no_style_attribute() {
        let html = render("::navbox title=x\n  [分类]\n  甲\n");
        assert!(!html.contains("style="), "{html}");
        // 给了别的属性也不该冒出 style
        let nested = render("::navbox\n  [分类]\n  甲\n");
        assert!(!nested.contains("style="), "{nested}");
    }

    /// 底色值只认十六进制 —— 它要写进 `style` 属性，宽松了就等于允许塞任意声明
    #[test]
    fn a_bad_background_is_refused() {
        for (param, value) in [
            ("group-bg", "red"),
            ("title-bg", "#12345"),
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
        let headers = render("::navbox title=x\n  [header]\n  只有通栏\n");
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

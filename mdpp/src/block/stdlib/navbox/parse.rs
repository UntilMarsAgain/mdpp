use crate::block::dispatch::{BlockNode, render_problem, sections};
use crate::block::stdlib::{hex_why, normalize_hex, text_on};
use crate::registry::Block;
use markdown_it::{Node, Renderer};

/// 一个 `[标签]` 的意思。
pub(super) enum Slot {
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
pub(super) enum Segment<'a> {
    /// 一条通栏
    Header(&'a Node),
    /// 一行分组：`(分组名的下标, 这一行的内容)`
    Group(usize, &'a Node),
}

/// 折起来的状态。三档，与 `::theme` 认 `light`/`dark` 是同一个做法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum State {
    /// 展开着，且可以折起来（默认）
    Expanded,
    /// 一上来就折着
    Collapsed,
    /// 展开着，且**不能**折（没有那个按钮）
    Plain,
}

impl State {
    /// 落进 `data-state` 的那三个字
    pub(super) fn attr(self) -> &'static str {
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
pub(super) fn state_of(block: &Block) -> Result<State, String> {
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
pub(super) fn inline_html(text: &str) -> Result<String, String> {
    let rendered = crate::engine::render_nested(text);
    let trimmed = rendered.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    match trimmed
        .strip_prefix("<p>")
        .and_then(|rest| rest.strip_suffix("</p>"))
    {
        Some(inner) if !inner.contains("</p>") => Ok(inner.to_string()),
        _ => Err(
            "导航框的标题和分组名只支持行内 Markdown：不能包含段落、列表或其他块级内容".to_string(),
        ),
    }
}

/// 分组名那一格里写什么。
///
/// **`[link 页面]`** 让分组名变成一条指向内部链接的链。走的是 markdown 的
/// `[[…]]`（交给 `inline_html` 再渲染一次），所以**链接解析器是同一个**：递了解析器
/// 就出 `href`，没递或解析器说没有就落成红链 —— 与正文里的 `[[…]]` 一模一样。
///
/// 为什么不给 `[分组名 [[链接]]]` 那种写法：分节那一步只认「整行一对方括号」
/// （见 `block::section` 的 `section_label`），而它为了不把 `[[甲]]` 认成标签，
/// 会把**任何含方括号**的标签都退回去。放开它就得去数嵌套的方括号——而一个写错
/// 半边的 `[a [b]` 会让整个分节错位，页面从此莫名其妙。分开写没这个风险。
fn group_label_html(label: &str) -> Result<String, String> {
    let Some(rest) = label.strip_prefix("link") else {
        return inline_html(label);
    };
    // `linkage` / `linked` 那些是普通分组名，不是链接写法
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return inline_html(label);
    }
    let page = rest.trim();
    if page.is_empty() {
        return Err(format!(
            "[{label}] 后面没写页面名：写 [link 某页]，也可以写 [link 某页|显示的字]"
        ));
    }
    inline_html(&format!("[[{page}]]"))
}

/// 那三个**底色**参数。只认 `#rgb` / `#rrggbb` —— 与 [`::banner`](super::text) 的
/// `color=` 是同一条规矩：值要写进 `style` 属性，宽松了就等于允许往里塞任意声明。
pub(super) struct Palette {
    /// `group-bg=` —— 每一栏标题的底色
    pub(super) group_bg: Option<String>,
    /// `title-bg=` —— 标题栏的底色
    pub(super) title_bg: Option<String>,
    /// `header-bg=` —— 通栏的底色
    pub(super) head_bg: Option<String>,
}

impl Palette {
    /// 落成一条 `style`。一个都没给时不写这个属性 —— 多余的 `style=""` 是噪音。
    ///
    /// **每个底色连字色一起给**：字色由 [`text_on`] 按亮度算出来。深底配深字
    /// 是最常见的「自己给自己挖坑」，而 `::banner` 与提示框那一族都是这么做的，
    /// 这里跟着同一条。字色也落成自定义属性，所以谁想改仍然可以用 `::css` 改。
    pub(super) fn style(&self) -> Option<String> {
        let mut out = String::new();
        for (background, foreground, value) in [
            ("--navbox-group-bg", "--navbox-group-fg", &self.group_bg),
            ("--navbox-title-bg", "--navbox-title-fg", &self.title_bg),
            ("--navbox-head-bg", "--navbox-head-fg", &self.head_bg),
        ] {
            if let Some(value) = value {
                out.push_str(&format!(
                    "{background}: {value}; {foreground}: {};",
                    text_on(value)
                ));
            }
        }
        (!out.is_empty()).then_some(out)
    }
}

/// 认那三个底色参数。错的值**整块拒掉** —— 与 `size_rule` / `normalize_hex` 的
/// 用法一致：渲染一半才发现颜色非法，那一半已经是错的了。
pub(super) fn palette_of(block: &Block) -> Result<Palette, String> {
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
        Some(value) => normalize_hex(value)
            .map(Some)
            .ok_or_else(|| hex_why(key, Some("#0055a4"))),
    }
}

/// 这一节的子节点里有没有一个 `::navbox` —— 即这一格装着一个子导航框。
///
/// 会递归进 `::element` 之类的块容器；但写在列表项里的（懒延续那种）不算，
/// 那整个子导航框会先被 markdown 收成列表项的文字（见
/// [`crate::block::scanner`] 的测试），这里认了反而会在一个「压根没嵌进去」的
/// 格子上打记号。
pub(super) fn holds_nested_navbox(section: &Node) -> bool {
    fn holds(node: &Node) -> bool {
        node.children.iter().any(|child| {
            child
                .cast::<BlockNode>()
                .is_some_and(|b| b.0.name == "navbox")
                || holds(child)
        })
    }
    holds(section)
}

/// [`prepare`] 的产物：渲染那一半要的每一样，**都已经认过一遍**。
///
/// 打成一个包而不让渲染函数自己去取七八样 —— 这个类型就是两半的分工：
/// 能装满它的是 [`prepare`]，而它装不满时，渲染那一步连头都不该开
pub(super) struct Prepared<'a> {
    /// `title=`（trim 过、空的算没给）
    pub(super) title: Option<&'a str>,
    /// 标题已经按行内 Markdown 渲染好 —— 写错在 `prepare` 这一轮就报
    pub(super) title_html: Option<String>,
    pub(super) state: State,
    /// 折起来没有 —— 由 [`State`] 定，不在渲染那半另算一遍
    pub(super) collapsed: bool,
    pub(super) palette: Palette,
    /// `[above]` / `[below]`：位置是**定的**，不看作者把它们写在哪儿
    pub(super) above: Option<&'a Node>,
    pub(super) below: Option<&'a Node>,
    /// 通栏与分组行，按写下来的次序（`above` / `below` 不在这儿）
    pub(super) segments: Vec<Segment<'a>>,
    /// 每个分组行一份**已渲染好的标签 HTML**：`Segment::Group` 存的是它在这里的
    /// 下标。标签写错在认这一遍时就报了，轮不到渲染
    pub(super) groups: Vec<String>,
}

/// 认一遍：参数、每一节的标签、次序 —— **要报的错全报在这儿**。
///
/// 每个分支都画完一个**完整的**问题框再返回 `None`。这条线是硬的：标签写错要是
/// 渲染到半截才发现，开标签已经写出去了，页面上就是一个烂掉的框（`[link]` 后面
/// 没写页面名就是最常见的一种）。
///
/// 返回 `None` = 框已经替调用方画好了，这次渲染到此为止
pub(super) fn prepare<'a>(
    block: &'a Block,
    node: &'a Node,
    fmt: &mut dyn Renderer,
) -> Option<Prepared<'a>> {
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

    let title_html = match title {
        Some(title) => match inline_html(title) {
            Ok(html) => Some(html),
            Err(why) => {
                render_problem(block, fmt, &why);
                return None;
            }
        },
        None => None,
    };

    Some(Prepared {
        title,
        title_html,
        state,
        collapsed: state == State::Collapsed,
        palette,
        above,
        below,
        segments,
        groups,
    })
}

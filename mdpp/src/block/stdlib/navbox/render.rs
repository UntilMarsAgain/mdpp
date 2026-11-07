use crate::registry::Block;
use markdown_it::{Node, Renderer};

use super::parse::{Prepared, Segment, State, holds_nested_navbox, inline_html, prepare};
use super::{HIDE, SHOW};

/// `::navbox` —— 见模块头。
///
/// [`prepare`] 认完之后这里只做一件事：**写**。中间那条线是硬的 —— 写到一半才
/// 发现参数不对，页面上已经留下半个框了
pub(crate) fn render_navbox(block: &Block, node: &Node, fmt: &mut dyn Renderer) {
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
    // `mdpp-cli/assets/mdpp.css` 的 `@media print` 那一节），而 `display: none`
    // 打印时救不回来
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
pub(super) fn full_row(fmt: &mut dyn Renderer, class: &str, section: &Node) {
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

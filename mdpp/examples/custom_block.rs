// 自定义块：往注册表里加自己的 `::` 块。
//
//     cargo run -p mdpp --example custom_block
//
// 三种 `ContentMode` 各来一个 —— 那不是三份文档，是**三种写法形状**：
// Markdown 档能写列表与内嵌块，Sections 档按 `[标签]` 分节，Raw 档一个字都不解析
// （TeX、样式、代码里的记号按 markdown 走一遍就毁了）。
//
// 跑法：`cargo run -p mdpp --example custom_block`

use mdpp::markdown_it::{Node, Renderer};
use mdpp::{registry::BlockRegistry, ContentMode, Mdpp};

/// `::倒计时 日期=2026-12-31` —— 一个 Raw 档的块。
///
/// 内容原样拿来做倒推：天数**在渲染时算**，所以同一段正文在不同日子渲染会得到
/// 不同的结果 —— 这是刻意的（这是给「还有几天」这类话用的），但也意味着这一块
/// 的输出不是纯函数，别拿它做缓存。
fn countdown(block: &mdpp::Block, _node: &Node, fmt: &mut dyn Renderer) {
    let Some(date) = block.param("日期") else {
        fmt.text("缺少 日期=…");
        return;
    };
    fmt.open("span", &[("class", "countdown".to_string())]);
    fmt.text(&format!("距 {date} 还有 "));
    // 真算的话得有个日期库；这里只演示形状，不假装算得出来
    fmt.text("若干");
    fmt.close("span");
}

/// `::对照 左=… 右=…` —— 一个 Markdown 档的块。
///
/// `fmt.contents(&node.children)` 是内容那一侧的规矩：块正文已经被整个解析器
/// 解析过并挂在 `children` 上了，渲染器只管把它写出去。想让块里能写列表、
/// 链接、还能再嵌块，就得走这一档。
fn compare(block: &mdpp::Block, node: &Node, fmt: &mut dyn Renderer) {
    fmt.open("dl", &[("class", "compare".to_string())]);
    for side in ["左", "右"] {
        if let Some(value) = block.param(side) {
            fmt.open("dt", &[]);
            fmt.text(side);
            fmt.close("dt");
            fmt.open("dd", &[]);
            fmt.text(value);
            fmt.close("dd");
        }
    }
    fmt.close("dl");
    fmt.contents(&node.children);
}

/// `::步骤` —— 一个 Sections 档的块，按 `[标签]` 分节。
fn steps(_block: &mdpp::Block, node: &Node, fmt: &mut dyn Renderer) {
    fmt.open("ol", &[("class", "steps".to_string())]);
    for section in &node.children {
        let Some(label) = section.cast::<mdpp::block::Section>() else {
            continue;
        };
        fmt.open("li", &[]);
        fmt.text(&label.label);
        fmt.contents(&section.children);
        fmt.close("li");
    }
    fmt.close("ol");
}

fn main() {
    // 块必须**独占一行**才成块 —— `然后 ::对照 …` 那一行里它只是正文里的几个字，
    // 与普通段落里写 `::note` 不会被当成块是同一条规矩
    let mut registry = BlockRegistry::with_builtins();
    registry
        .register_fn("倒计时", ContentMode::Raw, countdown)
        .register_fn("对照", ContentMode::Markdown, compare)
        .register_fn("步骤", ContentMode::Sections, steps);

    let mdpp = Mdpp::builder().registry(registry).build();

    let source = "\
::对照 左=旧写法 右=新写法
  两种写法在这一点上不同，其余一样。

::步骤
  [先做这个]
  把 **前提**准备好。
  [再做这个]
  ::对照 左=甲 右=乙
";

    println!("{}", mdpp.render(source));
}

use super::Mdpp;
use crate::registry::BlockRegistry;
use crate::registry::ContentMode;
use markdown_it::{Node, Renderer};

fn md_of(src: &str) -> String {
    Mdpp::new().render(src)
}

/// 解析器构建完之后只读，渲染状态每线程一份 —— 所以它该能跨线程共享
#[test]
fn mdpp_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Mdpp>();
}

/// `parse` 交出的 AST 可以再交给 `render_node`：块也照常查注册表
#[test]
fn parse_then_render_round_trips() {
    let mdpp = Mdpp::new();
    let root = mdpp.parse("::note\n  **粗**\n");
    let html = mdpp.render_node(&root, &crate::context::RenderContext::new());
    assert!(html.contains("callout--note"), "{html}");
    assert!(html.contains("<strong>粗</strong>"), "{html}");
}

/// 拿到 AST 之后可以改属性再渲染 —— 这是「不只填空」的那条路
#[test]
fn a_parsed_ast_can_be_modified_before_rendering() {
    let mdpp = Mdpp::new();
    let mut root = mdpp.parse("# 标题\n");
    root.children[0].attrs.push(("class", "mine".to_string()));
    let html = mdpp.render_node(&root, &crate::context::RenderContext::new());
    assert!(html.contains(r#"class="mine""#), "{html}");
}

/// 渲染 AST 也有站点模式：`::css` 外链并进资源清单
#[test]
fn a_parsed_ast_can_be_rendered_with_assets() {
    let mdpp = Mdpp::new();
    let root = mdpp.parse("::css\n  .a { color: red; }\n");
    let rendered =
        mdpp.render_node_with_assets(&root, &crate::context::RenderContext::new(), "assets");
    assert!(rendered.html.contains("assets/mdpp-"), "{}", rendered.html);
    assert_eq!(rendered.assets.len(), 1, "{:?}", rendered.assets);
}

/// 兜底：标题、强调这些最基本的记号还认得 —— 开关加了这么多，先看这里
#[test]
fn commonmark_is_there() {
    let html = md_of("# 一级\n\n正文里有**强调**。\n");
    assert!(html.contains("<h1"), "{html}");
    assert!(html.contains("<strong>强调</strong>"), "{html}");
}

/// 正文里的 HTML 默认转义掉 —— 透传得是调用方自己打开 raw HTML 才有的待遇
#[test]
fn raw_html_is_escaped() {
    let html = md_of("<script>alert(1)</script>\n");
    assert!(!html.contains("<script"), "{html}");
    assert!(html.contains("&lt;script"), "{html}");
}

/// `javascript:` 那类协议不许落成 `href` —— 链接是点得开的，这一条漏了就是注入入口
#[test]
fn dangerous_link_protocols_are_rejected() {
    let html = md_of("[x](javascript:alert(1))\n");
    assert!(!html.contains(r#"href="javascript:"#), "{html}");
}

/// 站外链接挂 `external` 类，相对地址与内部链接都不挂 —— 样式表靠这个类画外链符号
#[test]
fn external_links_are_marked_but_internal_ones_are_not() {
    let html = md_of("[外](https://example.com/x) [内](../指南/) [[某页]]\n");
    assert!(
        html.contains(r#"<a class="mdpp-external" href="https://example.com/x">"#),
        "{html}"
    );
    assert_eq!(html.matches("external__text").count(), 1, "{html}");
    // 全页只有那一条外链带这个类
    assert_eq!(
        html.matches(r#"class="mdpp-external""#).count(),
        1,
        "{html}"
    );
}

/// 三种链接节点（普通链接、尖括号自动链接、裸地址）都认，站内与邮件不认
#[test]
fn every_kind_of_external_link_is_marked() {
    let html = md_of("[外](https://example.com/x) <https://example.com/y> https://example.com/z\n");
    assert_eq!(
        html.matches(r#"class="mdpp-external""#).count(),
        3,
        "{html}"
    );
    // 相对地址、内部链接、邮件都不该带这个类
    let html = md_of("[内](../指南/) [[某页]] [信](mailto:a@b.c)\n");
    assert!(!html.contains("external"), "{html}");
}

/// 嵌套块里的外链也只挂一次类：core 规则每层 `md.parse` 都会走整棵树
#[test]
fn nested_blocks_do_not_duplicate_the_external_class() {
    let html = md_of("::note\n  [外](https://example.com/x)\n");
    assert_eq!(
        html.matches(r#"class="mdpp-external""#).count(),
        1,
        "{html}"
    );
    assert_eq!(html.matches("external__text").count(), 1, "{html}");
}

/// 围栏上的语言名要带进 `class="language-…"` —— 高亮与样式都认这个类
#[test]
fn fenced_code_keeps_its_language_class() {
    let html = md_of("```rust\nfn main() {}\n```\n");
    assert!(html.contains(r#"<code class="language-rust">"#), "{html}");
}

/// Setext 默认关：`标题` 下一行写 `---` 得退回「段落 + 分隔线」，
/// 一段写歪的正文不能悄悄变成另一种意思
#[test]
fn setext_headings_are_off_by_default() {
    let html = md_of("标题\n===\n");
    assert!(!html.contains("<h1"), "Setext 标题应当已被移除: {html}");
    // 下划线是 `---` 时更要确认：它本身就是 thematic break，
    // 移除 Setext 后应退化成「段落 + 一条分隔线」
    let html = md_of("标题\n---\n");
    assert!(!html.contains("<h1"), "{html}");
    assert!(html.contains("<hr"), "--- 应当退化成分隔线: {html}");
}

/// 摘 Setext 时别把 `# 标题` 那条路一起摘了 —— 两种标题是分开的两条规则
#[test]
fn atx_headings_still_work() {
    let html = md_of("# 一级\n\n## 二级\n");
    assert!(html.contains("<h1"), "{html}");
    assert!(html.contains("<h2"), "{html}");
}

/// 标题默认带 `id` —— `[跳转](#…)` 得有落点，锚点开关的开那一半
#[test]
fn headings_get_anchor_ids() {
    let html = md_of("## 文本与行内元素\n");
    assert!(html.contains(r#"id="文本与行内元素""#), "{html}");
}

/// 嵌套解析（块里的标题、模板页展开）不该把同一个标题挂两遍 `id` ——
/// 子解析挂一次，外层 core 再走整棵树又挂一次，重复的 id 是无效 HTML
#[test]
fn nested_parses_do_not_duplicate_heading_ids() {
    let html = md_of("::note\n  # 标题甲\n");
    assert_eq!(
        html.matches(r#"id="标题甲""#).count(),
        1,
        "一个标题只该有一个 id：{html}"
    );
}

/// 同一页两个同名标题：锚点要唯一，后一个加计数
#[test]
fn two_headings_with_the_same_text_get_unique_ids() {
    let html = md_of("## 安装\n\n## 安装\n");
    assert_eq!(
        html.matches(r#"id="安装""#).count(),
        1,
        "第一个保持原名：{html}"
    );
    assert!(html.contains(r#"id="安装-1""#), "第二个要加计数：{html}");
}

/// 关掉之后一个 `id=` 都不剩 —— 开关要关得干净，而不是只关掉一半
#[test]
fn anchor_anchors_can_be_turned_off() {
    let html = Mdpp::builder()
        .heading_anchors(false)
        .build()
        .render("## 表格\n");
    assert!(html.contains("<h2"), "{html}");
    assert!(!html.contains("id="), "{html}");
}

/// 四个空格开头的行是正文不是代码 —— 缩进在本库里是 `::` 块的边界，两者会互相干扰
#[test]
fn indented_code_blocks_are_off_by_default() {
    let html = md_of("    四个空格开头\n");
    assert!(!html.contains("<pre"), "不该再被当成代码块：{html}");
    assert!(html.contains("四个空格开头"), "{html}");
}

/// 代码片段里的字面量不许被排版规则改写。
///
/// 排版规则会把 `--` 变成 `–`、把直引号变成弯引号 —— 在正文里是锦上添花，
/// 在代码片段里则是**改坏了内容**：`--accent` 变成 `–accent` 就不再是那个
/// CSS 变量名了。
#[test]
fn code_spans_keep_their_literals() {
    let source = r#"`--accent` 与 `a -- b` 与 "引号""#;
    let html = md_of(source);
    assert!(html.contains("--accent"), "破折号被改写了：{html}");
    assert!(html.contains("a -- b"), "破折号被改写了：{html}");
    assert!(!html.contains('\u{201c}'), "引号被换成弯引号了：{html}");
    assert!(
        html.contains("&quot;引号&quot;"),
        "直引号应当原样保留：{html}"
    );
}

/// 排版规则是可以打开的 —— 这里是证明「默认关」是一个选择，不是不支持
#[test]
fn the_typographer_can_be_turned_on_on_purpose() {
    let html = Mdpp::builder().typographer(true).build().render("a -- b\n");
    assert!(html.contains('\u{2013}'), "{html}");
}

/// 关掉内部链接之后，`[[…]]` 老实当普通文字
#[test]
fn the_wikilink_syntax_can_be_turned_off() {
    let html = Mdpp::builder().wikilink(false).build().render("[[目标]]\n");
    assert!(!html.contains("wikilink"), "{html}");
    assert!(html.contains("[[目标]]"), "{html}");
}

/// 自定义注册表换掉整张表：`::quote` 不在了，它就是未知块
#[test]
fn a_custom_registry_replaces_the_whole_table() {
    let mut registry = BlockRegistry::new();
    registry.register_fn("我的块", ContentMode::Markdown, |_, _, _| {});
    let mdpp = Mdpp::builder().registry(registry).build();

    assert!(!mdpp.render("::quote\n  甲\n").contains("<blockquote"));
    assert!(mdpp.render("::quote\n  甲\n").contains("template--unknown"));
}

/// 注册进去的块真的渲染了
#[test]
fn a_registered_block_renders() {
    let mut registry = BlockRegistry::with_builtins();
    registry.register_fn("问候", ContentMode::Markdown, |block, node, fmt| {
        fmt.open("p", &[("class", "greet".to_string())]);
        fmt.text(&format!("你好，{}", block.param("who").unwrap_or("世界")));
        fmt.contents(&node.children);
        fmt.close("p");
    });
    let mdpp = Mdpp::builder().registry(registry).build();

    let html = mdpp.render("::问候 who=某人\n  内容\n");
    assert!(html.contains(r#"class="greet""#), "{html}");
    assert!(html.contains("你好，某人"), "{html}");
    // 内容按 Markdown 档处理，所以正文解析过了
    assert!(html.contains("<p>内容</p>"), "{html}");
}

/// **原始档**的内容不解析：写在里面的 markdown 记号原样留着
#[test]
fn a_raw_block_keeps_its_body_untouched() {
    let mut registry = BlockRegistry::new();
    registry.register_fn("原样", ContentMode::Raw, |block, node, fmt| {
        fmt.open("pre", &[]);
        fmt.open("code", &[]);
        fmt.text(&block.body);
        fmt.close("code");
        fmt.close("pre");
        assert!(node.children.is_empty(), "Raw 档不该解析内容");
    });
    let mdpp = Mdpp::builder().registry(registry).build();

    let html = mdpp.render("::原样\n  *星号* 与 $a_1$\n");
    assert!(html.contains("*星号* 与 $a_1$"), "{html}");
    assert!(!html.contains("<em>"), "{html}");
}

/// 同一个块可以按实例选内容模式：`raw=1` 时不解析，否则照 markdown 解析
#[test]
fn a_block_can_choose_its_content_mode_per_instance() {
    let mut registry = BlockRegistry::new();
    let spec = crate::registry::BlockSpec::new(
        "切换",
        ContentMode::Markdown,
        |block: &crate::registry::Block, node: &Node, fmt: &mut dyn Renderer| {
            if block.has("raw") {
                fmt.text(&block.body);
            } else {
                fmt.contents(&node.children);
            }
        },
    )
    .with_mode_fn(|block| {
        if block.has("raw") {
            ContentMode::Raw
        } else {
            ContentMode::Markdown
        }
    });
    registry.register(spec);
    let mdpp = Mdpp::builder().registry(registry).build();

    let markdown = mdpp.render("::切换\n  **粗**\n");
    assert!(markdown.contains("<strong>粗</strong>"), "{markdown}");

    let raw = mdpp.render("::切换 raw=1\n  **粗**\n");
    assert!(raw.contains("**粗**"), "{raw}");
    assert!(!raw.contains("<strong>"), "{raw}");
}

/// 两个 `Mdpp` 各带各的注册表，互不干扰 —— 注册表是**每解析器一份**，
/// 不是全局一份
#[test]
fn two_parsers_keep_their_own_registry() {
    let mut registry = BlockRegistry::new();
    registry.register_fn("只有一个", ContentMode::Raw, |_, _, fmt| {
        fmt.text("出来了")
    });
    let one = Mdpp::builder().registry(registry).build();
    let two = Mdpp::builder().registry(BlockRegistry::new()).build();

    assert!(one.render("::只有一个\n").contains("出来了"));
    assert!(!two.render("::只有一个\n").contains("出来了"));
    // 交错着来也各归各
    assert!(one.render("::只有一个\n").contains("出来了"));
    assert!(!two.render("::只有一个\n").contains("出来了"));
}

/// 嵌套渲染走的是**当次那一份**注册表：外层注册了 `::套娃`，内层也要认得它
///
/// 这条钉的是 `render_nested` 而不是「另建一个 `Mdpp`」—— 后者会拿到一份**内置**
/// 注册表，于是外层注册的自定义块在内层全成了未知块。
#[test]
fn nested_rendering_reuses_the_outer_registry() {
    let mut registry = BlockRegistry::new();
    registry.register_fn("套娃", ContentMode::Markdown, |block, _node, fmt| {
        if block.has("里") {
            fmt.text("到底了");
            return;
        }
        fmt.text(&crate::engine::render_nested("::套娃 里\n"));
    });
    let mdpp = Mdpp::builder().registry(registry).build();

    let html = mdpp.render("::套娃\n");
    assert!(html.contains("到底了"), "{html}");
    assert!(!html.contains("template--unknown"), "{html}");
}

/// 渲染器可以拿到子节点（内容已经被解析过）—— 容器型块的常规需求
#[test]
fn renderers_can_reach_the_parsed_children() {
    let mut registry = BlockRegistry::new();
    registry.register_fn("壳", ContentMode::Markdown, |_, node, fmt| {
        fmt.open("div", &[("class", "shell".to_string())]);
        fmt.contents(&node.children);
        fmt.close("div");
    });
    let mdpp = Mdpp::builder().registry(registry).build();

    let html = mdpp.render("::壳\n  - 甲\n  - 乙\n");
    assert!(html.contains("<li>甲</li>"), "{html}");
    assert!(html.contains(r#"<div class="shell">"#), "{html}");
}

/// 渲染器拿到的是 markdown-it 的渲染器，签名照旧
#[test]
fn a_renderer_can_be_a_plain_function_pointer() {
    fn shout(block: &crate::registry::Block, node: &Node, fmt: &mut dyn Renderer) {
        fmt.text(block.name.as_str());
        fmt.contents(&node.children);
    }
    let mut registry = BlockRegistry::new();
    registry.register_fn("叫", ContentMode::Markdown, shout);
    let mdpp = Mdpp::builder().registry(registry).build();
    let html = mdpp.render("::叫\n  甲\n");
    assert!(html.contains("叫"), "{html}");
    assert!(html.contains("<p>甲</p>"), "{html}");
}

/// 渲染报告从**结构**里取标题与特性，不数 HTML 里的类名
#[test]
fn rendered_reports_outline_and_features() {
    let mdpp = Mdpp::new();
    let rendered = mdpp.render_report(
        "# 标题\n\n## 小节\n\n```rust\nfn main() {}\n```\n",
        &crate::context::RenderContext::new(),
    );
    assert_eq!(
        rendered
            .outline
            .iter()
            .map(|heading| (heading.level, heading.text.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "标题"), (2, "小节")]
    );
    assert!(rendered.outline[0].id.is_some(), "{rendered:?}");
    assert_eq!(rendered.title(), Some("标题"));
    assert!(rendered.features.highlight, "{rendered:?}");
    assert!(!rendered.features.math, "{rendered:?}");
    assert!(!rendered.features.mermaid, "{rendered:?}");
    assert_eq!(rendered.problems, crate::engine::Problems::default());
}

/// 语义块与行内公式各自点亮对应特性；`::code` 的语言名也算高亮
#[test]
fn rendered_features_follow_semantic_nodes() {
    let mdpp = Mdpp::new();
    let rendered = mdpp.render_report(
        "::mermaid\n  graph TD; A-->B;\n\n::math\n  E=mc^2\n\n::code lang=rust\n  fn main() {}\n\n行内 $x$\n",
        &crate::context::RenderContext::new(),
    );
    assert!(rendered.features.mermaid, "{rendered:?}");
    assert!(rendered.features.math, "{rendered:?}");
    assert!(rendered.features.highlight, "{rendered:?}");
}

/// 问题框在渲染那一处计数：正文/代码里恰好写着类名不算
#[test]
fn rendered_problems_are_counted_at_the_render_site() {
    let mdpp = Mdpp::new();
    let rendered = mdpp.render_report(
        "::没这个名字\n  内容\n\n::mermaid\n\n```md\n<div class=\"mdpp-template--unknown\"></div>\n```\n",
        &crate::context::RenderContext::new(),
    );
    assert_eq!(rendered.problems.unknown, 1, "{rendered:?}");
    assert_eq!(rendered.problems.misused, 1, "{rendered:?}");
    assert!(
        !rendered.html.contains("template--unknown\"></div>"),
        "代码被解析了"
    );
}

/// 缩进的一级标题也能成为标题 —— 旧的手写扫描只认列首的 `# `
#[test]
fn rendered_title_accepts_indented_headings() {
    let rendered = Mdpp::new().render_report(
        "   # 缩进标题\n\n正文\n",
        &crate::context::RenderContext::new(),
    );
    assert_eq!(rendered.title(), Some("缩进标题"));
    assert!(rendered.html.contains("<h1"), "{}", rendered.html);
}

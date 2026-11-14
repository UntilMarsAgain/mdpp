//! 块级规则本体：在 markdown-it 的块解析里认出 `::名字 …` 那一行，往下量缩进，收成一个节点。
//!
//! 四件事，各自有讲究：
//!
//! 1. **头**：`::名字 key=value …` 交给 [`Block::parse_header`]（引号与转义那里的活最多）；
//! 2. **块边界**：往后看每一行的缩进 —— 更深算块内、不更深即结束，**空行不直接结束**
//!    （见下面 `run` 里那段规则）；收上来的行按公共缩进削平，还原成块正文；
//! 3. **内容**：按注册表里那条的 [`ContentMode`] 处理 —— 交给整个解析器再解析一遍、
//!    按严格缩进把 `[标签]` 切成几节各解析一遍、或者干脆不解析；
//! 4. **名字不在表里**：交给 [`crate::block::expand`] 去模板页里找同名的一页。

use markdown_it::Node;
use markdown_it::parser::block::{BlockRule, BlockState};

use crate::block::dispatch::BlockNode;
use crate::block::section::{self, SectionProblem};
use crate::block::{Section, expand, template};
use crate::registry::{self as reg, Block, ContentMode};

/// 块级规则：由 [`crate::block::add`] 挂到解析器上，排在段落规则之前。
#[doc(hidden)]
pub struct BlockScanner;

impl BlockRule for BlockScanner {
    fn run(state: &mut BlockState) -> Option<(Node, usize)> {
        let start = state.line;
        let marker_indent = state.line_indent(start);
        if marker_indent < 0 {
            return None;
        }
        let line = state.get_line(start);

        // 头解析失败：普通块继续按段落回退；只有名字看起来是模板（或 `::if`）
        // 时才把错误摆出来。模板调用的宽松回退正是这次要修掉的静默失败。
        let Some((name, params)) = Block::parse_header(line) else {
            return malformed_template_block(state, start, marker_indent, line);
        };

        let spec = reg::lookup(&name);
        // 名字撞上内置块时按内置块算；内置块的头仍走原来的宽松回退。
        let template_page = if spec.is_none() {
            template::page_for(&name)
        } else {
            None
        };
        let is_template = template_page.is_some();
        let strict = is_template || name == "if";

        // 模板调用（以及新的 `::if`）在解析参数之前先把写法查一遍：
        // `key= value` / 重复键 / 空键这些静默切错的写法在这里停下。
        if strict && let Err(why) = template::check_header(&name, line, &params) {
            let (body, next) = collect_body(state, start, marker_indent);
            return Some((
                expand::header_problem(&name, params, body, &why),
                next - start,
            ));
        }

        // `::if` 的条件要保留 `{{@名字}}` 到渲染时再查：只有在那里才能区分
        // 「变量不存在」与「变量存在但为空」。其它块在分词之后就可以把晚绑定值填进去。
        let raw_params = params.clone();
        let params = if name == "if" {
            params
        } else {
            match template::resolve_refs(params) {
                Ok(params) => params,
                Err(why) => {
                    let (body, next) = collect_body(state, start, marker_indent);
                    return Some((
                        expand::header_problem(&name, raw_params, body, &why),
                        next - start,
                    ));
                }
            }
        };

        let (body, next) = collect_body(state, start, marker_indent);
        // `text=…` 给了就以它为准（`::banner` 那种一条横条，正文与参数二选一）。
        // 它也要能写 markdown，所以和块内容走同一条路。
        //
        // **只有会解析的那两档吃它**：原始档的正文原样摆在 `block.body` 里（一个字
        // 不动，`text=` 同样不动它），用户块也照旧从 `body` 去模板页取正文 ——
        // 写在 `::css text=…` 或 `::卡片 text=…` 上是**不起作用**的。
        //
        // `::surface` 是例外：它的 `text=` 是**字色**（`auto` 或十六进制），不是正文。
        // 把这个名字让给内容替换会让 `text=auto` 直接吞掉整个盒子。
        let source = if name == "surface" {
            body.clone()
        } else {
            match params.iter().find(|(key, _)| key == "text") {
                Some((_, text)) => text.clone(),
                None => body.clone(),
            }
        };

        // **用户自定义块**：名字不在注册表里，就去模板页里找同名的一页。找得到就把那一页
        // 的正文填好参数、当这一块的内容解析 —— 模板里照样能写 markdown、能嵌别的块。
        //
        // 内置的优先：名字撞上时按内置的算（不然 `::quote` 会被某个同名页顶掉）。
        let Some(spec) = spec else {
            let Some(page) = template_page else {
                let block = Block { name, params, body };
                return Some((Node::new(BlockNode(block)), next - start));
            };
            let node = expand::expand_user_block(&name, params, body, page, state);
            return Some((node, next - start));
        };

        let block = Block { name, params, body };
        let mode = spec.mode_for(&block);
        // 分节结构错误要在扫描阶段摆出来；block 随后要进节点，先留一份给诊断框
        let problem_block = block.clone();
        let mut node = Node::new(BlockNode(block));

        match mode {
            // 原始档：**不解析**，正文就在 `block.body` 里，一个字不动
            ContentMode::Raw => {}
            // 分节：块内容按 `[标签]` 切成几节，**各节各解析一遍** ——
            // 一节是一篇小文档，这才谈得上「这一节里写什么」。
            ContentMode::Sections => {
                match section::parse(&source) {
                    Ok(sections) => {
                        for raw in sections {
                            let mut section = Node::new(Section {
                                label: raw.label,
                                body: raw.body.clone(),
                            });
                            let mut parsed =
                                crate::parse::markdown::parse_markdown(state.md, &raw.body);
                            section.children = std::mem::take(&mut parsed.children);
                            node.children.push(section);
                        }
                    }
                    Err(why) => {
                        // 结构错误在扫描阶段就报：先写出半截分节、渲染时才发现，
                        // 页面上已经留下一个开着的容器了
                        return Some((
                            Node::new(SectionProblem::new(problem_block, why)),
                            next - start,
                        ));
                    }
                }
            }
            ContentMode::Markdown => {
                // 内容交给**整个解析器**再解析一遍：块里因此可以写 markdown、内部链接，
                // 也可以再嵌块（嵌套的 `::quote` 就是靠这一步成立的）。
                //
                // 代价记在这里，因为它是**已知的慢**：块规则在这一行重新进整个解析器，
                // 于是第 1 层的 `parse` 会再去处理第 2 层，第 2 层又处理第 3 层……
                // 嵌套 N 层时累计的代价随 N 超线性增长（实测每层约 ×1.95）。要治得
                // 从这里下手（缓存子解析结果），而不是在别处加限制器 —— 那类限制器
                // 只挡住症状，挡住之后内容就少了。
                //
                // `Node` 带 Drop，字段不能直接搬出来，所以用 `mem::take` 换走它的 children。
                let mut parsed = crate::parse::markdown::parse_markdown(state.md, &source);
                node.children = std::mem::take(&mut parsed.children);
            }
        }
        Some((node, next - start))
    }
}

/// 把块内容收成 `(body, 下一行)`。
///
/// 模板头写坏时也要走这里：不消费块内容的话，正文会被 markdown 当成块外的一段，
/// 错误框和内容就分家了。
fn collect_body(state: &BlockState, start: usize, marker_indent: i32) -> (String, usize) {
    // (相对块头的缩进, 行文本)：缩进要留着，块内容的层级靠它
    // ---- 块边界在这里定规则 ----
    //
    // 三条规则：更深算块内；不更深即结束；**空行不直接结束**，要往后看一行 ——
    // 后面还有更深的内容，这个空行才算块内。
    let mut body_lines: Vec<(usize, &str)> = Vec::new();
    let mut line = start + 1;
    while line < state.line_max {
        // 空行的 `line_indent` 是 0 而不是 -1，必须用 `is_empty` 单独判
        let blank = state.is_empty(line) || state.line_indent(line) < 0;
        if blank {
            // 空行：只有后面还有更深的内容时才算块内，否则它属于块外
            let next = (line + 1..state.line_max).find(|candidate| {
                !state.is_empty(*candidate) && state.line_indent(*candidate) >= 0
            });
            match next {
                Some(next) if state.line_indent(next) > marker_indent => {
                    body_lines.push((0, ""));
                    line += 1;
                    continue;
                }
                _ => break,
            }
        }
        if state.line_indent(line) <= marker_indent {
            break;
        }
        let indent = state.line_indent(line).max(0) as usize;
        body_lines.push((indent, state.get_line(line)));
        line += 1;
    }
    (rebuild_body(&body_lines), line)
}

/// 头写坏、但名字指向模板（或 `::if`）：收下正文，报「用法有问题」。
fn malformed_template_block(
    state: &BlockState,
    start: usize,
    marker_indent: i32,
    line: &str,
) -> Option<(Node, usize)> {
    let (name, why) = template::malformed_header(line)?;
    // `template:卡片` 这类带冒号的名字不是库的模板语法：它是普通段落。
    // 命名空间归调用方识别，这里不替它猜。
    if name.contains(':') {
        return None;
    }
    let builtin = reg::lookup(&name).is_some();
    let is_template = !builtin && template::page_for(&name).is_some();
    if !(is_template || name == "if") {
        return None;
    }
    let (body, next) = collect_body(state, start, marker_indent);
    Some((
        expand::header_problem(&name, Vec::new(), body, &why),
        next - start,
    ))
}

/// 拼回块内容：`(相对块头的缩进, 行文本)` → 去掉公共缩进后的文本。
///
/// **必须用 `state.line_indent` 的数值**，不能去数字符串开头的空格：
/// markdown-it 的 `get_line` 已经把行首空白吃掉了（"trimming initial spaces"），
/// 数出来永远是 0 —— 那样块内的层级会被悄悄抹平，嵌套的块就坏了。
fn rebuild_body(lines: &[(usize, &str)]) -> String {
    let shared = lines
        .iter()
        .filter(|(_, text)| !text.trim().is_empty())
        .map(|(indent, _)| *indent)
        .min()
        .unwrap_or(0);
    lines
        .iter()
        .map(|(indent, text)| {
            if text.is_empty() {
                String::new()
            } else {
                format!("{}{}", " ".repeat(indent.saturating_sub(shared)), text)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::RenderContext;
    use crate::block::template::{TemplatePage, TemplateSource};
    use crate::engine::Mdpp;

    // 这一组测的是**块边界怎么划**，所以占位名必须挑一个**不在注册表里**的：注册表
    // 那些块各有各的渲染器，`::note` 一旦进了注册表，块就渲染成了那个样子，边界
    // 反而测不出来。

    fn render(src: &str) -> String {
        Mdpp::new().render(src)
    }

    /// 缩进量划边界：缩进去的归块身，顶格的那行回到正文里
    #[test]
    fn indentation_separates_inside_from_outside() {
        let html = render("::外层\n  块内\n块外\n");
        assert!(html.contains("块内"), "{html}");
        assert!(html.contains("<p>块外</p>"), "{html}");
    }

    /// 深一层的整段都归外层 —— 连里层那行 `::inner` 也留在块身里，不另起一块
    #[test]
    fn deeper_levels_stay_inside() {
        let html = render("::外层\n  第一层\n    ::inner\n      第二层\n");
        assert!(html.contains("::inner"), "{html}");
        assert!(html.contains("第二层"), "{html}");
    }

    /// 空行后面还缩着，那一段就仍是块身 —— 不能在空行处腰斩，把后半截赶出去
    #[test]
    fn blank_line_inside_stays_inside() {
        let html = render("::外层\n  第一段\n\n  第二段\n");
        assert!(html.contains("第一段"), "{html}");
        assert!(html.contains("第二段"), "{html}");
        assert!(!html.contains("<p>第二段</p>"), "第二段仍应在块内：{html}");
    }

    /// 空行之后跟的是顶格的行，块到此为止 —— 那一行属于正文
    #[test]
    fn blank_line_before_unindented_text_ends_the_block() {
        let html = render("::外层\n  块内\n\n块外\n");
        assert!(html.contains("块内"), "{html}");
        assert!(html.contains("<p>块外</p>"), "{html}");
    }

    /// 比首行缩得更深的行照样收进来，公共缩进削平后深浅还在 —— 只认一种缩进量就会把里层丢掉
    #[test]
    fn dedent_keeps_relative_structure() {
        let html = render("::外层\n    两空格缩进之下\n      再深一层\n");
        assert!(html.contains("两空格缩进之下"), "{html}");
        assert!(html.contains("再深一层"), "{html}");
    }

    /// 空行**不直接结束块**：后面还有更深的内容，它就算块内。
    ///
    /// 这是块边界规则里最容易走样的地方（「空行即结束」看着更直觉），
    /// 所以钉死它，改渲染规则时会先撞上它。
    #[test]
    fn blank_line_does_not_end_the_block_by_itself() {
        let html = render("::quote\n  第一段\n\n  第二段\n块外\n");
        assert!(html.contains("第一段"), "{html}");
        assert!(html.contains("第二段"), "{html}");
        // 两段都应当在同一个引用块里
        assert_eq!(html.matches("<blockquote").count(), 1, "{html}");
        let before = &html[..html.find("第二段").expect("应当渲染出第二段")];
        let depth = before.matches("<blockquote").count() - before.matches("</blockquote>").count();
        assert_eq!(depth, 1, "第二段应当仍在块内：{html}");
        // 不再是更深的那一行才结束
        assert!(html.contains("<p>块外</p>"), "{html}");
    }

    /// 块身不是死文字 —— 里面的 markdown 与内部链接都还要再走一遍完整解析
    #[test]
    fn body_is_parsed_by_the_whole_parser() {
        // 块内容里的 markdown 与内部链接都要生效
        let html = render("::quote\n  这是**强调**与[[目标]]\n");
        assert!(html.contains("<strong>强调</strong>"), "{html}");
        assert!(html.contains("wikilink"), "{html}");
    }

    /// 认不出的块不许静悄悄消失 —— 名字、参数摆出来，内容也留在框里给作者看
    #[test]
    fn unknown_block_renders_a_box() {
        let html = render("::还没有的块 标题=\"含 空格\" flag\n  内容一行\n");
        assert!(html.contains("template--unknown"), "{html}");
        assert!(html.contains("未知块"), "{html}");
        assert!(
            html.contains(r#"<code class="mdpp-template__name">还没有的块</code>"#),
            "{html}"
        );
        assert!(html.contains("标题=含 空格"), "{html}");
        assert!(html.contains("<pre><code>内容一行</code></pre>"), "{html}");
    }

    /// 注册表里有这个名字时，就**不是**未知块 —— 名字与渲染器由表说了算
    #[test]
    fn a_registered_name_beats_the_unknown_box() {
        let html = render("::note\n  正文\n");
        assert!(html.contains("callout"), "{html}");
        assert!(!html.contains("template--unknown"), "{html}");
    }

    /// 模板调用的头写坏了要**报错**，不能退回普通段落；正文仍收在错误框里
    #[test]
    fn a_template_call_with_a_bad_header_is_reported() {
        #[derive(Debug, Default)]
        struct Pages(HashMap<String, TemplatePage>);
        impl TemplateSource for Pages {
            fn page(&self, name: &str) -> Option<TemplatePage> {
                self.0.get(name).cloned()
            }
        }

        let mut pages = Pages::default();
        pages.0.insert(
            "卡片".to_string(),
            TemplatePage::Ready("CARD=[{{title}}]".to_string()),
        );
        let context = RenderContext::new().with_templates(Arc::new(pages));
        let mdpp = Mdpp::new();

        for source in [
            "::卡片 title=\"没关上\n  正文\n",
            "::卡片 title=1 title=2\n  正文\n",
            "::卡片 =x\n  正文\n",
        ] {
            let html = mdpp.render_with(source, &context);
            assert!(html.contains("template--problem"), "{source:?}：{html}");
            assert!(!html.contains("<p>::卡片"), "{source:?}：{html}");
            assert!(html.contains("正文"), "正文不该被吞：{source:?}：{html}");
        }
    }

    /// 引号名字、`template:` 前缀、`key = value` 这些正常写法照旧可用
    #[test]
    fn a_template_call_with_a_canonical_header_still_works() {
        #[derive(Debug, Default)]
        struct Pages(HashMap<String, TemplatePage>);
        impl TemplateSource for Pages {
            fn page(&self, name: &str) -> Option<TemplatePage> {
                self.0.get(name).cloned()
            }
        }

        let mut pages = Pages::default();
        pages.0.insert(
            "卡片".to_string(),
            TemplatePage::Ready("CARD=[{{title}}]".to_string()),
        );
        let context = RenderContext::new().with_templates(Arc::new(pages));
        let mdpp = Mdpp::new();

        for source in [
            "::卡片 title = \"甲 乙\"\n",
            "::卡片 title= \"甲 乙\"\n",
            "::卡片 title =\"甲 乙\"\n",
        ] {
            let html = mdpp.render_with(source, &context);
            assert!(html.contains("CARD=[甲 乙]"), "{source:?}：{html}");
        }

        // `template:` 不是库的语法：这一行仍是普通段落，和 `::卡片` 是两样东西
        let prefixed = mdpp.render_with("::template:卡片 title=\"前缀\"\n", &context);
        assert!(!prefixed.contains("CARD="), "{prefixed}");
        assert!(prefixed.contains("::template:卡片"), "{prefixed}");
    }

    /// **围栏代码块里的 `[标签]` 不是标签。**
    ///
    /// 分节正文必须缩进，围栏里的 `[x]` 也在正文缩进里，因此不会落到标签层。
    /// 钉的是**三个条件同时成立**：标签栏仍是一个、示例原样还在代码块里、围栏后面
    /// 的那句还留在原来那一节里。少了任一条都可能「看着对」。
    #[test]
    fn a_bracket_line_inside_a_fence_is_not_a_section_label() {
        let html = render(
            "::tabs\n  [甲]\n    正常条目\n\n    ```text\n      [这不是标签]\n      [group99]\n    ```\n\n    收尾一句\n",
        );
        // 标签栏仍是一个
        assert_eq!(html.matches("class=\"mdpp-tabs__tab").count(), 1, "{html}");
        // 两行示例原样留在代码块里 —— 注意它们**该出现**，所以不能断言全文没有它们
        assert!(html.contains("[这不是标签]"), "{html}");
        assert!(html.contains("[group99]"), "{html}");
        // 围栏后的那句还在原来那一节里（没被劈到别处去）
        assert!(html.contains("收尾一句"), "{html}");
        assert_eq!(
            html.matches("class=\"mdpp-tabs__panel").count(),
            1,
            "只有一节：{html}"
        );
    }

    /// 列表项里**紧跟在列表记号后面**开的围栏：`- ```text` 这一行不是以围栏
    /// 记号开头，旧的行首状态机会把它漏掉，然后把闭合那道当成开启，吞掉下一个标签。
    #[test]
    fn a_fence_opened_after_a_list_marker_does_not_swallow_the_next_label() {
        let html =
            render("::tabs\n  [甲]\n    - ```text\n      例子\n      ```\n  [丙]\n    丙内容\n");
        assert_eq!(
            html.matches("class=\"mdpp-tabs__tab").count(),
            2,
            "该有两节：{html}"
        );
        assert!(!html.contains("[丙]"), "不该把标签当正文：{html}");
        assert!(html.contains("丙内容"), "{html}");
    }

    /// 波浪围栏同样由正文缩进保护
    #[test]
    fn a_tilde_fence_protects_its_labels_too() {
        let html = render("::tabs\n  [甲]\n    ~~~text\n      [不是标签]\n    ~~~\n");
        assert_eq!(html.matches("class=\"mdpp-tabs__tab").count(), 1, "{html}");
        assert!(html.contains("[不是标签]"), "{html}");
    }

    /// **HTML 块里的 `[标签]` 也不是标签。**
    ///
    /// HTML 块的行缩进在分节正文里，`[这不是标签]` 不会落到标签层；这把
    /// 「HTML 块里的一行方括号」与「新的一节」分开。
    #[test]
    fn a_bracket_line_inside_an_html_block_is_not_a_section_label() {
        let html = render(
            "::tabs\n  [甲]\n    <div class=\"card\">\n      [这不是标签]\n    </div>\n\n    收尾一句\n",
        );
        assert_eq!(html.matches("class=\"mdpp-tabs__tab").count(), 1, "{html}");
        assert!(!html.contains("这不是标签</button>"), "{html}");
        assert!(html.contains("[这不是标签]"), "该原样留在 HTML 里：{html}");
        // 空行之后重新是普通内容，那一句该回原来那一节
        assert!(html.contains("收尾一句"), "{html}");
        assert_eq!(
            html.matches("class=\"mdpp-tabs__panel").count(),
            1,
            "{html}"
        );
    }
}

//! `mdpp render` —— 渲染文件，HTML 写到标准输出。
//!
//! 这一趟是「一个进程一件事」的活：读进来、渲染、写到 stdout。它**不知道**有没有项目，
//! 也不想知道 —— 整站的事在 [`crate::project`]。

use crate::context::{link_resolver, template_pages};
use crate::options::Render;
use crate::page;

use std::io::{Read, Write};

use mdpp::{Mdpp, RenderContext};
use tracing::{debug, info};

/// 没给语言时的 `<html lang>`。`--lang` 这一版还没有，用配置里那一份（站点模式）
pub const DEFAULT_LANG: &str = "zh-Hans";

pub fn run(options: &Render) -> Result<(), String> {
    if options.list_blocks {
        let mut out = std::io::stdout().lock();
        for name in mdpp::builtin_blocks() {
            let _ = writeln!(out, "{name}");
        }
        return Ok(());
    }

    let mut sources = read_sources(options)?;
    if sources.is_empty() {
        // 空的输入没什么可说的，但也不该报错 —— 管道里空了一段是常事
        sources.push((options.title_or(), String::new()));
    }

    let parser = build(options);
    let context = build_context(options)?;
    debug!(
        full = options.full,
        theme = ?options.theme,
        "配置就绪"
    );

    // `--css` / `--js` 附的那些：读进来放在默认的后面
    let extra_css = read_all(&options.css)?;
    let extra_js = read_all(&options.js)?;

    let mut out = std::io::stdout().lock();
    let mut total = Stats::default();
    let multiple = options.inputs.len() > 1;

    for (name, text) in &sources {
        info!(name, bytes = text.len(), "渲染");
        let html = parser.render_with(text, &context);
        total.add(name, text, &html);

        if multiple {
            // 多个输入时每个文件一段，用注释隔开 —— 不加的话几段 HTML 拼在一起看不出哪段是哪段
            let _ = writeln!(out, "<!-- {name} -->");
        }
        // 片段是常态（像 cmark 那样），`--full` 才套外壳 —— 外面套什么、内联什么，是界面
        // 那一层的事
        let chunk = if options.full {
            page::document(&page::Page {
                title: name,
                lang: DEFAULT_LANG,
                theme: options.theme.as_deref(),
                body: &html,
                extra_css: &extra_css,
                extra_js: &extra_js,
                // 一个文件就是整个页面：样式与脚本全内联，拷走还能看
                assets: page::Assets::Inline,
            })
        } else {
            html
        };
        out.write_all(chunk.as_bytes())
            .map_err(|error| format!("写标准输出失败：{error}"))?;
    }

    out.flush()
        .map_err(|error| format!("写标准输出失败：{error}"))?;
    if options.stats {
        eprint!("{total}");
    }
    Ok(())
}

/// 按选项拼出解析器
fn build(options: &Render) -> Mdpp {
    Mdpp::builder()
        .raw_html(options.unsafe_html)
        .typographer(options.typographer)
        .setext(options.setext)
        .indented_code(options.indented_code)
        .math(!options.no_math)
        .wikilink(!options.no_wikilink)
        .build()
}

/// 按选项拼出上下文（链接前缀 + 模板页目录）
fn build_context(options: &Render) -> Result<RenderContext, String> {
    let mut context = RenderContext::new();
    if let Some(resolver) = link_resolver(options.link_prefix.as_deref()) {
        context = context.with_links(resolver);
    }
    if let Some(dir) = &options.template_dir {
        context = context.with_templates(template_pages(dir)?);
    }
    Ok(context)
}

/// 把输入都读进来：`(显示用的名字, 正文)`。
///
/// 一次全读进内存而不是流式 —— 命令行的输入是一篇笔记，而 `--full` 那条路
/// 本来就要整篇拼成一个文档。
fn read_sources(options: &Render) -> Result<Vec<(String, String)>, String> {
    if options.inputs.is_empty() {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|error| format!("读标准输入失败：{error}"))?;
        return Ok(vec![(options.title_or(), text)]);
    }

    let mut sources = Vec::with_capacity(options.inputs.len());
    for path in &options.inputs {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("读不了 {}：{error}", path.display()))?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        sources.push((name, text));
    }
    Ok(sources)
}

/// 读几个附带文件（`--css` / `--js`）。读不了就说清是哪个。
fn read_all(paths: &[std::path::PathBuf]) -> Result<Vec<String>, String> {
    paths
        .iter()
        .map(|path| {
            std::fs::read_to_string(path)
                .map_err(|error| format!("读不了 {}：{error}", path.display()))
        })
        .collect()
}

/// `--stats` 那一行
#[derive(Default)]
struct Stats {
    files: usize,
    links: usize,
    unknown: usize,
    misused: usize,
    bytes_in: usize,
    bytes_out: usize,
}

impl Stats {
    fn add(&mut self, name: &str, source: &str, html: &str) {
        self.files += 1;
        self.bytes_in += source.len();
        self.bytes_out += html.len();
        // 只报**出问题的地方**：那几个类名由 `block::dispatch` 一处写死，不会过期。
        // 「块有几个」数不出来 —— 22 个块各写各的 class，要数就得把 22 个名字抄一份
        // 在这儿，而那份清单过期之后报出来的数字还是「像模���样」的，比不报更坏。
        self.unknown += html.matches("template--unknown").count();
        self.misused += html.matches("template--problem").count();
        self.links += html.matches("<a class=\"wikilink").count();
        let _ = name;
    }
}

impl std::fmt::Display for Stats {
    /// 末尾的换行由 `writeln!` 给
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} 份输入，{} 字节 → {} 字节 HTML；内部链接 {} 条，未知块 {} 个，误用 {} 个",
            self.files, self.bytes_in, self.bytes_out, self.links, self.unknown, self.misused
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::{Cli, Command};

    /// `render` 的参数：从顶层解析进去
    fn options_of(args: &[&str]) -> Render {
        use clap::Parser;
        match Cli::try_parse_from(std::iter::once("mdpp").chain(args.iter().copied()))
            .expect("参数该是好的")
            .command
        {
            Command::Render(render) => render,
            other => panic!("要的是 render，给的是 {other:?}"),
        }
    }

    /// 给了文件名就按文件读：`(显示用的名字, 正文)`。这一条顺带钉住了「显示用的名字
    /// 取文件名那一段」—— 它同时也是 `--full` 时的 `<title>` 默认值
    #[test]
    fn a_named_file_becomes_one_source() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let path = dir.path().join("甲.md");
        std::fs::write(&path, "正文").expect("写得下");

        let sources = read_sources(&options_of(&["render", path.to_str().unwrap()])).expect("读得动");
        assert_eq!(sources, vec![("甲.md".to_string(), "正文".to_string())]);
    }

    /// 不给文件名就是从标准输入读。这里只验那条路的**形状**（一个来源、标题取
    /// `title_or`），不去碰 stdin —— 那会挂住整个测试进程
    #[test]
    fn no_file_means_standard_input() {
        let options = options_of(&["render", "--title", "某篇"]);
        assert!(options.inputs.is_empty());
        assert_eq!(options.title_or(), "某篇");
    }

    /// 读不了的文件要说清**是哪个** —— 只说「读不了输入」的话，作者得自己一个个试
    #[test]
    fn an_unreadable_file_says_which_one() {
        let message = read_sources(&options_of(&["render", "/没有这个文件.md"])).unwrap_err();
        assert!(message.contains("没有这个文件.md"), "{message}");
    }

    #[test]
    fn extra_files_are_read_in_order() {
        let dir = tempfile::tempdir().expect("造得出临时目录");
        let a = dir.path().join("a.css");
        let b = dir.path().join("b.css");
        std::fs::write(&a, "/* a */").expect("写得下");
        std::fs::write(&b, "/* b */").expect("写得下");

        let css = read_all(&[a, b]).expect("读得动");
        assert_eq!(css, vec!["/* a */".to_string(), "/* b */".to_string()]);
    }

    /// 读不到附件要说清是哪个文件，而不是笼统一句「读不了」
    #[test]
    fn an_unreadable_extra_file_says_which_one() {
        let message = read_all(&[std::path::PathBuf::from("/没有这个.css")]).unwrap_err();
        assert!(message.contains("没有这个.css"), "{message}");
    }
}
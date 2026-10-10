// mdpp —— 把 Markdown++ 渲染成 HTML
//
// 输出默认是**片段**（像 `cmark` 那样）：那段 HTML 往哪去、外面套什么样式，是调用方的
// 事。`--full` 才输出一个完整文档（内嵌本包那份默认样式表）。

// 这三个模块只属于这个二进制，库那边一概不知道它们存在 —— 所以它们住在 `mdpp-cli`
// 这个包里，并且对 `mdpp::` 不可见（`pub(crate)`）。
mod context;
mod options;
mod page;
mod telemetry;

use context::{link_resolver, template_pages};
use options::{Cli, NAME};
use telemetry::install;

use std::io::{Read, Write};
use std::process::ExitCode;

use clap::Parser;
use mdpp::{Mdpp, RenderContext};
use tracing::{debug, info};

fn main() -> ExitCode {
    // clap 自己处理 `--help` / `--version` / 用法错误（后者走退出码 2，
    // 与「跑起来了但失败」的 1 区分开 —— cmark 也是这么分的）
    let options = match Cli::try_parse() {
        // 注意这里**什么都不往 stdout 打**：stdout 是产物要去的地方
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();
            return match err.kind() {
                clap::error::ErrorKind::DisplayHelp
                | clap::error::ErrorKind::DisplayVersion
                | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand => {
                    ExitCode::SUCCESS
                }
                _ => ExitCode::from(2),
            };
        }
    };

    // 订阅器在解析之后才装：否则连「这组参数是不是错的」都还没定，就已���打了一堆日志
    install(options.debug);

    match run(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{NAME}: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: &Cli) -> Result<(), String> {
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
            page::full_document(name, &html, options.theme.as_deref(), &extra_css, &extra_js)
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
fn build(options: &Cli) -> Mdpp {
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
fn build_context(options: &Cli) -> Result<RenderContext, String> {
    let mut context = RenderContext::new();
    if let Some(resolver) = link_resolver(options.link_prefix.as_deref()) {
        context = context.with_links(resolver);
    }
    if let Some(dir) = &options.template_dir {
        context = context.with_templates(template_pages(dir)?);
    }
    Ok(context)
}

/// 把输入都读进来：`(显示用��名字, 正文)`。
///
/// 一次全读进内存而不是流式 —— 命令行的输入是一篇笔记，而 `--full` 那条路
/// 本来就要整篇拼成一个文档。
fn read_sources(options: &Cli) -> Result<Vec<(String, String)>, String> {
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

// mdpp —— 把 Markdown++ 渲染成 HTML
//
// 输出默认是**片段**（像 `cmark` 那样）：那段 HTML 往哪去、外面套什么样式，
// 是调用方的事。`--full` 才输出一个完整文档（内嵌本库那份默认样式表）。

mod bin;

use std::io::{Read, Write};
use std::process::ExitCode;

use mdpp::{Mdpp, RenderContext};

use bin::{
    debug, help, info, install, link_resolver, template_pages, version, BadUsage, Options, Wanted,
    NAME,
};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    let options = match Options::parse(args) {
        Ok(Ok(options)) => options,
        Ok(Err(wanted)) => {
            let text = match wanted {
                Wanted::Help => help(),
                Wanted::Version => version(),
            };
            print!("{text}");
            return ExitCode::SUCCESS;
        }
        Err(BadUsage(message)) => {
            eprintln!("{NAME}: {message}");
            eprintln!("试试 `{NAME} --help`。");
            return ExitCode::from(2);
        }
    };

    // 订阅器在解析之后才装：否则连「这组参数是不是错的」都还没定，就已经打了一堆日志
    install(options.debug);

    match run(&options) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{NAME}: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(options: &Options) -> Result<(), String> {
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
        "配置就绪，开始渲染"
    );

    let mut out = std::io::stdout().lock();
    let mut total = Stats::default();
    let multiple = options.inputs.len() > 1;

    for (name, text) in &sources {
        info!(name, bytes = text.len(), "渲染这一个");
        let html = parser.render_with(text, &context);
        total.add(name, text, &html);

        if multiple {
            // 多个输入时每个文件一段，用注释隔开 —— 不加的话，几段 HTML 拼在一起
            // 看不出哪段是哪段
            let _ = writeln!(out, "<!-- {name} -->");
        }
        let chunk = if options.full {
            mdpp::assets::full_document(name, &html, options.theme.as_deref())
        } else {
            html
        };
        out.write_all(chunk.as_bytes())
            .map_err(|error| format!("写不出去：{error}"))?;
    }

    out.flush().map_err(|error| format!("写不出去：{error}"))?;
    if options.stats {
        eprint!("{total}");
    }
    Ok(())
}

/// 按选项拼出解析器
fn build(options: &Options) -> Mdpp {
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
fn build_context(options: &Options) -> Result<RenderContext, String> {
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
fn read_sources(options: &Options) -> Result<Vec<(String, String)>, String> {
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

/// `--stats` 那一行
#[derive(Default)]
struct Stats {
    files: usize,
    blocks: usize,
    links: usize,
    problems: usize,
    bytes_in: usize,
    bytes_out: usize,
}

impl Stats {
    fn add(&mut self, name: &str, source: &str, html: &str) {
        self.files += 1;
        self.bytes_in += source.len();
        self.bytes_out += html.len();
        // 数的是**容器**个数（`class="template …`），不是块数：
        // 「一个块里嵌了几个块」数不清，而嵌套的深浅也不是这里要报的事
        self.blocks += html.matches("class=\"template ").count();
        self.problems +=
            html.matches("template--unknown").count() + html.matches("template--problem").count();
        self.links += html.matches("<a class=\"wikilink").count();
        let _ = name;
    }
}

impl std::fmt::Display for Stats {
    /// 末尾的换行由 `writeln!` 给（不是 `write!` 里写死一个）—— 少那一个换行时
    /// shell 提示符会粘在这一行后面，而那种错只有真跑一次才看得见。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} 份输入（{} 字节）→ {} 字节 HTML；块 {} 个（其中报错的 {} 个），内部链接 {} 条",
            self.files, self.bytes_in, self.bytes_out, self.blocks, self.problems, self.links
        )
    }
}

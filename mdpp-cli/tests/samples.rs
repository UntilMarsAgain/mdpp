//! 示例文件**防腐烂**：`examples/` 下每一份都要能渲染，且渲染得一个错都没有。
//!
//! 断言的是**渲染结果里有没有那两个框**（`template--unknown` / `template--problem`），
//! 而不是比对 HTML 文本 —— 那两个类名由 `mdpp::block::dispatch` 一处写死，逐字符
//! 比对则会在任何一处无害的排版改动下全红。
//!
//! 走的是真的那个二进制（[`CARGO_BIN_EXE_mdpp`](env!("CARGO_BIN_EXE_mdpp"))），
//! 为了顺带把 `--template-dir` 那条路走一遍 —— 示例里的用户块要靠它。
//!
//! 打成包发布时这个目录不在包里，那时整条测试跳过。

use std::path::{Path, PathBuf};
use std::process::Command;

/// 仓库根（这个包在根下面一层）
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("包目录总有个上级")
        .to_path_buf()
}

fn examples_dir() -> Option<PathBuf> {
    let dir = root().join("examples");
    dir.is_dir().then_some(dir)
}

/// 列出 `examples/` 下的每一份 `.mdpp`（模板页不算 —— 那个由 `--template-dir` 读）
fn samples() -> Vec<PathBuf> {
    let Some(dir) = examples_dir() else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("examples/ 在")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "mdpp"))
        .collect();
    found.sort();
    found
}

fn render(sample: &Path) -> String {
    let templates = root().join("examples/templates");
    let output = Command::new(env!("CARGO_BIN_EXE_mdpp"))
        .arg("--link-prefix")
        .arg("/notes/")
        .arg("--template-dir")
        .arg(&templates)
        .arg(sample)
        .output()
        .expect("跑得动自己");

    assert!(
        output.status.success(),
        "{} 渲染失败：{}",
        sample.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("输出是 UTF-8")
}

#[test]
fn every_sample_renders_without_a_single_problem() {
    let samples = samples();
    if samples.is_empty() {
        eprintln!("examples/ 不在这里（打成包了？）—— 跳过");
        return;
    }
    assert!(
        samples.len() >= 2,
        "示例只剩 {:?} 了 —— 是不是被误删",
        samples
    );

    for sample in &samples {
        let html = render(sample);
        assert!(
            !html.contains("template--unknown"),
            "{} 里有未注册的块：\n{html}",
            sample.display()
        );
        assert!(
            !html.contains("template--problem"),
            "{} 里有误用的块：\n{html}",
            sample.display()
        );
    }
}

/// 巡览那份要把**每个**内置块都走到一次 —— 只断言「没错」不够：一段被截断的示例
/// 同样没错，而那样它就不再是巡览了。
#[test]
fn the_tour_visits_every_builtin_block() {
    let Some(dir) = examples_dir() else {
        eprintln!("examples/ 不在这里 —— 跳过");
        return;
    };
    let tour = dir.join("tour.mdpp");
    if !tour.is_file() {
        eprintln!("tour.mdpp 不在这里 —— 跳过");
        return;
    }

    let html = render(&tour);
    // 每个块留下的痕迹（不是块名，是它渲染出来的那个东西）
    let marks = [
        ("quote", "<blockquote"),
        ("code", r#"class="template-code""#),
        ("aside", r#"<aside class="aside">"#),
        ("fields", r#"<table class="fields">"#),
        ("banner", r#"class="banner"#),
        ("title", r#"class="title-block""#),
        ("tabs", r#"class="tabs__bar""#),
        // 类名是 `theme__part theme__part--dark` 两个，所以只认后半截
        ("theme", "theme__part--dark"),
        // navbox：认「左分组名、右内容」那两列，而不只是它渲染出了东西
        (
            "navbox",
            r#"<th class="navbox__group" scope="row">分类</th>"#,
        ),
        ("image", "<img "),
        ("video", "<video "),
        ("audio", "<audio "),
        ("css", "<style>"),
        ("html", r#"class="card""#),
        ("js", "<script"),
        ("mermaid", r#"class="mermaid""#),
        ("math", "math--display"),
        ("signature", r#"class="signature""#),
        ("note", "callout--note"),
        ("tip", "callout--tip"),
        ("warning", "callout--warning"),
        ("danger", "callout--danger"),
        ("error", "callout--error"),
        // 用户块（模板页）—— 要 `--template-dir` 才出得来。
        // 认的是**填进去的那个参数值**，不是渲染出来的标签：模板页展开后落成
        // `::banner` / `::tabs`，而那两个内置块在巡览别处也有，认标签就分不出
        // 「是模板页来的」还是「是内置块来的」。
        ("卡片", "<strong>一个模板页</strong>"),
        ("面板", "<p>另一个模板页</p>"),
    ];

    let missing: Vec<&str> = marks
        .iter()
        .filter(|(_, mark)| !html.contains(mark))
        .map(|(name, _)| *name)
        .collect();
    assert!(missing.is_empty(), "巡览里没走到：{missing:?}");
}

/// 链接那份要真的把 `[[…]]` 解析成带 `href` 的 —— 没有前缀时它落成不带 href 的
/// 写法，那份示例演示的正是「前缀」的作用。
#[test]
fn the_link_sample_actually_gives_out_hrefs() {
    let Some(dir) = examples_dir() else {
        return;
    };
    let sample = dir.join("links.mdpp");
    if !sample.is_file() {
        return;
    }

    let html = render(&sample);
    assert!(html.contains(r#"href="/notes/另一篇""#), "{html}");
    // 没闭合的那种仍然是字面文本，不能被解析掉
    assert!(html.contains("[[没闭合"), "{html}");
}

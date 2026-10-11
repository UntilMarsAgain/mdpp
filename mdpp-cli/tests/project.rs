//! **子命令**的端到端行为：走真的那个二进制（[`CARGO_BIN_EXE_mdpp`]）。
//!
//! 单元测试跑的是函数，这里跑的是**命令行**：退出码、消息去了哪条通道、文件落在哪。
//! 这三件事单元测试看不见，而它们恰恰是命令行最要紧的部分。
//!
//! 一个项目跑完整的一趟：`init` → `new` → `build`，不预先在测试里铺好目录 ——
//! 铺好的目录只会证明「文件已经在那儿」。

use std::path::Path;
use std::process::{Command, Output};

/// 跑一次 `mdpp`，子命令与参数都给全
fn mdpp(at: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_mdpp"))
        .current_dir(at)
        .args(args)
        .output()
        .expect("跑得动自己")
}

/// 造一个项目根（临时目录）。里面**什么都没有**，由各用例自己 init
fn empty_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("造得出临时目录")
}

/// 建一个项目出来，返回那个项目根
fn init(dir: &Path) -> std::path::PathBuf {
    let root = dir.join("站点");
    let out = mdpp(dir, &["init", "站点"]);
    assert!(out.status.success(), "init 失败：{out:?}");
    root
}

fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn init_new_build_is_a_whole_ride() {
    let dir = empty_dir();
    let root = init(dir.path());

    for path in ["mdpp.toml", "pages/index.mdpp", "templates/卡片.mdpp", ".gitignore"] {
        assert!(root.join(path).exists(), "少了 {path}");
    }
    assert!(root.join("build").is_dir(), "少了 build/");

    // `new` 建一页
    let out = mdpp(&root, &["new", "指南/安装"]);
    assert!(out.status.success(), "new 失败：{}", stderr_of(&out));
    assert!(root.join("pages/指南/安装.mdpp").is_file(), "那一页没建出来");

    // `build` 出站点
    let out = mdpp(&root, &["build"]);
    assert!(out.status.success(), "build 失败：{}", stderr_of(&out));
    assert!(root.join("build/index.html").is_file(), "首页没出");
    assert!(
        root.join("build/指南/安装/index.html").is_file(),
        "嵌套页没出"
    );
    // build 的消息一条都不该进 stdout —— 那条通道留给产物
    assert_eq!(stdout_of(&out), "", "build 不该往 stdout 写东西");
    assert!(stderr_of(&out).contains("build"), "{}", stderr_of(&out));
}

#[test]
fn a_page_links_to_another_one_and_a_missing_one_stays_red() {
    let dir = empty_dir();
    let root = init(dir.path());
    std::fs::write(
        root.join("pages/甲.mdpp"),
        "# 甲\n\n看 [[乙]]，也看 [[根本没有这一页]]。\n",
    )
    .expect("写得下");
    std::fs::write(root.join("pages/乙.mdpp"), "# 乙\n").expect("写得下");

    let out = mdpp(&root, &["build"]);
    assert!(out.status.success());
    let html = std::fs::read_to_string(root.join("build/甲/index.html")).expect("读得动");

    assert!(html.contains(r#"href="乙/""#), "站内链接没指到那一页：{html}");
    assert!(
        html.contains(r#"class="wikilink wikilink--missing""#),
        "指不到的那一页该是红链：{html}"
    );
    // 报告里逐条说清是哪一页的哪个目标
    let report = stderr_of(&out);
    assert!(report.contains("甲"), "{report}");
    assert!(report.contains("根本没有这一页"), "{report}");
}

/// `--strict` 把红链变成非零退出码 —— 这是给 CI 用的
#[test]
fn strict_fails_the_build_when_a_link_goes_nowhere() {
    let dir = empty_dir();
    let root = init(dir.path());
    std::fs::write(root.join("pages/甲.mdpp"), "# 甲\n\n[[还没写]]\n").expect("写得下");

    assert!(mdpp(&root, &["build"]).status.success(), "默认不该失败");

    let out = mdpp(&root, &["build", "--strict"]);
    assert!(!out.status.success(), "--strict 下红链该让 build 失败");
    assert!(stderr_of(&out).contains("红链"), "{}", stderr_of(&out));
}

/// 不是项目的目录里敲 build，要说清「这不是项目」并指向 init
#[test]
fn build_outside_a_project_says_so_and_points_at_init() {
    let dir = empty_dir();
    let out = mdpp(dir.path(), &["build"]);

    assert!(!out.status.success());
    let message = stderr_of(&out);
    assert!(message.contains("不是一个 mdpp 项目"), "{message}");
    assert!(message.contains("mdpp init"), "{message}");
}

#[test]
fn new_refuses_to_climb_out_of_pages() {
    let dir = empty_dir();
    let root = init(dir.path());

    let out = mdpp(&root, &["new", "../逃出"]);
    assert!(!out.status.success(), "页名不许跑出 pages/");
    assert!(stderr_of(&out).contains("页名"), "{}", stderr_of(&out));
}

/// 产物只有 HTML，`--stats` 与日志都不往 stdout 混
#[test]
fn render_writes_html_to_stdout_and_the_rest_to_stderr() {
    let dir = empty_dir();
    // 空的标准输入也算一份输入 —— 那不是「没有输入」
    let out = mdpp(dir.path(), &["render", "--stats"]);
    assert_eq!(stdout_of(&out), "", "空输入不该产出东西");
    assert!(stderr_of(&out).contains("1 份输入"), "{}", stderr_of(&out));

    let out = mdpp(dir.path(), &["render", "--full", "--title", "某篇"]);
    assert!(
        stdout_of(&out).starts_with("<!DOCTYPE html>"),
        "{}",
        stdout_of(&out)
    );
    assert_eq!(stderr_of(&out), "", "没有该报的东西就不该有输出");
}

/// 少了子命令时打印帮助并**成功**返回 —— 那是「还不知道该敲什么」，不是「用错了」。
/// 帮助走 stderr：`mdpp > 输出.html` 得到的该是一个空文件，而不是一页帮助。
#[test]
fn a_bare_invocation_prints_the_help() {
    let dir = empty_dir();
    let out = mdpp(dir.path(), &[]);

    assert!(out.status.success());
    assert_eq!(stdout_of(&out), "");
    assert!(stderr_of(&out).contains("子命令"), "{}", stderr_of(&out));
}

/// 敲错的子命令要报得像个人话，并带上「你是不是想写什么」
#[test]
fn an_unknown_subcommand_is_a_usage_error() {
    let dir = empty_dir();
    let out = mdpp(dir.path(), &["buidl"]);

    assert_eq!(out.status.code(), Some(2), "用法错是退出码 2");
    assert!(stderr_of(&out).contains("buidl"), "{}", stderr_of(&out));
}

/// `serve` 是个占位：明说没做，别让人以为站点坏了
#[test]
fn serve_says_it_is_not_implemented_yet() {
    let dir = empty_dir();
    let out = mdpp(dir.path(), &["serve"]);

    assert!(!out.status.success());
    let message = stderr_of(&out);
    assert!(message.contains("还没实现"), "{message}");
    assert!(message.contains("mdpp build"), "{message}");
}
// 把渲染结果变成一个能双击打开的文件。
//
//     cargo run --example standalone_page
//
// 库**不**管正文往哪去、外面套什么样式 —— 它给的是一段 HTML 片段。
// `assets::full_document` 是库里唯一「自己套壳」的地方，套的是那份默认样式表：
// 想换样式，复制 `assets::DEFAULT_CSS` 改，或者干脆自己写壳（那更常见）。
//
// 跑法：`cargo run --example standalone-page`（会在当前目录写出 out/）

use std::path::Path;

use mdpp::{assets, Mdpp};

const 标题: &str = "重逢笔记 · 一篇正文";

const 正文: &str = "\
# 卷首

这里是**markdown**，可以写列表：

- 甲
- 乙

::note title=\"提醒\"
  提示框里照样能写 markdown，也可以写 [[内部链接]]。

::tabs
  [读法一]
  照着念。
  [读法二]
  自己试。

::banner color=#0055a4
  深底自动配浅字

$$\\int_0^1 x^2 \\, dx = \\tfrac{1}{3}$$

::html
  <div style=\"border-left:3px solid #ccc;padding-left:1em\">
    <markdown>
    **HTML 里反过来写 markdown**，而 HTML 本身默认被净化过。
    </markdown>
  </div>

---

`::mermaid` 与 `::math` 渲染出的是**原文**加几个 `data-*` 提示：排版是前端的事
（KaTeX / mermaid 是 JS 库，库这一侧不引 JS 引擎）。取不到它们时看到的是原文，
而不是一片空白。
";

fn main() {
    let mdpp = Mdpp::new();
    let html = mdpp.render(正文);

    // `theme` 给 `Some` 时写 `<html data-mdpp-theme=…>`，于是 `::theme` 那两节
    // 由它挑，而不是由系统偏好挑
    let page = assets::full_document(标题, &html, Some("light"));

    let out = Path::new("out");
    if let Err(error) = std::fs::create_dir_all(out) {
        eprintln!("造不出 out/：{error}");
        std::process::exit(1);
    }
    let path = out.join("笔记.html");
    if let Err(error) = std::fs::write(&path, &page) {
        eprintln!("写不了 {}：{error}", path.display());
        std::process::exit(1);
    }

    println!("写了 {}（{} 字节）", path.display(), page.len());
    println!("把它拖进浏览器就能看。");
}

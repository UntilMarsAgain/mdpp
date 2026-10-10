// 把渲染结果变成一个能双击打开的文件。
//
//     cargo run -p mdpp --example standalone_page
//
// 库**不**管正文往哪去、外面套什么样式 —— 它给的是一段 HTML **片段**
// （`Mdpp::render`）与那份默认样式表（`mdpp::assets::DEFAULT_CSS`）。
// 「把片段套成一个页面」是**界面**那一层的事，所以这个例子自己动手套：
//
// - 想要开箱即用、还能点（选项卡切得动）的那份，用命令行：
//   `cargo run -p mdpp-cli -- --full 正文.mdpp`；
// - 想自己控制外壳的，就像这里这样，把 `DEFAULT_CSS` 内联进一个模板。

use std::path::Path;

use mdpp::Mdpp;
use mdpp::assets::DEFAULT_CSS;

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
    let html = Mdpp::new().render(正文);

    // 这一段是**本例自己写的**，不是库给的：库里没有「套外壳」这个函数，
    // 因为那是界面那一层的事。`theme` 写死成 light，于是 `::theme` 的两节
    // 由样式挑，而不是由系统偏好挑。
    let page = format!(
        "<!DOCTYPE html>\n\
         <html lang=\"zh-Hans\" data-mdpp-theme=\"light\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{标题}</title>\n\
         <style>\n{css}\n</style>\n\
         </head>\n\
         <body>\n\
         <article class=\"mdpp-content\">\n{html}\n</article>\n\
         </body>\n\
         </html>\n",
        标题 = 标题,
        css = DEFAULT_CSS,
        html = html,
    );

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
    println!("注意选项卡点不动 —— 这个外壳没接那段脚本。");
    println!("要一份能点的，用 `cargo run -p mdpp-cli -- --full …`，或者自己把");
    println!("`mdpp-cli/assets/mdpp.js` 引进去。");
}

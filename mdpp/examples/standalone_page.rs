// 把渲染结果变成一个能双击打开的文件。
//
//     cargo run -p mdpp --example standalone_page
//
// 演示的是「库给的是片段，外壳是你的事」：`Mdpp::render` 给的是 HTML **片段**，
// 样式表、脚本、`<html>` 这些本库一概不提供 —— 它是解析器，不是界面。
//
// 想要开箱即用、还能点的整页，用官方二进制：
//
// ```sh
// cargo run -p mdpp-cli -- --full 正文.mdpp
// ```
//
// 它内联的那套 CSS/JS 在 `mdpp-cli/assets/`，**同一个 MIT 许可，自取即可** ——
// 想改就复制一份去改，`--css 你的.css` / `--js 你的.js` 也能接在默认的后面。
//
// 这个例子自己写一份极简的外壳：它想说明的就是「没有样式表也能用，你自己决定」。

use std::path::Path;

use mdpp::Mdpp;

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

`::mermaid` 与 `::math` 渲染出的是**原文**加几个 `data-*` 提示：排版是界面那一层的
事（KaTeX / mermaid 是 JS 库）。取不到它们时看到的是原文，而不是一片空白。
";

/// 一份**极简**的样式：只够让上面那些块看起来还是块。
///
/// 认真排版的话去拿 `mdpp-cli/assets/` 那份 —— 它更全，而且可以被 `--css` 附带进来。
const 一点样式: &str = "\
body { margin: 0 auto; padding: 2rem 1.5rem; max-width: 44rem;
       font-family: system-ui, 'Noto Sans CJK SC', sans-serif; line-height: 1.7; }
a { color: #1f5fb0; }
pre { background: #f6f7f9; padding: .8rem 1rem; overflow-x: auto; }
.callout { background: #eaf1fb; border-left: 4px solid #4a7fd0; padding: .7rem 1rem; }
.banner { background: #f6f7f9; padding: .7rem 1rem; text-align: center; }
.tabs__bar { border-bottom: 2px solid #d8dce1; }
.tabs__tab { border: 0; background: none; padding: .4em .9em; cursor: pointer; }
.tabs__tab--on { font-weight: 700; border-bottom: 2px solid #2f6fd0; }
.template--unknown, .template--problem { border: 1px dashed #b4302a; padding: .7rem 1rem; }
";

fn main() {
    let html = Mdpp::new().render(正文);

    // 这一段是**本例自己写的**，不是库给的
    let page = format!(
        "<!DOCTYPE html>\n\
         <html lang=\"zh-Hans\">\n\
         <head>\n\
         <meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{标题}</title>\n\
         <style>\n{样式}\n</style>\n\
         </head>\n\
         <body>\n\
         {html}\n\
         </body>\n\
         </html>\n",
        标题 = 标题,
        样式 = 一点样式,
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
    println!();
    println!("这份外壳是自己写的，只够「看起来还是块」。");
    println!("官方那套更全，在 `mdpp-cli/assets/`（MIT，可自取）：");
    println!("  cargo run -p mdpp-cli -- --full 正文.mdpp        # 内联官方 CSS+JS，选项卡能点");
    println!("  mdpp --css 你的.css --js 你的.js --full …      # 接在默认的后面");
    println!("注意这一份选项卡点不动 —— 它没接那段脚本，那正是库不负责的事。");
}

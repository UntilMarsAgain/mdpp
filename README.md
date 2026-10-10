# mdpp —— Markdown++

把 [markdown-it](https://github.com/Markdown-it/markdown-it) 之上的块级方言
（`::名字 参数=值` 开一个块，缩进划边界）整理成一个库，外加一个像 `cmark` 那样的命令行。

```sh
cargo build --release
./target/release/mdpp 正文.md          # HTML 片段
./target/release/mdpp --full 正文.md   # 完整文档（内联默认样式与脚本）

cargo install --path mdpp-cli          # 装进 PATH（程序名仍是 mdpp）
```

## 两个包

| 包 | 是什么 |
|---|---|
| [`mdpp`](mdpp/) | 库。`markdown → HTML`，不知道文件系统、终端或仓库的任何事 |
| [`mdpp-cli`](mdpp-cli/) | 命令行。库的一个普通使用者，只用 `use mdpp::…` 那圈公开 API |

分家的依据是**依赖方向**：库不该知道有人要写命令行，反过来命令行也不该被库的
使用者拖进去。命令行里唯一给库的新增负担是 `path` + `version` 那一条依赖。

## 方言

```text
::名字 key=value key=value
  内容（缩进更深的算块内，遇到不更深的行就结束）
```

| 写法 | 是什么 |
|---|---|
| `::quote` `::aside` `::banner` `::title` `::signature` | 文字块，内容照常按 markdown 解析 |
| `::note` `::tip` `::warning` `::danger` `::error` | 提示框，形状一致，差别在记号与配色 |
| `::image` `::video` `::audio` | 摆一个文件；尺寸是**上限**，URL 只挡三种危险协议 |
| `::tabs` `::theme` | 按 `[标签]` 分节，各节各是一篇独立的小文档 |
| `::code` `::math` `::mermaid` `::css` `::html` `::js` | **不解析**，原文交给渲染器 |
| `::fields` | 逐行 `标签 \| 值` 的属性表 |
| `[[目标\|显示文字]]` | 内部链接，目标在前（MediaWiki 顺序） |
| `$…$` / `$$…$$` | 行内 / 独立一行的公式 |

`mdpp --list-blocks` 列出本版全部块名。`examples/tour.mdpp` 是当语法速查表用的巡览。

## 三个扩展点

### 一、块注册表

注册表是**普通数据**：不含解析器，也不含全局状态。两个 `Mdpp` 各带各的表。

```rust
use mdpp::{Mdpp, registry::{BlockRegistry, ContentMode}};

let mut registry = BlockRegistry::with_builtins();
registry.register_fn("问候", ContentMode::Markdown, |block, node, fmt| {
    fmt.open("p", &[("class", "greet".into())]);
    fmt.text(&format!("你好，{}", block.param("who").unwrap_or("世界")));
    fmt.contents(&node.children);
    fmt.close("p");
});

let mdpp = Mdpp::builder().registry(registry).build();
```

一项要说清渲染函数与**内容怎么处理**，后者不是可选项 —— 扫描器要在内容被解析
**之前**决定走哪条路，那时渲染函数还没被调用过：

| `ContentMode` | 内容 |
|---|---|
| `Markdown` | 交给整个解析器再解析一遍，于是块里能写列表、链接、还能再嵌块 |
| `Sections` | 按 `[标签]` 切成几节，各节各解析一遍 |
| `Raw` | 不解析，原文在 `block.body` 里 |

### 二、内部链接解析器

`resolve` 返回三种东西，对应三种目标性质：

| 返回 | 渲染成 |
|---|---|
| `Some(ResolvedLink::Link(target))` | 带 `href` 的 `<a>` |
| `Some(ResolvedLink::Node(f))` | 整个节点归 `f` 写，连 `<a>` 都由它决定 |
| `None`，**或压根没递解析器** | 红链：`<a class="wikilink wikilink--missing" data-missing="true">` |

```rust
use std::sync::Arc;
use mdpp::{Mdpp, RenderContext, link::{LinkResolver, LinkTarget, ResolvedLink}};

struct Route;
impl LinkResolver for Route {
    fn resolve(&self, target: &str) -> Option<ResolvedLink> {
        Some(ResolvedLink::Link(LinkTarget::new(format!("/notes/{target}"))))
    }
}

let context = RenderContext::new().with_links(Arc::new(Route));
let html = Mdpp::new().render_with("[[另一篇]]", &context);
```

三点约定：

- **`Link` / `Node` 是目标性质之别，不是口味之别。** `Node` 刻意不给 `href`：
  有了 `href` 浏览器就会自己走，调用方的单击处理再也轮不到，所以「点它会发生什么」
  必须是显式递进来的东西。
- **`None` 与「没递解析器」渲染成同一样东西。** 两者都是「调用方不知道 `[[某页]]`
  存不存在」，而渲染成一条看起来正常的链等于替它宣称这一页存在。
- **红链不带 `href`**，理由同第一条。要换掉它的样子有两条路：递
  `ResolvedLink::Node` 自己画，或用自己那份样式接 `wikilink--missing` /
  `data-missing="true"`。

实现方要保证：**同一段原文、同一个目标，每次给出同一个答案** —— 渲染是纯函数，
解析器若在两次渲染之间改主意，缓存下来的 HTML 就是错的。

### 三、模板页

```rust
use std::{collections::HashMap, sync::Arc};
use mdpp::{Mdpp, RenderContext, block::template::{TemplatePage, TemplateSource}};

#[derive(Debug, Default)]
struct MyPages(HashMap<String, TemplatePage>);

impl TemplateSource for MyPages {
    fn page(&self, name: &str) -> Option<TemplatePage> {
        // name 已归一过（小写、剥掉 template: 前缀）
        self.0.get(name).cloned()
    }
}

let context = RenderContext::new().with_templates(Arc::new(MyPages::default()));
let html = Mdpp::new().render_with("::卡片 标题=某\n", &context);
```

`Ready` 是正文，`Unreadable { reason }` 是「这一页在，但这一刻读不出来」。分开是因为
对作者这是两件事，混成一句「取不到」最难查；渲染出来也是两个不同的框。

页名的归一（`甲` 与 `template:甲` 是同一页）在库里做，实现方不必自己处理一遍。

### 这两个 trait 上，库不带任何实现

给一个现成的实现（一张 `HashMap`、一个文件目录）就等于把「东西存在哪」定成默认
答案，而真实使用方常常是数据库、搜索索引或按需解密的页 —— 要绕开那个类型，就得
重新实现一遍 trait。最简的实现十行，见上；`mdpp-cli/src/context.rs` 里是同样两行的
另一份。

`impl<F: Fn(&Node, &mut dyn Renderer)> NodeRenderer for F`（闭包当渲染器）看着像
例外，其实不是：那个给的是写法上的方便，里面没有「东西存在哪」的知识。

## 刻意关掉的

markdown-it 默认开着、本库默认关掉四样：

| 关掉 | 为什么 | 要开 |
|---|---|---|
| **raw HTML** | 渲染结果常常直接注入页面，开着等于把 XSS 入口交给正文作者 | `raw_html(true)` |
| **缩进代码块** | 缩进是 `::` 块的边界，两者会互相干扰 | `indented_code(true)` |
| **Setext 标题** | 与 `---` 撞：`标题\n---` 开着是标题、关着是段落加分隔线 | `setext(true)` |
| **排版替换** | typographer / smartquotes 不区分代码片段，会把代码里的 `--` 改成 `–` | `typographer(true)` |

## 并发

**`Mdpp` 既不是 `Send` 也不是 `Sync`。** markdown-it 的 `ext` 集合是 `RefCell`，
`parse` 期间真的读写它 —— 两个线程共用一个解析器是数据竞争，不是「慢一点」。

因此**一个线程建一个**。建一次是微秒级的事。渲染状态（链接解析器、模板页、
注册表那一趟）是线程局部的，各线程各渲各的，有测试钉着。

## `::html` 的净化

按 HTML5 规范解析后按**白名单**逐个节点重建，不是黑名单过滤：

- 白名单外的东西出不去（默认方向是关）；
- 输出是自己拼的，属性值与文本都重新转义；
- 与浏览器看的是同一棵树，所以畸形标签不会被两边理解成两样东西。

`<markdown>` 那几段先从原文里**原样取出**，原处放占位记号；余下照常解析净化，
拼字符串时碰到记号才把那段渲染一遍。不用「解析完再从树里取」是因为那等于让 HTML
解析器先把 markdown 当 HTML 过一遍。

## `--full`

片段是常态（像 `cmark` 那样），`--full` 输出一个内联了两样东西的完整文档：

| | |
|---|---|
| `mdpp-cli/assets/*.css` | 七个文件，14.8 KB |
| `mdpp-cli/assets/mdpp.js` | 1.9 KB，只做一件事：点一下就切 `::tabs` |

两者与库**同一个 MIT 许可，自取即可**。`--css 你的.css` / `--js 你的.js` 附加自己
那份，按给定次序接在默认的**后面**（同优先级时后者胜出）；两个选项都要求 `--full`。

**库不带这些。** 它是解析器，给的是 HTML 片段；样式、脚本、`<html>` 一概不提供。
`mdpp/examples/standalone_page.rs` 演示的就是这件事。

打印（Ctrl+P）时折叠的东西会展开：选项卡的每一节都出来并顶上带自己的名字，深浅色
两节都出来，信息栏取消浮动，链接后面写出地址。

## 日志

库**只发不装**。埋点按「作者大概知不知道」分档：`debug!` 是排查问题要看的（渲染
规模、没注册的块名、读不出来的模板页），`trace!` 逐块逐链接，默认关着，`warn!` 只给
嵌套超上限这种作者多半没意识到的事。`info!` 库不发。

`mdpp --debug` 装一个 DEBUG 级订阅器，写**标准错误**：

```sh
mdpp --debug 正文.md > 输出.html    # 输出.html 里没有一行日志
```

## 例子

| 在哪 | 是什么 |
|---|---|
| [`examples/`](examples/) | 用命令行跑的示例文件（`.mdpp`）：块的巡览、内部链接的写法、两个模板页 |
| [`mdpp/examples/`](mdpp/examples/) | 四个 Rust 例子，对着上面三个扩展点各来一份 |

示例文件是被测的：`mdpp-cli/tests/samples.rs` 每一份都渲染一遍，断言里面没有「未知
块」与「误用」的框。

```sh
cargo run -p mdpp-cli -- --link-prefix /notes/ -T examples/templates examples/tour.mdpp
```

## 开发

```sh
cargo fmt                # 提交前先跑
cargo test               # 库 153 + 4 文档、命令行 36、示例 3
cargo clippy --all-targets
cargo doc --open
```

rustfmt 的 `wrap_comments` 默认是 `false`，中文注释里手工对齐的缩进与列表原样留着。
clippy 没有 blanket `deny` —— 拦下来之后一行警告就发不了版，于是会开始加
`#[allow]`。

## 许可

MIT，版权属于 Until Mars Again。代码来自 `refind-note-demo`，那一版的著作权也是
同一个人。

## 结构

```text
Cargo.toml              只有 [workspace]：members 与 profile，没有 [package]

mdpp/                   库
├── examples/           四个 Rust 例子
└── src/
    ├── lib.rs
    ├── engine.rs       Mdpp：构建器、渲染入口、嵌套渲染
    ├── context.rs      渲染上下文（线程局部，每次调用一份）
    ├── registry.rs     块注册表、Block、ContentMode
    ├── telemetry.rs    发日志的宏全部从这儿取
    ├── link/mod.rs     LinkResolver 与 ResolvedLink
    ├── block/
    │   ├── mod.rs      `::` 方言的入口
    │   ├── scanner.rs  块级规则：认头、量缩进、按 ContentMode 处理内容
    │   ├── dispatch.rs 按名字分发；未知块与用法错误各一个框
    │   ├── expand.rs   用户块：模板页 → 填 {{}} → 解析
    │   ├── fill.rs     {{}} 填空、CSS 收窄、HTML 净化转发
    │   ├── html.rs     ::html 的白名单净化器与 <markdown> 回环
    │   ├── template.rs TemplateSource
    │   └── stdlib/     22 个内置块，按内容怎么处理分五组
    ├── syntax/         行内：math.rs、wikilink.rs
    ├── slug.rs         标题 id
    └── targets.rs      链接目标里的空格补成 %20

mdpp-cli/               命令行
├── assets/             官方那套 CSS/JS，MIT 可自取
├── tests/samples.rs    把 examples/ 每一份渲染一遍
└── src/
    ├── main.rs         入口：读写、拼装
    ├── page.rs         整页外壳（--full）：把 assets/ 内联
    ├── options.rs      clap 的参数定义与帮助文本
    ├── context.rs      链接前缀与模板页目录
    └── telemetry.rs    装订阅器（--debug）
```
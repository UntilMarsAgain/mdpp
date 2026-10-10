# mdpp —— Markdown++

把 [markdown-it](https://github.com/Markdown-it/markdown-it) 之上的那套块级方言
（`::名字 参数=值` 开一个块，缩进划边界）整理成一个能被别人接手的 Rust 库，
外加一个像 `cmark` 那样的命令行。

```sh
cargo build --release
./target/release/mdpp 正文.md          # HTML 片段
./target/release/mdpp --full 正文.md   # 完整文档（内嵌默认样式）
```

## 它是什么

一个包、两个 target：

| target | 路径 | 是什么 |
|---|---|---|
| 库 `mdpp` | `src/lib.rs` | `markdown → HTML`。**不知道文件系统、终端、仓库的任何事** |
| 二进制 `mdpp` | `src/main.rs` | 命令行。库唯一的消费者 |

命令行是个薄壳，它自己的零件（参数解析、模板页目录）在 `src/bin/` —— 那是它
**独占**的目录，`src/` 根是两个 target 共用的。（`Cargo.toml` 里那行
`autobins = false` 是配套的：不关掉的话 `src/bin/*.rs` 会被 cargo 当成
「这里还有一个二进制」。）

那条边界不用 workspace 来守 —— **rustc 自己就守着**：二进制里的代码要用库的东西
得写 `use mdpp::`，而库那边根本看不到二进制。为此多一个 `Cargo.toml`、多一层
目录是划不来的。

## 方言一览

```text
::名字 key=value key=value
  内容（缩进更深的是块内，遇到不更深的行就结束）
```

| 写法 | 是什么 |
|---|---|
| `::quote` `::aside` `::banner` `::title` `::signature` | 只写文字的块，内容照常按 markdown 解析 |
| `::note` `::tip` `::warning` `::danger` `::error` | 提示框，形状一样，差别在记号与配色 |
| `::image` `::video` `::audio` | 摆一个文件；尺寸是**上限**，协议只挡三种危险协议 |
| `::tabs` `::theme` | 按 `[标签]` 分节，各节各是一篇独立的小文档 |
| `::code` `::math` `::mermaid` `::css` `::html` `::js` | **不解析**，原文交给渲染器 |
| `::fields` | 逐行 `标签 | 值` 的属性表 |
| `[[目标\|显示文字]]` | 内部链接，目标在前（MediaWiki 顺序） |
| `$…$` / `$$…$$` | 行内 / 独立一行的公式 |

`mdpp --list-blocks` 列当前这一版带的全部块名。

## 三条边界

### 一、块注册表是普通数据

```rust
use mdpp::{registry::{BlockRegistry, ContentMode}, Mdpp};

let mut registry = BlockRegistry::with_builtins();
registry.register_fn("问候", ContentMode::Markdown, |block, node, fmt| {
    fmt.open("p", &[("class", "greet".into())]);
    fmt.text(&format!("你好，{}", block.param("who").unwrap_or("世界")));
    fmt.contents(&node.children);
    fmt.close("p");
});

let mdpp = Mdpp::builder().registry(registry).build();
```

注册一项要说清两件事：渲染函数，以及**内容怎么处理**：

| `ContentMode` | 内容 |
|---|---|
| `Markdown` | 交给整个解析器再解析一遍 —— 于是块里能写列表、链接、还能再嵌块 |
| `Sections` | 按 `[标签]` 切成几节，**各节各解析一遍** |
| `Raw` | **不解析**，原文在 `block.body` 里 |

第二件不是可选项：扫描器要在**内容被解析之前**决定走哪条路，那时渲染函数
还没被调用过。

`BlockRegistry` 不含解析器也不含全局状态 —— 两个 `Mdpp` 各带各的表，互不相干。

### 二、内部链接解析器是一个 trait

```rust
use mdpp::{
    link::{LinkResolver, LinkTarget, NodeRenderer, ResolvedLink},
    markdown_it::{Node, Renderer},
};

// 目标出得去 → 给一个 href
struct Route;
impl LinkResolver for Route {
    fn resolve(&self, target: &str) -> Option<ResolvedLink> {
        Some(ResolvedLink::Link(LinkTarget::new(format!("/notes/{target}"))))
    }
}

// 目标没有 href、但仍然是个链接 → 函数自己写整个节点
struct RedLink;
impl LinkResolver for RedLink {
    fn resolve(&self, _target: &str) -> Option<ResolvedLink> {
        Some(ResolvedLink::Node(Box::new(|node: &Node, fmt: &mut dyn Renderer| {
            fmt.open("a", &[("class", "wikilink".to_string())]);
            fmt.contents(&node.children);
            fmt.text(" ✗");
            fmt.close("a");
        })))
    }
}
```

第二种不给 `href` **不是缺陷而是要紧的设计**：`href` 一旦有，浏览器就会自己走，
使用方的单击处理根本轮不到。所以「点它会发生什么」必须是显式递进来的东西。

`None` 的含义是「认不出来」，那一段留作字面文本 —— 宁可什么都不承诺，
也不要给一个错的地址。

### 三、模板页也是一个 trait

```rust
use mdpp::{block::template::{TemplatePage, TemplateSource}, RenderContext};

#[derive(Debug)]
struct MyPages;
impl TemplateSource for MyPages {
    fn page(&self, name: &str) -> Option<TemplatePage> {
        // name 已经归一过（小写、剥掉了 template: 前缀）
        match name {
            "卡片" => Some(TemplatePage::Ready("::banner\n  {{标题}}\n".into())),
            _ => None,
        }
    }
}

let context = RenderContext::new().with_templates(std::sync::Arc::new(MyPages));
let html = Mdpp::new().render_with("::卡片 标题=某\n", &context);
```

`Ready` 是正文；`Unreadable { reason }` 是「这一页在，但这一刻读不出来」——
那两种情况对作者是两件事，混成一句「取不到」最难查，所以分成了两个变体，
渲染出来也是两个不同的框。

页名的归一（`甲` 与 `template:甲` 是同一页）在**库里**做，实现方不必自己处理一遍
—— 漏掉这一步的症状很隐蔽：`::甲` 能用、`src=template:甲` 却报「不存在」。

## 刻意关掉的

markdown-it 默认开着、本库默认关掉四样，每一条的代价都写在对应模块的模块头里：

| 关掉 | 为什么 |
|---|---|
| **raw HTML** | 渲染结果常常是被直接注入页面的（`innerHTML` 那套），开着等于把 XSS 入口交给正文作者。要开：`Mdpp::builder().raw_html(true)` |
| **缩进代码块** | 缩进在本库有别的用途 —— 它是 `::` 块的边界，两者会互相干扰 |
| **Setext 标题** | 它和 `---`（thematic break）撞在一起：`标题\n---` 开着是标题、关着是段落加分隔线 |
| **排版替换** | typographer / smartquotes 会把 `--accent` 变成 `–accent`、把 `"引号"` 变成 `“引号”`，而这两条规则**不区分代码片段** |

## 并发

**`Mdpp` 既不是 `Send` 也不是 `Sync`。** markdown-it 内部有 `RefCell`（它的 `ext`
集合），而 `parse` 期间真的会读写那个集合 —— 两个线程共用一个解析器是**数据竞争**，
不是「慢一点」。

于是：**一个线程建一个**。建一次是微秒级的事，比外面套一把锁划算，也比给一个
`!Sync` 的东西硬加 `unsafe impl Sync` 划算。

渲染状态（链接解析器、模板页、注册表那一趟）是线程局部的，各线程各渲各的
互不干扰 —— 有测试钉着（`the_resolver_does_not_leak_across_threads`）。

## `::html` 的净化

按 HTML5 规范解析后按**白名单**逐个节点重建，不是黑名单过滤：

- **只有白名单里的东西能活下来**。没想过的东西默认出不去。
- **输出是自己拼的**，属性值与文本都重新转义，不存在「原样穿过来了」。
- **理解与浏览器一致**：畸形标签按规范补成浏览器看到的那棵树。

`<markdown>` 那几段**先从原文里原样取出**（逐字节，一个字符不改），原处放一个
占位记号；余下照常解析净化；拼字符串时碰到记号就把那段真的渲染一遍。不用
「解析完再从树里取」是因为那等于让 HTML 解析器先把 markdown 当 HTML 过一遍 ——
`&` 会被解成实体、`<div>` 会被当成标签吃掉。

## 开发

```sh
cargo test           # 177 个测试（库 154、命令行 19、文档 4）
cargo clippy --all-targets
cargo doc --open
```

## 许可

MIT。版权属于 Until Mars Again。代码来自 `refind-note-demo`，那一版的著作权也是
同一个人。

## 结构

```
Cargo.toml
assets/mdpp.css              默认样式表（include_str! 进库）
src/
├── lib.rs                   库
├── engine.rs                Mdpp：构建器、渲染入口、嵌套渲染
├── context.rs               渲染上下文（线程局部，每次调用一份）
├── registry.rs              :: 块的注册表、Block、ContentMode
├── link/mod.rs              LinkResolver 与 ResolvedLink
├── block/
│   ├── mod.rs               `::` 方言的入口
│   ├── scanner.rs           块级规则：认头、量缩进、按 ContentMode 处理内容
│   ├── dispatch.rs          按名字分发；不认识/用法不对各一个框
│   ├── expand.rs            用户块：模板页 → 填 {{}} → 解析
│   ├── fill.rs              {{}} 填空、CSS 收窄、HTML 净化转发
│   ├── html.rs              ::html 的白名单净化器与 <markdown> 回环
│   ├── template.rs          TemplateSource：模板页从哪来
│   └── stdlib/              22 个内置块，按内容怎么处理分五组
├── syntax/                  行内：math.rs（$…$）、wikilink.rs（[[…]]）
├── slug.rs                  标题 id
├── targets.rs               链接目标里的空格补成 %20
├── assets.rs                内嵌样式表与完整文档
├── main.rs                  二进制入口（惯例位置）：读写、拼装
└── bin/                     **只有**二进制用的那几个文件
    ├── mod.rs
    ├── options.rs           手写的参数解析与帮助
    └── context.rs           链接前缀与模板页目录
```
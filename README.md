# mdpp —— Markdown++

给 [markdown-it](https://github.com/Markdown-it/markdown-it) 的一层扩展：在 markdown
之外加一套块级方言（`::名字 参数=值` 开一个块，缩进划边界），并把「内部链接指到哪、
模板页正文从哪来」这类问题留给使用方回答。

**它只做解析。** 产物是一段 HTML 片段 —— 外面套什么样式、点一下会发生什么，库一概
不提供，那是使用方的事。

## 演示

输入：

```text
# 十四门桥

::aside title="某年建成"
  与某桥并称。

::note
  这里可以写 **markdown** 与 [[内部链接]]。

::tabs
  [读法一]
    照着念。

  [读法二]
    自己试。
```

输出（略去部分属性，缩排有改动）：

```html
<h1 id="十四门桥">十四门桥</h1>
<aside class="aside"><p class="aside__title">某年建成</p>
<p>与某桥并称。</p>
</aside>
<div class="callout callout--note">
<p class="callout__title"><span class="callout__mark">ℹ</span>说明</p>
<p>这里可以写 <strong>markdown</strong> 与 <a class="wikilink" href="/notes/内部链接">内部链接</a>。</p>
</div>
<div class="tabs">
  <div class="tabs__bar" role="tablist">
    <button role="tab" aria-selected="true">读法一</button>
    <button role="tab" aria-selected="false">读法二</button>
  </div>
  ...
</div>
```

内置块 21 个，覆盖条件渲染、分节/行表格遍历、页面与模板包含条件、图片音视频、代码块、
公式、mermaid 图、导航框、原样文本、内联样式与原始 HTML；`columns`、`fields`、`fold`、`tabs`、`theme`、
`indent`、`banner`、`aside`、`quote`、`title` 等展示块由标准库模板提供。完整的巡览见
[`examples/tour.mdpp`](examples/tour.mdpp)，内置块清单见 `mdpp render --list-blocks`。

## 命令行

命令分两拨。**文件**那一拨把一个文件渲染成 HTML 往标准输出写，像 `cmark` 那样；
**项目**那一拨把一个目录里的许多页渲染成一个可静态部署的站点。

```sh
cargo build --release

# 文件：HTML 片段
./target/release/mdpp render 正文.md

# 文件：一个能双击打开的整页，内联默认样式与脚本
./target/release/mdpp render --full 正文.md > 输出.html
./target/release/mdpp render --full 正文.md -o 输出.html   # 直接写文件，覆盖旧文件

# 项目
mdpp init 我的站点          # 建骨架：mdpp.toml、pages/、templates/、build/
cd 我的站点
mdpp new 指南/安装           # 在 pages/ 下新建一页
mdpp build                  # 扫 pages/，出站点到 build/
mdpp serve                  # 本地预览，改一个字浏览器自己刷新

cargo install --path mdpp-cli   # 装进 PATH（程序名仍是 mdpp）
```

`--help` 有全部选项。加 `--full` 得到的页面可以直接双击打开，选项卡可点、打印时
折叠的内容会展开。

`-C, --dir <目录>` 先切到那个目录再干活（`make -C` / `git -C` 那个 `-C`）：往上找
`mdpp.toml` 的起点、`render` 的输入文件与 `--css` / `--js`、`init` 不给目录时的默认
目标，都从它算 —— `mdpp -C 我的站点 build` 与 `cd 我的站点 && mdpp build` 一样。

**产物默认走标准输出，其余消息走标准错误** —— 于是 `mdpp render 正文.md > 输出.html`
出来的文件里只有 HTML。`-o, --output <文件>` 可以直接写文件，已有文件会被覆盖；
输入会在打开输出之前读完，`render 甲.html -o 甲.html` 也不会截断输入。

### 项目

一个项目就是一个目录：

```text
我的站点/
  mdpp.toml     站点叫什么、东西都在哪
  pages/        正文，一页一个文件；子目录就是路径
  templates/    模板页，::名字 从这里取
  build/        产物，不进版本库
```

`mdpp init` 把 [`mdpp-cli/site/`](mdpp-cli/site/) 那个骨架目录整个拷过去，**目标目录
得是空的** —— 里面有东西（含隐藏文件）就报错退出，不覆盖也不合并。想改新项目长什么样，
改那个目录里的文件就好：它和 `assets/` 一样是在编译期整个嵌进二进制的。

`mdpp build` 递归扫 `pages/`（`.md` 与 `.mdpp` 都算页面），每一页渲染成一个整页，
落在 `build/<路径>/index.html`。页面从 `assets/` 外链样式与脚本，那份资源由 `build`
从二进制里拷出来 —— 几十个页面共用一份，不必每页里都塞一遍。站内链接是**相对**的，
所以整个 `build/` 拷到任何地方、挂在子路径下，链接都还对。

构建是**整目录替换**：先出到同级的临时目录，全部成功才换名；中途失败时旧站点一个
字都不动，上一次留下、这一版不再生成的文件也不会继续留在产物里。`--clean` 保留为
兼容开关，现在不再改变行为。`--jobs N` 控制并行渲染的线程数（`0` = 按 CPU 核数，
`1` = 单线程）；同一份项目用不同并行度出的产物逐字节一致。`build_dir` 必须位于项目
目录之内，指向项目外会在删除/替换任何东西之前报错。

内部链接 `[[目标]]` 在这里才真正有意义：`pages/` 就是全部的真相，指得到就带上地址，
指不到就是一条**红链**（不带 `href`，一眼看得出），并在 build 结束时逐条报出来。
`mdpp build --strict` 让红链与写坏的块变成非零退出码，好挂进 CI。

站点页面的外壳照着 [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/)
那个样子来的 —— 顶上那条、正中一列、底下收一条 —— 但有几处**故意不一样**：那一列是一张
圆角卡片摆在浅一号的底上（mkdocs 那边是白底平铺）、顶栏是 `sticky` 而不是 `fixed`、
底栏只有一句话加一句「由 mdpp 生成」。

顶上那一条（站点名那一行与它下面的导航）是**一整块有色的底**：底色由 `theme_color`
给（不写就是默认的蓝），压在它上面的字由它当场算 —— 亮底配近黑字、暗底配白字，深色下
再自动压暗一档（亮块在暗页面上是一条刺眼的亮带）。版式这一层写死，没有开关；其余配色
仍取自 `mdpp.css` 的那组变量。

顶栏下面还有一行**导航栏**（`nav` 配的那几个链接，mkdocs-material 的 tabs 那个位置）：
一排平铺的**可点链接**（不是按钮），当前页那一个不另给颜色（只多 `aria-current`，
那是给读屏软件的）。
`toc` 那一页（骨架里默认 `contents`）是 build **额外生成**的「页面清单」——
把全站每一页列成链接，导航栏末尾自动补一个按钮指过去。想关掉就 `toc = ""`。

顶栏与底栏上那几句话由 `mdpp.toml` 给，**全是单行字符串** —— 与 mkdocs 的
`site_name` / `site_description` / `copyright` / `repo_url` 是一回事：

```toml
site_name = "桥梁笔记"                        # 顶栏左边
theme_color = "#2f6fd0"                      # 顶栏那块底的颜色；不写就是默认的蓝
site_description = "十四座桥，与它们的记述"    # 顶栏右边，也进 <meta name="description">
base_url = "https://x.github.io/repo/"        # 站点在网上的地址；配了才有 canonical
repo_url = "https://example.com/notes"        # 顶栏右端那个链接（只认 http/https）
footer = "© 某人"                             # 底栏那句；空的就是没有底栏
favicon = "assets/icon.svg"                   # 标签页上那个小图标；相对项目根
logo = "assets/icon.svg"                      # 顶栏里、站点名旁边那个图标；同上
override_dir = "override"                     # 覆盖层：build 后把这里的东西原样拷到产物里
toc = "contents"                              # 目录页的名字（键）；空串就不生成目录页
nav = ["首页", { "指南" = "指南/入门" }]        # 导航栏：顶栏下面那一行链接
max_template_depth = 64                       # 用户模板最多嵌套层数
```

**导航栏**：`nav` 给的那一行链接，顶栏下面、mkdocs-material 的 tabs 那个位置。
每一项要么是一个字符串（显示文字 = 页名，按 `[[名字]]` 那套规则找：键名、或全站唯一
的文件名），要么是一个单项表（显示文字 → 站内页的键，`index` 是首页）。当前页那
一项只多 `aria-current="page"`（语义，不改样式）。不写就没有导航栏。

**目录页**：`toc`（骨架里默认 `contents`）是生成的那一页「页面清单」的**键** —— `mdpp build`
会额外出一页，把全站每一页列成链接，并在导航栏末尾自动补一个按钮指过去（作者已经
在 `nav` 里指过它就不补）。写空串（`toc = ""`）就不生成。

`base_url` 只回答「你在网上是哪一个」那一个问题：配了，每一页就多一条
`<link rel="canonical">`。**页内链接一律仍是相对的** —— 相对地址让整站能拷到任何
地方、挂在任意子路径下，而 canonical 是给搜索引擎看的。本地预览不必写它。
`footer` 会按 mdpp 语法渲染，所以底栏里可以写链接、`::note` 之类的块。

正文里**引用的本地文件会被拷进 `build/assets/`**，地址改成指到那儿去 —— 产物是
目录式的，而页面自己比源文件深了一层，正文里那个相对地址到了产物里会指错地方：

```markdown
![桥](桥.png)          <!-- pages/指南/桥.png → build/assets/指南/桥.png -->
```

只拷**被引用**的那些；引了而 `pages/` 里没有的会逐条报出来（`--strict` 下算失败）。
跑出 `pages/` 的不拷 —— 那是一条 `../../../etc/passwd`。

产物根上总有一个空的 `.nojekyll`：GitHub Pages 少了它就会跑 Jekyll，而 Jekyll 把 `_`
开头的目录当内部数据结构**静默丢掉** —— `pages/_草稿/某页.mdpp` 出了页面，推上去
却 404，而且没有任何报错。别的静态托管不认这个文件。

**覆盖层目录**：`override_dir`（默认 `override/`）。`mdpp build` 完成后，把这个目录里的
东西原样拷到产物里，重复就覆盖。可以往里放 `CNAME`（GitHub Pages 自定义域名）、
自定义 `robots.txt`、`sitemap.xml`，或者把某页换成完全手写的 HTML。配个别的名字也行。

## 本地预览

```sh
mdpp serve            # http://127.0.0.1:8080/
mdpp serve -p 9000    # 换端口；-p 0 让系统挑一个空闲的
```

改一个字，浏览器自己就刷新了 —— `pages/`、`templates/`、`mdpp.toml`，
外加配置里指的那几个文件（`extra_css` / `extra_js` / `favicon`）都在盯着。

预览出到**临时目录**，不去动你的 `build/`：预览到一半的东西不该被当成产物。
要的是产物就用 `mdpp build`。`Ctrl-C` 停。

## 当库用

```rust
let html = mdpp::Mdpp::new().render("::note\n  这里可以写 **markdown**。\n");
```

`Mdpp` 是 `Send + Sync`：构建一次，可以被多个线程共享，每页各渲各的。需要
内部链接就实现 `LinkResolver`，需要模板页就实现 `TemplateSource`，分别交给
`RenderContext::with_links` / `with_templates`。模板嵌套上限默认 64，需要改就用
`RenderContext::with_max_template_depth`。

要**不止填空**，有三条路：

```rust
// 1. 解析成 AST，改完再渲染
let mdpp = mdpp::Mdpp::new();
let mut root = mdpp.parse("# 标题\n");
root.children[0].attrs.push(("class", "mine".to_string()));
let html = mdpp.render_node(&root, &mdpp::RenderContext::new());

// 2. 自定义块按实例选内容模式（同一个块有时 raw、有时 markdown）
let mut registry = mdpp::BlockRegistry::with_builtins();
let spec = mdpp::BlockSpec::new("切换", mdpp::ContentMode::Markdown, |block: &mdpp::Block, node: &mdpp::markdown_it::Node, fmt: &mut dyn mdpp::markdown_it::Renderer| {
    if block.has("raw") { fmt.text(&block.body); } else { fmt.contents(&node.children); }
})
.with_mode_fn(|block| {
    if block.has("raw") { mdpp::ContentMode::Raw } else { mdpp::ContentMode::Markdown }
});
registry.register(spec);

// 3. 自定义块登记自己的 CSS / JS
use mdpp::{AssetKind, AssetUse, use_asset};
match use_asset(AssetKind::Css, ".mine { color: red; }") {
    AssetUse::Inline(css) => { /* 单页：写进 <style> */ }
    AssetUse::Linked(url) => { /* 站点：写 <link href=url> */ }
    AssetUse::Deferred(asset) => { /* 变体作用域：写 asset.html() */ }
    AssetUse::Duplicate => {}
}
```

### 用户模板与 `::if`

`templates/<名字>.mdpp` 是一页用户模板：正文里写 `::名字 参数=值` 就会把那一页
填好再解析。模板正文里 `{{参数}}` / `{{body}}` 是**早绑定**文本插值，缺了必填参数
会报 `template--problem`；写成 `{{?参数}}` 就是可选参数，缺失时填空串。参数位置上的
`{{@参数}}` 是**晚绑定**引用，留到块头分词之后再取值，所以值里有空格、引号、换行
也不会破坏参数；`{{?@参数}}` 是它的可选形式。正文里写 `{{@参数}}` 不会填充，正文
插值要用 `{{参数}}`。

模板调用的头按严格语法检查：等号两侧的空格一律忽略（`key=value`、`key = value`、
`key= value` 同义）；同名参数、空参数名、以 `@` 开头的参数名会报错。`::template:名字`
不再由库特殊处理：它是普通段落；命名空间若需要，由调用方自己识别。

模板嵌套上限默认 64：`render --max-template-depth N` 改单文件模式，站点模式在
`mdpp.toml` 里写 `max_template_depth = N`。

库自带的标准库模板在 `mdpp` 包里，编译期嵌入，不需要项目里再放一份；默认
`RenderContext` 已经带上，项目 `templates/` 里的同名页优先覆盖。目前从 Rust 实用块
迁入的是 `small` / `title` / `quote` / `signature` / `flatlist` / `banner` /
`aside` / `note` / `tip` / `warning` / `danger` / `error` / `columns` / `fields` / `fold` /
`theme` / `tabs` / `indent`；`callout` 是提示框共用的底模板。`::card`、`::panel`、`::main`、`::deprecated`、
`::grammar`、`::example` 属于示例或文档项目，不是库标准库。

提示框可以写 `collapsible`（展开）或 `collapsible=closed`（默认折着）变成可折叠的
`<details>`；不写或写 `collapsible=off` 仍是原来的静态形状。它由 `::disclose` 原语
实现，`fold` 也走同一个原语：

```mdpp
::note collapsible
  点标题可以折起来。

::tip collapsible=closed title=先折着
  打开之后才看到的内容。
```

基础包装块用 `::element`（替代原来的 `::class`）、`::surface`（受控的底色/字色/
尺寸盒子）和 `::text`（正文原样输出，不解析 markdown），外围模板可以由它们组合出来。
`::surface` 的 `class=` 归调用者：写了就按原样用，不再自动追加 `surface` 或
`surface--*`；高度、圆角与 `mini` 会落成行内样式。`tag=auto` 会按正文形状选
`<p>` 或 `<div>`。裸 flag `key` 等价于 `key=on`，显式 `key=` 才是空值。

分节块（`tabs` / `theme` / `columns` / `navbox` / `if` / `each`）有一条共同规则：
`[标签]` 在块的基础缩进上，**正文必须再缩进一层**；标签层上不允许出现没有标签头的
正文。`theme` 的「两边都显示」要显式写成 `[both]`（或 `[all]`），不再用第一个标签
之前的正文表示。

```mdpp
::if condition-exist={{@body}}
  [then]
    你好，{{body}}！
  [else]
    你好！
```

条件先支持这几个命名测试：

- `condition-exist={{@名字}}`：存在且非空白；
- `condition-defined={{@名字}}`：存在即可（裸 flag 也算）；
- `condition-empty={{@名字}}`：不存在或空白；
- `condition-equals={{@名字}} value=字面量`：去首尾空白后相等；
- `condition-truthy={{@名字}}`：不是空串、`0`、`false`、`no`、`off`；
- `condition-yes={{@名字}}`：去空白、忽略大小写后是 `yes` / `on` / `true` / `1`；
- `condition-no={{@名字}}`：去空白、忽略大小写后是 `no` / `off` / `false` / `0`；
- `condition-not`：反转最后结果。

`[then]` 必填，`[else]` 可选；多个条件用嵌套 `::if`。分节内容一律缩进在
`[标签]` 之下，标签之间不再夹匿名正文；`{{body}}` 作为分支内容时也放在那一层
缩进里，用户正文里的一行 `[else]` 因此不会被外层误认成分节。行内模板调用
还没做，块级 `::名字` 是当前唯一入口。

### 页面与模板的包含条件

两个透明容器，不产生自己的 DOM：

- `::noinclude`：页面**直接渲染**时输出；作为模板被 `::名字` 引入时隐藏。
- `::includeonly`：反过来，只在作为模板引入时输出。

判据是当前有没有模板作用域。所以模板页直接打开时看到 `noinclude`，被别的页面
调用时看到 `includeonly`；模板里再嵌套模板也仍然算“被引入”。

```mdpp
::noinclude
  这段只在模板页本身出现。
::includeonly
  这段只在别人 `::模板名` 时出现。
```

### 模板遍历 `::each`

`::each` 是写模板页时用的遍历原语。模板页里把「每一节怎么渲染」放在
`[__template__]` 下面，把 caller 的正文放在 `[__data__]` 下面：

```mdpp
::each
  [__template__]
    ::element class=columns__col
      ::text
        {{@item.label}}
      {{@item.body}}
  [__data__]
    {{body}}
```

每一项用 `{{@item.label}}` / `{{@item.body}}` / `{{@item.index}}` /
`{{@item.first}}` / `{{@item.last}}` 取数据；`as=` 可以改名，例如
`::each as=col` 后写 `{{@col.label}}`。`{{@body}}` 仍然表示 caller 的完整正文，
不会和 `{{@item.body}}` 冲突。

`only=甲,乙` / `skip=above,below` 过滤标签，`required` 让过滤后为空时报用法问题；
`name=` / `empty-why=` 可以改诊断框里的名字与空列表提示。

### 行表格 `::rows`

`::rows` 是 `fields` 这类「每行若干格」数据的遍历原语。模板区用
`[__template__]`，数据行缩进在 `[__data__]` 下面：

```mdpp
::rows
  [__template__]
    ::text
      {{@row.0}} / {{@row.1}}
  [__data__]
    甲|乙
    丙|丁
```

`{{@row.0}}` / `{{@row.1}}` … 取单元格；`row.count` 是这一行几格，
`row.index` / `row.first` / `row.last` 是行位置；缺列可用 `{{?@row.N}}`。
`max=2` 只拆第一道 `|`，剩下的 `|` 留在第二格；`fields` 模板就是这么保持
「标签 | 值」语义的。`Block::rows()` 把这行解析能力也开放给自定义块。

### 变体组与延迟资源

`::variants` 是变体组的原语：一组选项，每个选项可以带自己的
`::css` / `::js` / `::html js`。渲染某个选项时进入它的作用域，资源就会变成
`AssetUse::Deferred`，由页面脚本在选项激活时挂载。`::tabs` 与 `::theme` 是它上面
的两层标准库模板：

```rust
use mdpp::variants::{Control, Group};

let group = Group::new("mine", Control::Choice)
    .option("0", "甲")
    .option("1", "乙");
assert_eq!(group.variants().len(), 2);
```

自定义块只需要把 `container_attrs()` 挂到容器、`option_attrs()` 挂到控件、
`panel_attrs()` 挂到面板，并在渲染选项内容时调用
`group.with_option(value, || ...)`。内置的 `::variants` 把分节接到这套 API 上，
`::tabs` / `::theme` 再由标准库模板给它类名与文案；完整 DOM 协议见 `mdpp::variants`
的文档。

所有脚本都带 `data-mdpp-version` 与 `data-mdpp-mode`；延迟脚本还带
`data-mdpp-group` / `data-mdpp-value` / `data-mdpp-key` / `data-mdpp-run`，
脚本内可用 `document.currentScript.dataset` 读取，或用
`window.mdppVariants.hasRun(key)` / `runCount(key)` / `markRun(key)` 判断/标记。
默认每次激活执行一次；`::js once` 或 `use_asset_once` 可以改成只执行一次。

动态加载只做 DOM 挂载：`render --full` 把内容内联在 `<template>` 里，`build`
把资源写成 `assets/` 下的静态文件。两条路都不需要网络接口，也没有运行时后端。

写块、模板时常用的填空、分节、净化、诊断框都在 `mdpp::author` 里；具体块的
DOM（`.tabs`、`.callout` …）不属于扩展契约，要复用分节数据就用
`mdpp::author::sections`，要让现成交互脚本认领就照浏览器钩子表那套 `class` /
`data-*` 写。AST 是 markdown-it 的 `Node`（本库重导出了 `markdown_it`），
`parse` 与 `render_node` 应当用同一个 `Mdpp`。

## 许可

MIT
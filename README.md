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

块一共 24 个，覆盖引用、提示框、信息栏、选项卡、深浅色主题、水平分栏、图片音视频、
代码块、公式、mermaid 图、导航框、内联样式与原始 HTML。完整的巡览见
[`examples/tour.mdpp`](examples/tour.mdpp)，块名清单见 `mdpp render --list-blocks`。

## 命令行

命令分两拨。**文件**那一拨把一个文件渲染成 HTML 往标准输出写，像 `cmark` 那样；
**项目**那一拨把一个目录里的许多页渲染成一个可静态部署的站点。

```sh
cargo build --release

# 文件：HTML 片段
./target/release/mdpp render 正文.md

# 文件：一个能双击打开的整页，内联默认样式与脚本
./target/release/mdpp render --full 正文.md > 输出.html

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

**产物走标准输出，其余消息走标准错误** —— 于是 `mdpp render 正文.md > 输出.html`
出来的文件里只有 HTML。

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

内部链接 `[[目标]]` 在这里才真正有意义：`pages/` 就是全部的真相，指得到就带上地址，
指不到就是一条**红链**（不带 `href`，一眼看得出），并在 build 结束时逐条报出来。
`mdpp build --strict` 让红链与写坏的块变成非零退出码，好挂进 CI。

站点页面的外壳照着 [Material for MkDocs](https://squidfunk.github.io/mkdocs-material/)
那个样子来的 —— 顶上那条、正中一列、底下收一条 —— 但有几处**故意不一样**：那一列是一张
圆角卡片摆在浅一号的底上（mkdocs 那边是白底平铺）、顶栏是 `sticky` 而不是 `fixed`、
底栏只有一句话加一句「由 mdpp 生成」。这一层写死，没有开关；配色仍取自 `base.css`
的那组变量。

顶栏下面还有一行**导航栏**（`nav` 配的那几个链接，mkdocs-material 的 tabs 那个位置）：
当前页那一项高亮。`toc` 那一页（默认「目录」）是 build **额外生成**的「页面清单」——
把全站每一页列成链接，导航栏末尾自动补一个按钮指过去。想关掉就 `toc = ""`。

顶栏与底栏上那几句话由 `mdpp.toml` 给，**全是单行字符串** —— 与 mkdocs 的
`site_name` / `site_description` / `copyright` / `repo_url` 是一回事：

```toml
site_name = "桥梁笔记"                        # 顶栏左边
site_description = "十四座桥，与它们的记述"    # 顶栏右边，也进 <meta name="description">
site_url = "https://x.github.io/repo/"        # 站点在网上的地址；配了才有 canonical
repo_url = "https://example.com/notes"        # 顶栏右端那个链接（只认 http/https）
copyright = "© 某人"                          # 底栏那句；空的就是没有底栏
favicon = "assets/icon.svg"                   # 标签页上那个小图标；相对项目根
override_dir = "override"                     # 覆盖层：build 后把这里的东西原样拷到产物里
toc = "目录"                                  # 目录页的名字（键）；空串就不生成目录页
nav = ["首页", { "指南" = "指南/入门" }]        # 导航栏：顶栏下面那一行链接
```

**导航栏**：`nav` 给的那一行链接，顶栏下面、mkdocs-material 的 tabs 那个位置。
每一项要么是一个字符串（显示文字 = 页名，按 `[[名字]]` 那套规则找：键名、或全站唯一
的文件名），要么是一个单项表（显示文字 → 站内页的键，`index` 是首页）。当前页那
一项高亮。不写就没有导航栏。

**目录页**：`toc`（默认「目录」）是生成的那一页「页面清单」的**键** —— `mdpp build`
会额外出一页，把全站每一页列成链接，并在导航栏末尾自动补一个按钮指过去（作者已经
在 `nav` 里指过它就不补）。写空串（`toc = ""`）就不生成。

`site_url` 只回答「你在网上是哪一个」那一个问题：配了，每一页就多一条
`<link rel="canonical">`。**页内链接一律仍是相对的** —— 相对地址让整站能拷到任何
地方、挂在任意子路径下，而 canonical 是给搜索引擎看的。本地预览不必写它。

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

## 许可

MIT
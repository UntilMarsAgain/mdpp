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

块一共 22 个，覆盖引用、提示框、信息栏、选项卡、深浅色主题、图片音视频、代码块、
公式、mermaid 图、内联样式与原始 HTML。完整的巡览见
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

cargo install --path mdpp-cli   # 装进 PATH（程序名仍是 mdpp）
```

`--help` 有全部选项。加 `--full` 得到的页面可以直接双击打开，选项卡可点、打印时
折叠的内容会展开。

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

顶栏与底栏上那几句话由 `mdpp.toml` 给，**全是单行字符串** —— 与 mkdocs 的
`site_name` / `site_description` / `copyright` / `repo_url` 是一回事：

```toml
site_name = "桥梁笔记"                        # 顶栏左边
site_description = "十四座桥，与它们的记述"    # 顶栏右边
repo_url = "https://example.com/notes"        # 顶栏右端那个链接（只认 http/https）
copyright = "© 某人"                          # 底栏那句；空的就是没有底栏
```

`mdpp serve` 还没实现 —— 先 `mdpp build`，再用任意静态服务器托管 `build/`。

## 当库用

```rust
let html = mdpp::Mdpp::new().render("::note\n  这里可以写 **markdown**。\n");
```

## 许可

MIT
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
[`examples/tour.mdpp`](examples/tour.mdpp)，块名清单见 `mdpp --list-blocks`。

## 用起来

```sh
cargo build --release

./target/release/mdpp 正文.md          # HTML 片段，写到标准输出
./target/release/mdpp --full 正文.md   # 完整文档，内联默认样式与脚本
cargo install --path mdpp-cli          # 装进 PATH（程序名仍是 mdpp）
```

`--help` 有全部选项。加 `--full` 得到的页面可以直接双击打开，选项卡可点、打印时
折叠的内容会展开。

当库用：

```rust
let html = mdpp::Mdpp::new().render("::note\n  这里可以写 **markdown**。\n");
```

## 许可

MIT
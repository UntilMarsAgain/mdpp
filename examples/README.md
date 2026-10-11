# 示例

`.mdpp` 是本方言的后缀（普通 markdown 用 `.md`）。这里放的是**用命令行跑的示例
文件** —— 与 [`mdpp/examples/`](../mdpp/examples/) 那四个 Rust 例子不同：那边演示
API，这边演示写出来的东西长什么样。

| 文件 | 是什么 |
|---|---|
| [`tour.mdpp`](tour.mdpp) | 本版每一个块各来一次，当语法速查表用 |
| [`links.mdpp`](links.mdpp) | 内部链接 `[[…]]` 的写法与红链 |
| [`templates/`](templates/) | 模板页：`::卡片` 与 `::面板` 的正文 |

## 跑

```sh
# tour 里的用户块要模板页目录才渲染得出来
cargo run -p mdpp-cli -- render --link-prefix /notes/ -T examples/templates examples/tour.mdpp

# 整个文件变成一个能双击打开的页面
cargo run -p mdpp-cli -- render --full --theme light -T examples/templates examples/tour.mdpp

# 用自己那份样式与脚本（两者都要求 --full）
mdpp render --full --css 我的.css --js 我的.js examples/tour.mdpp
```

输出默认是 HTML **片段**（像 `cmark` 那样）；`--full` 才输出整页，并内联
`mdpp-cli/assets/` 下的 CSS 与 JS，所以选项卡可点、打印时折叠的会展开。那套资源与
库同一个 MIT 许可，可以复制去改。

## 这些示例放在项目里

`examples/` 也可以整个当 `pages/` 用 —— `mdpp init` 之后把 `templates/` 与这几个
文件拷进去，`mdpp build` 就出得来一个站点。链接那一份里的 `[[…]]` 在站点里会指向
站内页面；写出来却还没有的那一页会落成红链。

## 模板页的后缀

`--template-dir` 只认 `.mdpp`。模板页里常有 `{{}}` 与 `::` 方言，而后缀就是「这一份
不是普通 markdown」这句话 —— 不必去猜内容。

## 这些文件是被测的

`mdpp-cli/tests/samples.rs` 把每一份渲染一遍，断言里面**没有**「未知块」与「误用」
的框。块改名、参数改动或某个块坏了，这里会先红。
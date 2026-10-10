# 示例

`.mdpp` 是本方言的文件后缀（普通 markdown 用 `.md`）。这里放的是**用命令行跑的
示例文件** —— 与 `mdpp/examples/` 那四个 Rust 例子是两回事：那边演的是库的 API，
这边演的是写出来的东西长什么样。

| 文件 | 是什么 |
|---|---|
| `tour.mdpp` | 巡览：这一版带的块每一个来一次。**当语法速查表用** |
| `links.mdpp` | 内部链接 `[[…]]` 的写法，以及「没认出来」时是什么样 |
| `templates/` | 模板页。`::卡片` 与 `::面板` 的正文在这儿 |

## 跑

```sh
cargo run -p mdpp-cli -- examples/tour.mdpp

# 带上链接前缀与模板页目录（tour 里的用户块要后者才渲染得出来）
cargo run -p mdpp-cli -- --link-prefix /notes/ -T examples/templates examples/tour.mdpp

# 整个文件变成一个能双击打开的页面
cargo run -p mdpp-cli -- --full --theme light -T examples/templates examples/tour.mdpp
```

输出默认是 HTML **片段**（像 `cmark` 那样）；要整页加 `--full`。整页那份内联了
`mdpp-cli/assets/` 下的官方 CSS 与 JS，所以选项卡是能点的；打印（Ctrl+P）时折叠的
也会展开。那套资源与库同一个 MIT 许可，可以复制去改：

```sh
mdpp --full --css 我的.css --js 我的.js examples/tour.mdpp
```

## 模板页的后缀

`--template-dir` 只认 `.mdpp`，别的后缀一律不认。用一个专门的后缀是因为模板页里
常有 `{{}}` 与 `::` 方言，而它终究要走进 markdown 解析器 —— 后缀就是「这一份不是
普通 markdown」这句话，不用去猜内容。

## 这些文件是被测的

`mdpp-cli/tests/samples.rs` 每一份都渲染一遍，断言里面**没有**「未知块」与「误用」
的框。所以块改名了、参数改了、或者某个块坏了，这里会先红 —— 而示例文件不会悄悄
烂掉。
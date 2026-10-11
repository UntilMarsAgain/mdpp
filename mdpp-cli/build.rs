// 盯住那两个在**编译期**整个嵌进二进制的目录：`assets/`（页面外壳）与 `site/`
// （`mdpp init` 的站点骨架）。
//
// `include_dir!` 只对它**已经看见的那些文件**登记依赖 —— 于是「在骨架目录里加一个新
// 文件」不会触发重编，要 `touch` 一下别的文件才生效。登记目录本身之后，cargo 会
// 递归盯着它：改了、加了、删了，都会重编。
//
// 这个 crate 没有 build.rs 也能编译，但骨架是拿来改的（`site/` 里的文件就是新站点
// 长什么样），改不动就失去了把它挪出来的意义。

fn main() {
    for dir in ["assets", "site"] {
        println!("cargo:rerun-if-changed={dir}");
    }
    println!("cargo:rerun-if-changed=build.rs");
}

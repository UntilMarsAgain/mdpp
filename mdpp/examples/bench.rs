//! 一个不依赖 criterion 的小基准：单线程一页的耗时，以及同一份 `Mdpp`
//! 被多线程共享时的吞吐。
//!
//! 跑：
//!
//! ```sh
//! cargo run -p mdpp --release --example bench
//! ```
//!
//! 它不是性能承诺，只是一把尺子：改解析或状态管理之后，拿同一份输入量一量。

use std::time::Instant;

use mdpp::Mdpp;

/// 一份有点像真页面的输入：标题、提示框、列表、围栏代码、内部链接、公式
fn sample() -> String {
    let mut out = String::from("# 基准页\n\n");
    for index in 0..40 {
        out.push_str(&format!(
            "## 第 {index} 节\n\n\
             这一段有 **粗体**、`行内代码` 与 [[另一页]]。\n\n\
             ::note title=提示\n  块里的正文也能写 markdown：$x_{index}$。\n\n\
             - 甲\n- 乙\n- 丙\n\n\
             ```rust\nfn f() -> usize {{ {index} }}\n```\n\n"
        ));
    }
    out
}

fn main() {
    let source = sample();
    let mdpp = Mdpp::new();

    // 预热：把第一次构建/分配排除在外
    let _ = mdpp.render(&source);

    let rounds = 20usize;
    let started = Instant::now();
    for _ in 0..rounds {
        let _ = std::hint::black_box(mdpp.render(&source));
    }
    let single = started.elapsed() / rounds as u32;
    println!("单线程：{single:?}/页（输入 {} 字节）", source.len());

    let threads = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    if threads == 1 {
        return;
    }

    let started = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            let mdpp = &mdpp;
            let source = &source;
            scope.spawn(move || {
                for _ in 0..rounds {
                    let _ = std::hint::black_box(mdpp.render(source));
                }
            });
        }
    });
    let total = started.elapsed();
    let per = total / (rounds * threads) as u32;
    println!("{threads} 线程共享一个 Mdpp：{per:?}/页（总 {total:?}）");
}

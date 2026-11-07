// 模板页：把「不在注册表里的块名」接到一页 markdown 上。
//
//     cargo run -p mdpp --example template_pages
//
// `::card` 不是一个注册好的块，而是去模板页里找 `card` 那一页，把它正文填好
// `{{param}}` 再按 markdown 解析一遍。于是模板页怎么写就长什么样。

use std::sync::Arc;

use mdpp::{
    Mdpp, RenderContext,
    block::template::{TemplatePage, TemplateSource},
};

#[derive(Debug)]
struct Demo;

/// 「这一页在不在、读得动读不动」由实现方说了算。
///
/// 注意实现方**不必**自己剥 `template:` 前缀、不必自己转小写 —— 传进来的 `name`
/// 已经归一过了（见 `page_key`）。漏掉这一步的症状很隐蔽：`::card` 能用，
/// `src=template:card` 却报「不存在」。
impl TemplateSource for Demo {
    fn page(&self, name: &str) -> Option<TemplatePage> {
        match name {
            "card" => Some(TemplatePage::Ready(
                "::banner color=#0055a4\n  **{{title}}**\n\n  {{body}}\n".into(),
            )),
            "locked" => Some(TemplatePage::Unreadable {
                reason: "这一页是加密存的，需要口令".into(),
            }),
            // 别的名字一律「没有这一页」—— 那与「有但读不出来」是两件事，
            // 渲染出来也是两个不同的框，混成一句「取不到」最难查
            _ => None,
        }
    }
}

fn main() {
    let mdpp = Mdpp::new();
    let context = RenderContext::new().with_templates(Arc::new(Demo));

    println!("== 有参数、有正文 ==\n");
    println!(
        "{}",
        mdpp.render_with("::card title=\"某篇笔记\"\n  这段是块正文。\n", &context)
    );

    // 这行本身是个格式串，所以 `{{}}` 要写两遍才打得出 `{{}}`
    println!("== 参数没给：{{{{}}}} 原样留着 ==\n");
    println!("{}", mdpp.render_with("::card\n", &context));

    println!("== 那一页在，但读不出来 ==\n");
    println!("{}", mdpp.render_with("::locked\n", &context));

    println!("== 压根没有这一页 ==\n");
    println!("{}", mdpp.render_with("::missing\n", &context));
}

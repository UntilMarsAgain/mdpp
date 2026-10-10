// 模板页：把「不在注册表里的块名」接到一页 markdown 上。
//
//     cargo run --example template_pages
//
// `::卡片` 不是一个注册好的块，而是**去模板页里找 `卡片` 那一页**，把它��正文
// 填好 `{{参数}}` 再按 markdown 解析一遍。于是模板页怎么写就长什么样 ——
// 上面那一层什么都不知道，也不需要知道。
//
// 跑法：`cargo run --example template-pages`

use std::sync::Arc;

use mdpp::{
    block::template::{TemplatePage, TemplateSource},
    Mdpp, RenderContext,
};

#[derive(Debug)]
struct Demo;

/// 「这一页在不在、读得动读不动」由实现方说了算。
///
/// 注意实现方**不必**自己剥 `template:` 前缀、不必自己转小写 —— 传进来的 `name`
/// 已经归一过了（见 `page_key`）。漏掉这一步的症状很隐蔽：`::卡片` 能用，
/// `src=template:卡片` 却报「不存在」。
impl TemplateSource for Demo {
    fn page(&self, name: &str) -> Option<TemplatePage> {
        match name {
            "卡片" => Some(TemplatePage::Ready(
                "::banner color=#0055a4\n  **{{标题}}**\n\n  {{body}}\n".into(),
            )),
            "锁着的那页" => Some(TemplatePage::Unreadable {
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
        mdpp.render_with("::卡片 标题=\"某篇笔记\"\n  这段是块正文。\n", &context)
    );

    // 这行本身是个格式串，所以 `{{}}` 要写两遍才打得出 `{{}}`
    println!("== 参数没给：{{{{}}}} 原样留着 ==\n");
    println!("{}", mdpp.render_with("::卡片\n", &context));

    println!("== 那一页在，但读不出来 ==\n");
    println!("{}", mdpp.render_with("::锁着的那页\n", &context));

    println!("== 压根没有这一页 ==\n");
    println!("{}", mdpp.render_with("::没有这个\n", &context));
}

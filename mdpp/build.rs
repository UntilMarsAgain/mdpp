// 标准库模板在编译期整个嵌进库里。`include_dir!` 只对已经看见的文件登记依赖，
// 所以目录本身也要盯着：新增、改名、删除模板都会触发重编。

fn main() {
    println!("cargo:rerun-if-changed=stdlib");
    println!("cargo:rerun-if-changed=build.rs");
}

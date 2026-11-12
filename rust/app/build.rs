//! 把图标与版本信息编进 exe。
//!
//! Windows 上靠 `windres`（GNU 工具链）或 `rc.exe`（MSVC）把 `.rc` 编成资源段；
//! `embed-resource` 会自己找工具链，失败时只警告、不阻断构建（图标属于装修）。

fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=assets/app.ico");

    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("windows") {
        return;
    }
    // 返回 ()，失败时内部已经 panic；换了工具链导致编不进去也不该阻断功能开发
    embed_resource::compile("app.rc", embed_resource::NONE);
}

// 构建脚本：仅在 Windows 目标下把 violin 图标嵌入可执行文件。
// winres 在非 Windows 主机/目标上是空操作，因此 Linux/macOS 构建不受影响。
fn main() {
    // CARGO_CFG_TARGET_OS 由 cargo 注入，值为 "windows" / "linux" / "macos" 等。
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/violin.ico");
        // 版本资源里的产品名与文件说明也一并改为 violin。
        res.set("ProductName", "violin");
        res.set("FileDescription", "violin - MCPE 0.14 server");
        if let Err(e) = res.compile() {
            // 图标嵌入失败不应让整个构建静默通过，给出明确错误。
            panic!("failed to embed windows icon resource: {e}");
        }
    }
}

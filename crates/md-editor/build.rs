fn main() {
    println!("cargo:rerun-if-changed=resources/windows/app-icon.ico");
    println!("cargo:rerun-if-changed=resources/windows/app-icon.rc");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    embed_resource::compile("resources/windows/app-icon.rc", embed_resource::NONE)
        .manifest_optional()
        .unwrap();
}

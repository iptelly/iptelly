fn main() {
    // Debug info lets the headless UI tests find elements by id
    // (i_slint_backend_testing::ElementHandle). Release builds leave it out.
    let debug = std::env::var("PROFILE").as_deref() == Ok("debug");
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent-dark".into())
        .with_debug_info(debug);
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("failed to compile the Slint UI");

    // Only MSVC builds (the release ones) get the icon: building for the GNU
    // target would need MinGW's windres.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        embed_resource::compile("windows/iptelly.rc", embed_resource::NONE)
            .manifest_required()
            .expect("failed to embed the icon");
    }
}

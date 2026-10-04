fn main() {
    // Debug info lets the headless UI tests find elements by id
    // (i_slint_backend_testing::ElementHandle). Release builds leave it out.
    let debug = std::env::var("PROFILE").as_deref() == Ok("debug");
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent-dark".into())
        .with_debug_info(debug);
    slint_build::compile_with_config("ui/app.slint", config)
        .expect("failed to compile the Slint UI");
}

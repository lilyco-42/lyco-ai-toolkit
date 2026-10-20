fn main() {
    // Windows-only: embed the app icon into the exe's resource section.
    // Other targets compile fine without a resource compiler in play.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("failed to embed assets/icon.ico");
    }
}
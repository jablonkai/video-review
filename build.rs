fn main() {
    // libmpv from Homebrew (macOS) or the system (Linux).
    for dir in ["/opt/homebrew/lib", "/usr/local/lib"] {
        if std::path::Path::new(dir).join("libmpv.dylib").exists() {
            println!("cargo:rustc-link-search=native={dir}");
        }
    }
    if let Ok(dir) = std::env::var("MPV_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
    }
}

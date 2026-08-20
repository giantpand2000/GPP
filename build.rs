fn main() {
    println!("cargo:rerun-if-changed=pkgconfig");

    #[cfg(target_os = "macos")]
    {
        let root = "/Library/Frameworks/GStreamer.framework/Versions/1.0";
        let lib = format!("{root}/lib");
        if std::path::Path::new(&lib).exists() {
            println!("cargo:rustc-link-search=native={lib}");
            println!("cargo:rustc-link-search=framework=/Library/Frameworks");
            println!("cargo:rustc-link-arg=-Wl,-rpath,{root}");
            println!("cargo:rustc-link-arg=-Wl,-rpath,{lib}");
        }
    }
}

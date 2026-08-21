fn main() {
    println!("cargo:rerun-if-changed=pkgconfig");

    #[cfg(target_os = "macos")]
    {
        // Packaged builds place GStreamer beside the executable. Keep these
        // first so the app uses its private runtime instead of a system copy.
        let bundled = "@executable_path/../Frameworks/GStreamer.framework/Versions/1.0";
        println!("cargo:rustc-link-arg=-Wl,-rpath,{bundled}");
        println!("cargo:rustc-link-arg=-Wl,-rpath,{bundled}/lib");

        // The system paths keep `cargo run` working during development. The
        // packaging script removes them from the distributed executable.
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

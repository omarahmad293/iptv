use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let src = dir.join("libs").join("mpv").join("libmpv-2.dll");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // OUT_DIR is target/<profile>/build/<pkg>/out
    if let Some(profile_dir) = out.ancestors().nth(3) {
        let dst = profile_dir.join("libmpv-2.dll");
        if src.exists() && !dst.exists() {
            let _ = std::fs::copy(&src, &dst);
        }
    }
    println!("cargo:rerun-if-changed=libs/mpv/libmpv-2.dll");
}

//! Gives the exe its icon. `icon.res` is a compiled Windows resource written
//! ahead of time by `scripts/make_icon.py`, so the build needs no resource
//! compiler: the linker takes the file as it is.

fn main() {
    println!("cargo:rerun-if-changed=icon.res");
    // Only the MSVC linker takes a .res file as an input.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        let resource = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("icon.res");
        println!("cargo:rustc-link-arg-bins={}", resource.display());
    }
}

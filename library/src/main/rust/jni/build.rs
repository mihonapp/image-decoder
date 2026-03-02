fn main() {
    // AndroidBitmap_lockPixels / AndroidBitmap_unlockPixels live in libjnigraphics.so
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-lib=jnigraphics");
    }
}

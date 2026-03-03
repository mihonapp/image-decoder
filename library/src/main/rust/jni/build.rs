fn main() {
    // AndroidBitmap_lockPixels / AndroidBitmap_unlockPixels live in libjnigraphics.so
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        println!("cargo:rustc-link-lib=jnigraphics");

        // The vendored libjxl build produces static C++ libraries that reference
        // NDK libc++ symbols.  Link the static C++ runtime so those symbols
        // resolve in the final .so.
        println!("cargo:rustc-link-lib=static=c++_static");
        println!("cargo:rustc-link-lib=static=c++abi");

        // 16 KB page-size alignment (required by Google Play for Android 15+ devices).
        println!("cargo:rustc-link-arg=-Wl,-z,max-page-size=16384");
    }
}

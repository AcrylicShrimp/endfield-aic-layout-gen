fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && std::env::var_os("CARGO_FEATURE_EXTENSION_MODULE").is_some()
    {
        // Resolve CPython symbols from the importing interpreter, only for this cdylib.
        println!("cargo:rustc-link-arg-cdylib=-undefined");
        println!("cargo:rustc-link-arg-cdylib=dynamic_lookup");
    }
}

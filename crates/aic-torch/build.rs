fn main() {
    println!("cargo:rerun-if-env-changed=LIBTORCH");
    println!("cargo:rerun-if-env-changed=LIBTORCH_CXX11_ABI");
    println!("cargo:rerun-if-changed=native/bridge.cpp");
    println!("cargo:rerun-if-changed=native/optimizer.cpp");
    println!("cargo:rerun-if-changed=native/optimizer.h");
    #[cfg(feature = "libtorch")]
    {
        let root = std::path::PathBuf::from(
            std::env::var_os("LIBTORCH")
                .expect("aic-torch.build: set LIBTORCH to the matching LibTorch installation"),
        );
        let abi = std::env::var("LIBTORCH_CXX11_ABI").unwrap_or_else(|_| "1".into());
        assert!(
            matches!(abi.as_str(), "0" | "1"),
            "aic-torch.build: invalid LIBTORCH_CXX11_ABI"
        );
        let mut build = cc::Build::new();
        if root.join("lib/libtorch_cuda.so").exists() {
            build.define("AIC_LIBTORCH_CUDA", None);
        }
        build
            .cpp(true)
            .file("native/bridge.cpp")
            .file("native/optimizer.cpp")
            .flag("-isystem")
            .flag(root.join("include").to_str().expect("UTF-8 LibTorch path"))
            .flag("-isystem")
            .flag(
                root.join("include/torch/csrc/api/include")
                    .to_str()
                    .expect("UTF-8 LibTorch path"),
            )
            .flag_if_supported("-std=c++17")
            .define("_GLIBCXX_USE_CXX11_ABI", abi.as_str())
            .warnings(true)
            .compile("aic_torch_bridge");
    }
}

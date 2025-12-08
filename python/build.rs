fn main() {
    if std::env::var("CARGO_FEATURE_PYTHON_EXTENSION").is_ok() {
        let os = std::env::var("CARGO_CFG_TARGET_OS").expect("Unable to get TARGET_OS");
        match os.as_str() {
            "linux" | "windows" => {
                if let Some(lib_path) = std::env::var_os("DEP_TCH_LIBTORCH_LIB") {
                    println!(
                        "cargo:rustc-link-arg=-Wl,-rpath={}",
                        lib_path.to_string_lossy()
                    );
                }
                println!("cargo:rustc-link-arg=-Wl,--no-as-needed");
                println!("cargo:rustc-link-arg=-Wl,--copy-dt-needed-entries");
                println!("cargo:rustc-link-arg=-ltorch");
            }
            "macos" => {
                if let Some(lib_path) = std::env::var_os("DEP_TCH_LIBTORCH_LIB") {
                    println!(
                        "cargo:rustc-link-arg=-Wl,-rpath,{}",
                        lib_path.to_string_lossy()
                    );
                }
                println!("cargo:rustc-link-arg=-Wl,-undefined,dynamic_lookup");
                println!("cargo:rustc-link-arg=-ltorch");
                println!("cargo:rustc-link-arg=-ltorch_cpu");
                println!("cargo:rustc-link-arg=-lc10");

                // Link against Metal frameworks for MPS support
                println!("cargo:rustc-link-arg=-framework");
                println!("cargo:rustc-link-arg=Metal");
                println!("cargo:rustc-link-arg=-framework");
                println!("cargo:rustc-link-arg=MetalPerformanceShaders");
                println!("cargo:rustc-link-arg=-framework");
                println!("cargo:rustc-link-arg=Accelerate");
            }
            _ => {}
        }
    }
}

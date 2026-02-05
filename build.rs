fn main() {
    // 从环境变量获取版本号，优先级：
    // 1. CI_VERSION (GitHub Actions 注入)
    // 2. CARGO_PKG_VERSION (Cargo 自动设置)

    let version = if let Ok(ci_version) = std::env::var("CI_VERSION") {
        ci_version
    } else {
        std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".to_string())
    };

    // 将版本号注入编译代码
    println!("cargo::rustc-env=APP_VERSION={}", version);
}

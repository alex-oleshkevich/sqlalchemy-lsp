fn main() {
    let ts = std::env::var("SOURCE_DATE_EPOCH").unwrap_or_else(|_| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string())
    });
    println!("cargo:rustc-env=BUILD_TIMESTAMP={ts}");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
}

fn main() {
    // Ensure external binary placeholder exists before Tauri build checks
    let target_triple = std::env::var("TAURI_ENV_TARGET_TRIPLE")
        .or_else(|_| std::env::var("TARGET"))
        .unwrap_or_default();
    if !target_triple.is_empty() {
        let bin_path = std::path::PathBuf::from(
            std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default(),
        )
        .join("binaries")
        .join(format!("crawlflow-service-{}", target_triple));
        if !bin_path.exists() {
            std::fs::write(&bin_path, b"").ok();
        }
    }

    tauri_build::build();
    pyo3_build_config::add_python_framework_link_args();
}

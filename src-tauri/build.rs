fn main() {
    tauri_build::build();

    // On macOS, add rpath for the Python framework so the binary can find
    // libpython at runtime. This mirrors what pyo3's own build script does
    // (add_python_framework_link_args), but that flag only applies to the
    // pyo3 crate itself, not to dependents.
    pyo3_build_config::add_python_framework_link_args();
}

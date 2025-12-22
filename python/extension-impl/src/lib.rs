mod extension;
pub mod vllm;

pub fn init_embedded_python() -> std::io::Result<()> {
    if std::env::var("TRITON_HOME").is_err() {
        let triton_home = "/tmp/psyche-triton";

        std::fs::create_dir_all(triton_home)?;

        std::env::set_var("TRITON_HOME", triton_home);
    }

    extension::load_module();
    pyo3::prepare_freethreaded_python();
    pyo3::Python::with_gil(|py| {
        let _ = pyo3::Python::import(py, "psyche");
    });

    Ok(())
}

mod extension;

pub fn init_embedded_python() {
    extension::load_module();
    pyo3::prepare_freethreaded_python();
    pyo3::Python::with_gil(|py| {
        let _ = pyo3::Python::import(py, "psyche");
    });
}

#[cfg(feature = "python")]
mod extension;

#[cfg(feature = "python")]
pub fn init_embedded_python() {
    #[cfg(not(feature = "python-extension"))]
    {
        extension::load_module();
        pyo3::prepare_freethreaded_python();
    }
}

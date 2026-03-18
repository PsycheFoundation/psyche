use std::path::PathBuf;

mod extension;

/// Check if a directory supports exec by creating a temp file and trying to run it.
fn dir_supports_exec(dir: &std::path::Path) -> bool {
    use std::io::Write;
    let test_path = dir.join(".psyche_exec_test");
    let ok = (|| -> std::io::Result<bool> {
        std::fs::create_dir_all(dir)?;
        let mut f = std::fs::File::create(&test_path)?;
        f.write_all(b"#!/bin/sh\ntrue\n")?;
        drop(f);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&test_path, std::fs::Permissions::from_mode(0o755))?;
            match std::process::Command::new(&test_path).output() {
                Ok(_) => Ok(true),
                Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(false),
                Err(_) => Ok(true), // other errors (e.g. bad interpreter) — not a noexec issue
            }
        }
        #[cfg(not(unix))]
        Ok(true)
    })();
    let _ = std::fs::remove_file(&test_path);
    ok.unwrap_or(false)
}

pub fn init_embedded_python() -> std::io::Result<()> {
    let candidates: Vec<PathBuf> = if let Ok(home) = std::env::var("TRITON_HOME") {
        vec![home.into()]
    } else {
        [
            std::env::var("XDG_CACHE_HOME")
                .ok()
                .filter(|s| !s.is_empty())
                .map(|cache| PathBuf::from(cache).join("psyche/triton")),
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".cache/psyche/triton")),
            Some(PathBuf::from("/tmp/psyche-triton")),
        ]
        .into_iter()
        .flatten()
        .collect()
    };

    let triton_home = candidates
        .into_iter()
        .find(|p| dir_supports_exec(p))
        .ok_or_else(|| {
            std::io::Error::other(
                "No exec-capable directory found for Triton cache. Tried dirs {candidates:?}\
                     Set TRITON_HOME to a directory on a filesystem without noexec.",
            )
        })?;

    std::fs::create_dir_all(&triton_home)?;

    std::env::set_var("TRITON_HOME", triton_home);

    extension::load_module();
    pyo3::prepare_freethreaded_python();
    pyo3::Python::with_gil(|py| {
        let _ = pyo3::Python::import(py, "psyche");
    });

    Ok(())
}

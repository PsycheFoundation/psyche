use pyo3_stub_gen::Result;
use std::env;
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    if args.len() != 2 {
        eprintln!("Usage: {} <path-to-pyproject.toml>", args[0]);
        std::process::exit(1);
    }

    let pyproject_path = Path::new(&args[1]);

    if !pyproject_path.exists() {
        eprintln!(
            "Error: pyproject.toml not found at: {}",
            pyproject_path.display()
        );
        std::process::exit(1);
    }

    if !pyproject_path.is_file() {
        eprintln!("Error: {} is not a file", pyproject_path.display());
        std::process::exit(1);
    }

    let stub = psyche_python_extension::stub_info(pyproject_path)?;
    stub.generate()?;
    Ok(())
}

use pyo3::prelude::*;
use pyo3::wrap_pyfunction;

pub use spfs as other;

#[pyfunction]
fn test() {}

#[pymodule]
fn spkrs(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(test, m)?)?;
    Ok(())
}

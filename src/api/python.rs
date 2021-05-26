use pyo3::prelude::*;

pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<super::Ident>()?;
    m.add_class::<super::Spec>()?;
    m.add_class::<super::BuildSpec>()?;
    m.add_class::<super::InstallSpec>()?;
    m.add_class::<super::PkgRequest>()?;
    m.add_class::<super::RangeIdent>()?;
    m.add_class::<super::VarRequest>()?;
    m.add_class::<super::TestSpec>()?;
    m.add_class::<super::Version>()?;
    Ok(())
}

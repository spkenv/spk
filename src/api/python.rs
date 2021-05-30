use pyo3::{prelude::*, types, wrap_pyfunction};

#[pyfunction]
fn parse_version(v: &str) -> crate::Result<super::Version> {
    super::parse_version(v)
}

pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
    m.add("EMBEDDED", super::Build::Embedded.to_string())?;
    m.add("SRC", super::Build::Source.to_string())?;

    m.add_function(wrap_pyfunction!(parse_version, m)?)?;

    m.add_class::<super::Ident>()?;
    m.add_class::<super::Spec>()?;
    m.add_class::<super::BuildSpec>()?;
    m.add_class::<super::InstallSpec>()?;
    m.add_class::<super::PkgRequest>()?;
    m.add_class::<super::RangeIdent>()?;
    m.add_class::<super::VarRequest>()?;
    m.add_class::<super::TestSpec>()?;
    m.add_class::<super::Version>()?;
    m.add_class::<super::SemverRange>()?;
    m.add_class::<super::WildcardRange>()?;
    m.add_class::<super::LowestSpecifiedRange>()?;
    m.add_class::<super::GreaterThanRange>()?;
    m.add_class::<super::LessThanRange>()?;
    m.add_class::<super::GreaterThanOrEqualToRange>()?;
    m.add_class::<super::LessThanOrEqualToRange>()?;
    m.add_class::<super::ExactVersion>()?;
    m.add_class::<super::ExcludedVersion>()?;
    m.add_class::<super::CompatRange>()?;
    m.add_class::<super::VersionFilter>()?;
    Ok(())
}

impl IntoPy<Py<types::PyAny>> for super::Build {
    fn into_py(self, py: Python) -> Py<types::PyAny> {
        self.to_string().into_py(py)
    }
}

impl<'source> FromPyObject<'source> for super::Build {
    fn extract(ob: &'source PyAny) -> PyResult<Self> {
        let string = <&'source str>::extract(ob)?;
        match super::parse_build(string) {
            Err(err) => Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(
                err.to_string(),
            )),
            Ok(res) => Ok(res),
        }
    }
}

impl IntoPy<Py<PyAny>> for super::Request {
    fn into_py(self, py: Python) -> Py<PyAny> {
        match self {
            super::Request::Var(var) => var.into_py(py),
            super::Request::Pkg(pkg) => pkg.into_py(py),
        }
    }
}

impl IntoPy<Py<PyAny>> for super::VersionRange {
    fn into_py(self, py: Python) -> Py<PyAny> {
        match self {
            super::VersionRange::Semver(rng) => rng.into_py(py),
            super::VersionRange::Wildcard(rng) => rng.into_py(py),
            super::VersionRange::LowestSpecified(rng) => rng.into_py(py),
            super::VersionRange::GreaterThan(rng) => rng.into_py(py),
            super::VersionRange::LessThan(rng) => rng.into_py(py),
            super::VersionRange::GreaterThanOrEqualTo(rng) => rng.into_py(py),
            super::VersionRange::LessThanOrEqualTo(rng) => rng.into_py(py),
            super::VersionRange::Exact(rng) => rng.into_py(py),
            super::VersionRange::Excluded(rng) => rng.into_py(py),
            super::VersionRange::Compat(rng) => rng.into_py(py),
            super::VersionRange::Filter(rng) => rng.into_py(py),
        }
    }
}

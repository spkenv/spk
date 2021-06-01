use pyo3::{prelude::*, types, wrap_pyfunction};

#[pyfunction]
fn parse_version(v: &str) -> crate::Result<super::Version> {
    super::parse_version(v)
}

#[pyclass]
struct Compatibility {
    inner: super::Compatibility,
}

#[pymethods]
impl Compatibility {
    #[new]
    #[args(msg = "\"\"")]
    fn new(msg: &str) -> Compatibility {
        let inner = if msg.is_empty() {
            super::Compatibility::Compatible
        } else {
            super::Compatibility::Incompatible(msg.to_string())
        };
        Compatibility { inner: inner }
    }
}

#[pyproto]
impl pyo3::PyObjectProtocol for Compatibility {
    fn __bool__(&self) -> bool {
        match self.inner {
            super::Compatibility::Compatible => true,
            super::Compatibility::Incompatible(_) => false,
        }
    }

    fn __str__(&self) -> String {
        match &self.inner {
            super::Compatibility::Compatible => "".to_string(),
            super::Compatibility::Incompatible(msg) => msg.clone(),
        }
    }
}

pub fn init_module(_py: &Python, m: &PyModule) -> PyResult<()> {
    m.add("EMBEDDED", super::Build::Embedded.to_string())?;
    m.add("SRC", super::Build::Source.to_string())?;
    m.add("COMPATIBLE", Compatibility::new(""))?;

    m.add_function(wrap_pyfunction!(parse_version, m)?)?;
    m.add_function(wrap_pyfunction!(opt_from_dict, m)?)?;
    m.add_function(wrap_pyfunction!(request_from_dict, m)?)?;

    m.add_class::<super::Ident>()?;
    m.add_class::<super::Spec>()?;
    m.add_class::<super::BuildSpec>()?;
    m.add_class::<super::InstallSpec>()?;
    m.add_class::<super::PkgRequest>()?;
    m.add_class::<super::RangeIdent>()?;
    m.add_class::<super::VarRequest>()?;
    m.add_class::<super::TestSpec>()?;
    m.add_class::<super::Version>()?;
    m.add_class::<super::OptionMap>()?;
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

impl IntoPy<Py<PyAny>> for super::Opt {
    fn into_py(self, py: Python) -> Py<PyAny> {
        match self {
            super::Opt::Var(var) => var.into_py(py),
            super::Opt::Pkg(pkg) => pkg.into_py(py),
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

#[pymethods]
impl super::Spec {
    #[staticmethod]
    fn from_dict(input: Py<pyo3::types::PyDict>, py: Python) -> crate::Result<Self> {
        from_dict(input, py)
    }
}

#[pymethods]
impl super::TarSource {
    #[staticmethod]
    fn from_dict(input: Py<pyo3::types::PyDict>, py: Python) -> crate::Result<Self> {
        from_dict(input, py)
    }
}

#[pymethods]
impl super::GitSource {
    #[staticmethod]
    fn from_dict(input: Py<pyo3::types::PyDict>, py: Python) -> crate::Result<Self> {
        from_dict(input, py)
    }
}

#[pymethods]
impl super::LocalSource {
    #[staticmethod]
    fn from_dict(input: Py<pyo3::types::PyDict>, py: Python) -> crate::Result<Self> {
        from_dict(input, py)
    }
}

#[pymethods]
impl super::PkgRequest {
    #[staticmethod]
    fn from_dict(input: Py<pyo3::types::PyDict>, py: Python) -> crate::Result<Self> {
        from_dict(input, py)
    }
}

#[pyfunction]
fn opt_from_dict(input: Py<pyo3::types::PyDict>, py: Python) -> crate::Result<super::Opt> {
    from_dict(input, py)
}

#[pyfunction]
fn request_from_dict(input: Py<pyo3::types::PyDict>, py: Python) -> crate::Result<super::Request> {
    from_dict(input, py)
}

fn from_dict<T>(input: Py<pyo3::types::PyDict>, py: Python) -> crate::Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let locals = pyo3::types::PyDict::new(py);
    let _ = locals.set_item("data", input);
    let dumps = py
        .eval("import json; json.dumps(data)", None, Some(locals))
        .or_else(|err| {
            Err(crate::Error::String(format!(
                "Not a valid dictionary: {:?}",
                err
            )))
        })?;
    let json: &str = dumps.extract().or_else(|err| {
        Err(crate::Error::String(format!(
            "Not a valid dictionary: {:?}",
            err
        )))
    })?;
    Ok(serde_yaml::from_str(json)?)
}

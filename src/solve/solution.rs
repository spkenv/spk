// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{
    prelude::*,
    types::{PyDict, PyTuple},
    PyIterProtocol,
};
use std::collections::HashMap;

use crate::api::{self, Ident};

#[derive(Clone, Debug)]
pub enum PackageSource {
    Repository(PyObject),
    Spec(Box<api::Spec>),
}

impl<'source> FromPyObject<'source> for PackageSource {
    fn extract(ob: &'source PyAny) -> PyResult<Self> {
        ob.extract::<api::Spec>()
            .map(|s| PackageSource::Spec(Box::new(s)))
            .or_else(|_| Ok(PackageSource::Repository(ob.into())))
    }
}

impl IntoPy<Py<PyAny>> for PackageSource {
    fn into_py(self, py: Python) -> Py<PyAny> {
        match self {
            PackageSource::Repository(s) => s,
            PackageSource::Spec(s) => s.into_py(py),
        }
    }
}

impl PackageSource {
    pub fn read_spec(&self, ident: &Ident) -> PyResult<api::Spec> {
        match self {
            PackageSource::Spec(s) => Ok((**s).clone()),
            PackageSource::Repository(repo) => {
                Python::with_gil(|py| {
                    // XXX: Ident: ToPyObject missing?
                    let args = PyTuple::new(py, &[ident.to_string()]);
                    repo.call_method1(py, "read_spec", args)?
                        .as_ref(py)
                        .extract::<api::Spec>()
                })
            }
        }
    }
}

/// Represents a package request that has been resolved.
#[pyclass]
pub struct SolvedRequest {
    pub request: api::PkgRequest,
    pub spec: api::Spec,
    pub source: PackageSource,
}

#[pyclass]
#[derive(Debug)]
pub struct Solution {
    options: api::OptionMap,
    resolved: HashMap<api::PkgRequest, (api::Spec, PackageSource)>,
    by_name: HashMap<String, api::Spec>,
}

impl Solution {}

#[pyproto]
impl pyo3::PyObjectProtocol for Solution {
    fn __repr__(&self) -> String {
        format!("{:#?}", self)
    }
}

#[pyclass]
pub struct SolvedRequestIter {
    iter: std::vec::IntoIter<(api::PkgRequest, api::Spec, PackageSource)>,
}

#[pyproto]
impl PyIterProtocol for SolvedRequestIter {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<Self>) -> Option<(api::PkgRequest, api::Spec, PackageSource)> {
        slf.iter.next()
    }
}

#[derive(Debug, FromPyObject)]
pub enum BaseEnvironment<'a> {
    Dict(HashMap<String, String>),
    // Handle being called with `os.environ`.
    // '_Environ' object cannot be converted to 'PyDict'
    Other(&'a PyAny),
}

#[pymethods]
impl Solution {
    #[new]
    pub fn new(options: Option<api::OptionMap>) -> Self {
        Self {
            options: options.unwrap_or_default(),
            resolved: HashMap::default(),
            by_name: HashMap::default(),
        }
    }

    pub fn add(&mut self, request: &api::PkgRequest, package: &api::Spec, source: PackageSource) {
        self.resolved
            .insert(request.clone(), (package.clone(), source));
        self.by_name
            .insert(request.pkg.name().to_owned(), package.clone());
    }

    pub fn items(&self) -> SolvedRequestIter {
        SolvedRequestIter {
            iter: self
                .resolved
                .clone()
                .into_iter()
                .map(|(request, (spec, source))| (request, spec, source))
                .collect::<Vec<_>>()
                .into_iter(),
        }
    }

    pub fn options(&self) -> api::OptionMap {
        self.options.clone()
    }

    /// Return the data of this solution as environment variables.
    ///
    /// If base is given, also clean any existing, conflicting values.
    pub fn to_environment(
        &self,
        base: Option<BaseEnvironment>,
    ) -> PyResult<HashMap<String, String>> {
        let mut out = match base {
            Some(BaseEnvironment::Dict(base)) => base,
            Some(BaseEnvironment::Other(base)) => {
                Python::with_gil(|py| {
                    // Try to coerce given object into a dictionary, as in:
                    //
                    //     dict(os.environ)
                    let dict = py.get_type::<PyDict>();
                    dict.call1((base,))?.extract::<HashMap<String, String>>()
                })?
            }
            None => HashMap::default(),
        };

        out.retain(|name, _| !name.starts_with("SPK_PKG_"));

        out.insert("SPK_ACTIVE_PREFIX".to_owned(), "/spfs".to_owned());
        for (_request, (spec, _source)) in self.resolved.iter() {
            out.insert(format!("SPK_PKG_{}", spec.pkg.name()), spec.pkg.to_string());
            out.insert(
                format!("SPK_PKG_{}_VERSION", spec.pkg.name()),
                spec.pkg.version.to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_BUILD", spec.pkg.name()),
                spec.pkg
                    .build
                    .as_ref()
                    .map(|b| b.to_string())
                    .unwrap_or_else(|| "None".to_owned()),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_MAJOR", spec.pkg.name()),
                spec.pkg.version.major.to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_MINOR", spec.pkg.name()),
                spec.pkg.version.minor.to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_PATCH", spec.pkg.name()),
                spec.pkg.version.patch.to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_BASE", spec.pkg.name()),
                spec.pkg
                    .version
                    .parts()
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<String>>()
                    .join(api::VERSION_SEP),
            );
        }

        out.extend(self.options.to_environment().into_iter());
        Ok(out)
    }
}

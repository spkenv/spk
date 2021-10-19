// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use pyo3::{prelude::*, types::PyDict, PyIterProtocol};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

use crate::{
    api::{self, Ident},
    storage, Result,
};

#[derive(Clone, Debug)]
pub enum PackageSource {
    Repository(Arc<Mutex<storage::RepositoryHandle>>),
    Spec(Arc<api::Spec>),
}

impl<'source> FromPyObject<'source> for PackageSource {
    fn extract(ob: &'source PyAny) -> PyResult<Self> {
        if let Ok(s) = ob.extract::<api::Spec>() {
            Ok(PackageSource::Spec(Arc::new(s)))
        } else {
            ob.extract::<storage::python::Repository>()
                .map(|r| PackageSource::Repository(r.handle))
        }
    }
}

impl IntoPy<Py<PyAny>> for PackageSource {
    fn into_py(self, py: Python) -> Py<PyAny> {
        match self {
            PackageSource::Repository(s) => storage::python::Repository { handle: s }.into_py(py),
            PackageSource::Spec(s) => (*s).clone().into_py(py),
        }
    }
}

impl PackageSource {
    pub fn read_spec(&self, ident: &Ident) -> Result<api::Spec> {
        match self {
            PackageSource::Spec(s) => Ok((**s).clone()),
            PackageSource::Repository(repo) => repo.lock().unwrap().read_spec(ident),
        }
    }
}

/// Represents a package request that has been resolved.
#[pyclass]
pub struct SolvedRequest {
    #[pyo3(get)]
    pub request: api::PkgRequest,
    pub spec: Arc<api::Spec>,
    #[pyo3(get)]
    pub source: PackageSource,
}

#[pymethods]
impl SolvedRequest {
    #[getter]
    pub fn spec(&self) -> api::Spec {
        (*self.spec).clone()
    }

    pub fn is_source_build(&self) -> bool {
        match &self.source {
            PackageSource::Repository(_) => false,
            PackageSource::Spec(spec) => spec.pkg == self.spec.pkg.with_build(None),
        }
    }
}

// Support code that treats a SolvedRequest as a 3-tuple
#[pyproto]
impl pyo3::PySequenceProtocol for SolvedRequest {
    fn __getitem__(&self, idx: isize) -> PyResult<PyObject> {
        Python::with_gil(|py| match idx {
            0 => Ok(self.request.clone().into_py(py)),
            1 => Ok((*self.spec).clone().into_py(py)),
            2 => Ok(self.source.clone().into_py(py)),
            _ => Err(pyo3::exceptions::PyIndexError::new_err("")),
        })
    }

    fn __len__(&self) -> usize {
        3
    }
}

/// Represents a set of resolved packages.
#[pyclass]
#[derive(Clone, Debug)]
pub struct Solution {
    options: api::OptionMap,
    resolved: HashMap<api::PkgRequest, (Arc<api::Spec>, PackageSource)>,
    by_name: HashMap<String, Arc<api::Spec>>,
    insertion_order: HashMap<api::PkgRequest, usize>,
}

#[pyproto]
impl pyo3::mapping::PyMappingProtocol for Solution {
    fn __len__(self) -> usize {
        self.resolved.len()
    }
}

#[pyproto]
impl pyo3::PyObjectProtocol for Solution {
    fn __repr__(&self) -> String {
        format!("{:#?}", self)
    }
}

#[pyclass]
pub struct SolvedRequestIter {
    iter: std::vec::IntoIter<SolvedRequest>,
}

impl Iterator for SolvedRequestIter {
    type Item = SolvedRequest;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

#[pyproto]
impl PyIterProtocol for SolvedRequestIter {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(mut slf: PyRefMut<Self>) -> Option<SolvedRequest> {
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
            insertion_order: HashMap::default(),
        }
    }

    pub fn add(&mut self, request: &api::PkgRequest, package: api::Spec, source: PackageSource) {
        let package = Arc::new(package);
        if self
            .resolved
            .insert(request.clone(), (package.clone(), source))
            .is_none()
        {
            self.insertion_order
                .insert(request.clone(), self.insertion_order.len());
        }
        self.by_name.insert(request.pkg.name().to_owned(), package);
    }

    pub fn items(&self) -> SolvedRequestIter {
        let mut items = self
            .resolved
            .clone()
            .into_iter()
            .map(|(request, (spec, source))| SolvedRequest {
                request,
                spec,
                source,
            })
            .collect::<Vec<_>>();
        // Test suite expects these items to be returned in original insertion order.
        items.sort_by_key(|sr| self.insertion_order.get(&sr.request).unwrap());

        SolvedRequestIter {
            iter: items.into_iter(),
        }
    }

    pub fn get(&self, name: &str) -> PyResult<SolvedRequest> {
        for (request, (spec, source)) in &self.resolved {
            if request.pkg.name() == name {
                return Ok(SolvedRequest {
                    request: request.clone(),
                    spec: spec.clone(),
                    source: source.clone(),
                });
            }
        }
        Err(pyo3::exceptions::PyKeyError::new_err(name.to_owned()))
    }

    pub fn options(&self) -> api::OptionMap {
        self.options.clone()
    }

    /// Return the set of repositories in this solution.
    pub fn repositories(&self) -> Result<Vec<storage::python::Repository>> {
        let mut seen = HashSet::new();
        let mut repos = Vec::new();
        for (_, source) in self.resolved.values() {
            if let PackageSource::Repository(repo) = source {
                let addr = repo.lock().unwrap().address();
                if seen.contains(&addr) {
                    continue;
                }
                repos.push(storage::python::Repository {
                    handle: repo.clone(),
                });
                seen.insert(addr);
            }
        }
        Ok(repos)
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

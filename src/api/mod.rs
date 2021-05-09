// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod source_spec;
mod build_spec;
mod name;
mod spec;
mod option_map;
mod mod;
mod version_range;
mod request;
mod ident;
mod compat;
mod version;
mod build;
mod test_spec;

pub use name::{ InvalidNameError};
pub use option_map::{ OptionMap, host_options};
pub use version::{ Version, parse_version, VERSION_SEP, InvalidVersionError};
pub use compat::{ Compat, parse_compat, Compatibility, COMPATIBLE, CompatRule};
pub use build::{ Build, parse_build, SRC, EMBEDDED, InvalidBuildError};
pub use ident::{ Ident, parse_ident, validate_name};
pub use version_range::{
    VersionRange,
    VersionFilter,
    VERSION_RANGE_SEP,
    parse_version_range,
};
pub use build_spec::{ BuildSpec, opt_from_dict, VarOpt, PkgOpt, Option, Inheritance};
pub use source_spec::{ SourceSpec, LocalSource, GitSource, TarSource, ScriptSource};
pub use test_spec::{ TestSpec};
pub use request::{
    Request,
    PkgRequest,
    VarRequest,
    parse_ident_range,
    PreReleasePolicy,
    InclusionPolicy,
    RangeIdent,
};
pub use spec::{
    InstallSpec,
    Spec,
    read_spec_file,
    read_spec,
    write_spec,
    save_spec_file,
};

use pyo3::prelude::*;
pub fn init_module(py: &Python, m: &PyModule) -> PyResult<()> {
    Ok(())
}

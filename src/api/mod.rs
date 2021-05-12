// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build;
// mod build_spec;
mod compat;
// mod ident;
mod name;
mod option_map;
// mod request;
// mod source_spec;
// mod spec;
// mod test_spec;
mod version;
// mod version_range;

pub use build::{parse_build, Build, InvalidBuildError};
// pub use build_spec::{opt_from_dict, BuildSpec, Inheritance, Option, PkgOpt, VarOpt};
pub use compat::{parse_compat, Compat, CompatRule, CompatRuleSet, Compatibility};
// pub use ident::{parse_ident, validate_name, Ident};
pub use name::{validate_name, validate_tag_name, InvalidNameError};
pub use option_map::{host_options, OptionMap};
// pub use request::{
//     parse_ident_range, InclusionPolicy, PkgRequest, PreReleasePolicy, RangeIdent, Request,
//     VarRequest,
// };
// pub use source_spec::{GitSource, LocalSource, ScriptSource, SourceSpec, TarSource};
// pub use spec::{read_spec, read_spec_file, save_spec_file, write_spec, InstallSpec, Spec};
// pub use test_spec::TestSpec;
pub use version::{parse_version, InvalidVersionError, Version, TAG_SEP, TAG_SET_SEP, VERSION_SEP};
// pub use version_range::{parse_version_range, VersionFilter, VersionRange, VERSION_RANGE_SEP};

use pyo3::prelude::*;
pub fn init_module(_py: &Python, _m: &PyModule) -> PyResult<()> {
    Ok(())
}

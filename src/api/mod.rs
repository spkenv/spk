// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build;
mod build_spec;
mod compat;
mod component_spec;
mod component_spec_list;
mod embedded_packages_list;
mod environ;
mod ident;
mod install_spec;
mod meta;
mod name;
mod option;
mod option_map;
mod python;
mod request;
mod requirements_list;
mod source_spec;
mod spec;
mod test_spec;
mod validation;
mod validators;
mod version;
mod version_range;

pub use build::{parse_build, Build, InvalidBuildError};
pub use build_spec::BuildSpec;
pub use compat::{parse_compat, Compat, CompatRule, CompatRuleSet, Compatibility};
pub use component_spec::{Component, ComponentSpec, FileMatcher};
pub use component_spec_list::ComponentSpecList;
pub use embedded_packages_list::EmbeddedPackagesList;
pub use environ::{AppendEnv, EnvOp, PrependEnv, SetEnv};
pub use ident::{parse_ident, BuildIdent, Ident};
pub use install_spec::InstallSpec;
pub use meta::Meta;
pub use name::{validate_name, validate_tag_name, InvalidNameError};
pub use option::{Inheritance, Opt, PkgOpt, VarOpt};
pub use option_map::{host_options, OptionMap};
pub use request::{
    parse_ident_range, InclusionPolicy, PkgRequest, PreReleasePolicy, RangeIdent, Request,
    VarRequest,
};
pub use requirements_list::RequirementsList;
pub use source_spec::{GitSource, LocalSource, ScriptSource, SourceSpec, TarSource};
pub use spec::{read_spec_file, save_spec_file, Spec, SpecWithBuildVariant};

pub use python::init_module;
pub use test_spec::{TestSpec, TestStage};
pub use validation::{default_validators, ValidationSpec, Validator};
pub use version::{
    parse_version, InvalidVersionError, TagSet, Version, TAG_SEP, TAG_SET_SEP, VERSION_SEP,
};
pub use version_range::{
    parse_version_range, CompatRange, ExactVersion, ExcludedVersion, GreaterThanOrEqualToRange,
    GreaterThanRange, LessThanOrEqualToRange, LessThanRange, LowestSpecifiedRange, Ranged,
    SemverRange, VersionFilter, VersionRange, WildcardRange, VERSION_RANGE_SEP,
};

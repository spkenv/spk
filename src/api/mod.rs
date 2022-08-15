// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build;
mod build_key;
mod build_spec;
mod builded;
mod compat;
mod component_spec;
mod component_spec_list;
mod deprecate;
mod embedded_packages_list;
mod environ;
pub(crate) mod ident;
mod install_spec;
mod meta;
mod name;
mod option;
mod option_map;
mod package;
pub mod prelude;
mod recipe;
mod request;
mod requirements_list;
mod source_spec;
mod spec;
mod template;
mod test_spec;
pub mod v0;
mod validation;
mod validators;
mod version;
mod version_range;

pub use build::{parse_build, Build, InvalidBuildError};
pub(crate) use build::{EMBEDDED, SRC};
pub use build_key::BuildKey;
pub use build_spec::BuildSpec;
pub use builded::Builded;
pub use compat::{parse_compat, Compat, CompatRule, CompatRuleSet, Compatibility};
pub use component_spec::{Component, ComponentSpec, FileMatcher};
pub use component_spec_list::ComponentSpecList;
pub use deprecate::{Deprecate, DeprecateMut};
pub use embedded_packages_list::EmbeddedPackagesList;
pub use environ::{AppendEnv, EnvOp, PrependEnv, SetEnv};
pub use ident::{
    parse_build_ident, parse_ident, parse_version_ident, AnyId, BuildId, BuildIdent, Ident,
    PlacedBuildId, VersionId, VersionIdent,
};
pub use install_spec::InstallSpec;
pub use meta::Meta;
pub use name::{
    validate_tag_name, InvalidNameError, OptName, OptNameBuf, PkgName, PkgNameBuf, RepositoryName,
    RepositoryNameBuf,
};
pub use option::{Inheritance, Opt, PkgOpt, VarOpt};
pub(crate) use option_map::DIGEST_SIZE;
pub use option_map::{host_options, OptionMap};
pub use package::Package;
pub use recipe::{Recipe, Versioned, VersionedMut};
pub use request::{
    parse_ident_range, InclusionPolicy, PkgRequest, PreReleasePolicy, RangeIdent, Request,
    RequestedBy, VarRequest,
};
pub use requirements_list::RequirementsList;
pub use source_spec::{GitSource, LocalSource, ScriptSource, SourceSpec, TarSource};
pub use spec::{Spec, SpecRecipe, SpecTemplate};
pub use template::{Named, Template, TemplateExt};

pub use test_spec::{TestSpec, TestStage};
pub use validation::{default_validators, ValidationSpec, Validator};
pub use version::{
    parse_tag_set, parse_version, InvalidVersionError, TagSet, Version, TAG_SEP, TAG_SET_SEP,
    VERSION_SEP,
};
pub use version_range::{
    parse_version_range, CompatRange, DoubleEqualsVersion, DoubleNotEqualsVersion, EqualsVersion,
    GreaterThanOrEqualToRange, GreaterThanRange, LessThanOrEqualToRange, LessThanRange,
    LowestSpecifiedRange, NotEqualsVersion, Ranged, SemverRange, VersionFilter, VersionRange,
    WildcardRange, VERSION_RANGE_SEP,
};

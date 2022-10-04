// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod build_spec;
mod component_spec;
mod component_spec_list;
mod deprecate;
mod embedded_packages_list;
mod environ;
mod error;
mod install_spec;
mod meta;
mod option;
mod package;
pub mod prelude;
mod recipe;
mod requirements_list;
mod source_spec;
mod spec;
mod template;
mod test_spec;
pub mod v0;
mod validation;

pub use build_spec::{BuildSpec, Script};
pub use component_spec::{ComponentFileMatchMode, ComponentSpec};
pub use component_spec_list::ComponentSpecList;
pub use deprecate::{Deprecate, DeprecateMut};
pub use embedded_packages_list::EmbeddedPackagesList;
pub use environ::{AppendEnv, EnvOp, PrependEnv, SetEnv};
pub use error::{Error, Result};
pub use install_spec::InstallSpec;
pub use option::{Inheritance, Opt};
pub use package::Package;
pub use recipe::{BuildEnv, Recipe};
pub use requirements_list::RequirementsList;
pub use source_spec::{GitSource, LocalSource, ScriptSource, SourceSpec, TarSource};
pub use spec::{Spec, SpecRecipe, SpecTemplate};
pub use spk_schema_foundation::option_map::{self, OptionMap};
pub use spk_schema_foundation::{
    self as foundation,
    env,
    ident_build,
    ident_component,
    ident_ops,
    name,
    spec_ops,
    version,
    version_range,
    FromYaml,
};
pub use spk_schema_ident::{self as ident, Ident};
pub use template::{Template, TemplateExt};
pub use test_spec::TestStage;
pub use validation::{default_validators, ValidationSpec, Validator};
pub use {serde_json, spk_schema_validators as validators};

#[cfg(test)]
#[path = "./version_range_test.rs"]
mod version_range_test;

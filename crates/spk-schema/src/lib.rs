// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

mod component_spec;
mod component_spec_list;
mod deprecate;
mod environ;
mod error;
mod meta;
mod package;
pub mod prelude;
mod recipe;
mod requirements_list;
mod source_spec;
mod spec;
mod template;
pub mod v0;
pub mod v1;
mod validation;

pub use component_spec::{ComponentFileMatchMode, ComponentSpec};
pub use component_spec_list::ComponentSpecList;
pub use deprecate::{Deprecate, DeprecateMut};
pub use environ::{AppendEnv, EnvOp, PrependEnv, SetEnv};
pub use error::{Error, Result};
pub use package::{Package, PackageMut};
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
pub use spk_schema_ident::{self as ident, AnyIdent, BuildIdent, VersionIdent};
pub use template::{Template, TemplateExt};
pub use validation::{default_validators, ValidationSpec, Validator};
pub use {serde_json, spk_schema_validators as validators};

#[cfg(test)]
#[path = "./version_range_test.rs"]
mod version_range_test;

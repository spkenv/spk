// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod build_spec;
mod component_embedded_packages;
mod component_spec;
mod component_spec_list;
mod deprecate;
mod embedded_packages_list;
mod environ;
mod error;
mod input_variant;
mod install_spec;
mod lints;
mod metadata;
mod option;
mod package;
pub mod prelude;
mod recipe;
mod requirements_list;
mod source_spec;
mod spec;
mod template;
mod test;
pub mod v0;
pub mod validation;
pub mod variant;

pub use build_spec::{BuildSpec, Script};
pub use component_embedded_packages::ComponentEmbeddedPackagesList;
pub use component_spec::{ComponentFileMatchMode, ComponentSpec};
pub use component_spec_list::ComponentSpecList;
pub use deprecate::{Deprecate, DeprecateMut};
pub use embedded_packages_list::EmbeddedPackagesList;
pub use environ::{
    AppendEnv,
    EnvComment,
    EnvOp,
    EnvOpList,
    EnvPriority,
    OpKind,
    PrependEnv,
    RuntimeEnvironment,
    SetEnv,
};
pub use error::{Error, Result};
pub use input_variant::InputVariant;
pub use install_spec::{InstallSpec, RawInstallSpec};
pub use lints::{Lint, LintedItem, Lints, UnknownKey};
pub use option::{Inheritance, Opt};
pub use package::{Package, PackageMut};
pub use recipe::{BuildEnv, Recipe};
pub use requirements_list::RequirementsList;
pub use serde_json;
pub use source_spec::{GitSource, LocalSource, ScriptSource, SourceSpec, TarSource};
pub use spec::{ApiVersion, LintedSpec, Spec, SpecFileData, SpecRecipe, SpecTemplate, SpecVariant};
pub use spk_schema_foundation::option_map::{self, OptionMap};
pub use spk_schema_foundation::{
    self as foundation,
    env,
    ident_build,
    ident_component,
    ident_ops,
    name,
    opt_name,
    spec_ops,
    version,
    version_range,
    FromYaml,
};
pub use spk_schema_ident::{self as ident, AnyIdent, BuildIdent, Request, VersionIdent};
pub use template::{Template, TemplateData, TemplateExt};
pub use test::{Test, TestStage};
pub use validation::{ValidationRule, ValidationSpec};
pub use variant::{Variant, VariantExt};

#[cfg(test)]
#[path = "./version_range_test.rs"]
mod version_range_test;

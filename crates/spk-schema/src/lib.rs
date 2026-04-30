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
mod fb_converter;
mod input_variant;
mod install_spec;
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
pub mod v1;
pub mod validation;
pub mod variant;

pub use build_spec::BuildSpec;
pub use component_embedded_packages::{ComponentEmbeddedPackage, ComponentEmbeddedPackagesList};
pub use component_spec::ComponentSpec;
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
pub use fb_converter::{
    build_to_fb_build,
    compat_to_fb_compat,
    component_specs_to_fb_component_specs,
    components_to_fb_components,
    embedded_pkg_specs_to_fb_embedded_package_specs,
    fb_compat_to_compat,
    fb_component_emb_pkgs_to_component_emb_pkgs,
    fb_component_names_to_component_names,
    fb_component_names_to_component_names_set,
    fb_component_specs_to_component_name_set,
    fb_component_specs_to_component_names,
    fb_component_specs_to_component_specs,
    fb_component_to_component,
    fb_embedded_package_specs_to_embedded_package_specs,
    fb_inclusion_policy_to_inclusion_policy,
    fb_lone_compat_rule_to_lone_compat_rule,
    fb_opt_to_opt,
    fb_opts_to_opts,
    fb_pin_policy_to_pin_policy,
    fb_pin_to_pin,
    fb_pkg_opt_to_opt,
    fb_pkg_request_option_values_to_pkg_request_options,
    fb_prerelease_policy_to_prerelease_policy,
    fb_var_opt_to_opt,
    fb_version_filter_to_version_filter,
    fb_version_to_version,
    get_build_from_fb_build_index,
    get_build_from_fb_pkg_request_with_options,
    opts_to_fb_opts,
    requirements_with_options_to_fb_requirements_with_options,
    version_to_fb_version,
};
pub use input_variant::InputVariant;
pub use install_spec::InstallSpec;
pub use option::{Inheritance, Opt};
pub use package::{
    BuildOptions,
    Components,
    DownstreamRequirements,
    OptionValues,
    Package,
    PackageMut,
};
pub use recipe::{BuildEnv, Recipe};
pub use requirements_list::{RequirementsList, convert_requests_to_requests_with_options};
pub use serde_json;
pub use source_spec::{GitSource, LocalSource, ScriptSource, SourceSpec, TarSource};
pub use spec::{ApiVersion, Spec, SpecFileData, SpecRecipe, SpecTemplate, SpecTest, SpecVariant};
pub use spk_schema_foundation::ident::{
    self as ident,
    AnyIdent,
    BuildIdent,
    PinnableRequest,
    PinnedRequest,
    RequestWithOptions,
    VersionIdent,
};
pub use spk_schema_foundation::option_map::{self, OptionMap};
pub use spk_schema_foundation::{
    self as foundation,
    FromYaml,
    env,
    ident_build,
    ident_component,
    ident_ops,
    name,
    opt_name,
    spec_ops,
    version,
    version_range,
};
pub use template::{Template, TemplateData, TemplateExt};
pub use test::{Test, TestStage};
pub use v0::{AutoHostVars, IndexedPackage, RecipeComponentSpec, Script};
pub use validation::{ValidationRule, ValidationSpec};
pub use variant::{Variant, VariantExt};

#[cfg(test)]
#[path = "./version_range_test.rs"]
mod version_range_test;

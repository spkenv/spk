// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

mod embedded_build_spec;
mod embedded_install_spec;
mod embedded_package_spec;
mod embedded_recipe_install_spec;
mod embedded_recipe_spec;
mod package_spec;
mod platform;
mod recipe_build_spec;
mod recipe_component_spec;
mod recipe_install_spec;
mod recipe_spec;
mod requirements;
mod test_spec;
mod variant;
mod variant_spec;

pub use embedded_build_spec::EmbeddedBuildSpec;
pub use embedded_install_spec::EmbeddedInstallSpec;
pub use embedded_package_spec::EmbeddedPackageSpec;
pub use embedded_recipe_install_spec::EmbeddedRecipeInstallSpec;
pub use embedded_recipe_spec::EmbeddedRecipeSpec;
pub use package_spec::PackageSpec;
pub(crate) use package_spec::check_package_spec_satisfies_pkg_request;
pub use platform::Platform;
pub(crate) use recipe_build_spec::UncheckedRecipeBuildSpec;
pub use recipe_build_spec::{AutoHostVars, RecipeBuildSpec, Script};
pub use recipe_component_spec::RecipeComponentSpec;
pub use recipe_install_spec::RecipeInstallSpec;
pub use recipe_spec::RecipeSpec;
pub use requirements::Requirements;
pub use test_spec::TestSpec;
pub use variant::Variant;
pub use variant_spec::VariantSpec;

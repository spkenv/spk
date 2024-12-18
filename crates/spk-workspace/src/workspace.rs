// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::str::FromStr;

use spk_schema::name::{PkgName, PkgNameBuf};
use spk_schema::version::Version;
use spk_schema::{SpecTemplate, Template, TemplateExt};

use crate::error;

#[cfg(test)]
#[path = "workspace_test.rs"]
mod workspace_test;

/// A collection of recipes and build targets.
///
/// Workspaces are used to define and build many recipes
/// together, helping to produce complete environments
/// with shared compatibility requirements. Workspaces
/// can be used to determine the number and order of
/// packages to be built in order to efficiently satisfy
/// and entire set of requirements for an environment.
#[derive(Default)]
pub struct Workspace {
    /// Spec templates available in this workspace.
    ///
    /// A workspace may contain multiple recipes for a single
    /// package.
    pub(crate) templates: HashMap<PkgNameBuf, Vec<ConfiguredTemplate>>,
}

#[derive(Debug)]
pub struct ConfiguredTemplate {
    pub template: SpecTemplate,
    pub config: crate::file::TemplateConfig,
}

impl Workspace {
    /// Create a new workspace [`crate::builder::WorkspaceBuilder`].
    pub fn builder() -> crate::builder::WorkspaceBuilder {
        crate::builder::WorkspaceBuilder::default()
    }

    /// Iterate over all templates in the workspace.
    pub fn iter(&self) -> impl Iterator<Item = (&PkgName, &ConfiguredTemplate)> {
        self.templates
            .iter()
            .flat_map(|(name, templates)| templates.iter().map(|t| (name.as_ref(), t)))
    }

    /// Returns the default package template file for the current workspace.
    ///
    /// The default template in a workspace is a lone template file, and
    /// this function will return an error if there is more than one template.
    pub fn default_package_template(&self) -> FindPackageTemplateResult<'_> {
        let mut iter = self.iter();
        // This must catch and convert all the errors into the appropriate
        // FindPackageTemplateResult, e.g. NotFound(error_message), so
        // that find_package_recipe_from_template_or_repo() can operate
        // correctly.
        let Some((_name, template)) = iter.next() else {
            return FindPackageTemplateResult::NoTemplateFiles;
        };

        if iter.next().is_some() {
            let all = self.templates.values().flatten().collect();
            return FindPackageTemplateResult::MultipleTemplateFiles(all);
        };

        FindPackageTemplateResult::Found(template)
    }

    /// Find a package template file for the requested package, if any.
    ///
    /// A package name, name with version, or filename can be provided.
    pub fn find_package_template<S>(&self, package: S) -> FindPackageTemplateResult
    where
        S: AsRef<str>,
    {
        let package = package.as_ref();
        let found = if let Ok(name) = spk_schema::name::PkgName::new(package) {
            tracing::debug!("Find package template by name: {name}");
            self.find_package_templates(name)
        } else if let Ok(ident) = spk_schema::VersionIdent::from_str(package) {
            tracing::debug!("Find package template for version: {ident}");
            self.find_package_template_for_version(ident.name(), ident.version())
        } else {
            tracing::debug!("Find package template by path: {package}");
            self.find_package_template_by_file(std::path::Path::new(package))
        };

        if found.is_empty() {
            return FindPackageTemplateResult::NotFound(package.to_owned());
        }
        if found.len() > 1 {
            return FindPackageTemplateResult::MultipleTemplateFiles(found);
        }
        FindPackageTemplateResult::Found(found[0])
    }

    /// Like [`Self::find_package_template`], but further filters by package version.
    pub fn find_package_template_for_version(
        &self,
        package: &PkgName,
        version: &Version,
    ) -> Vec<&ConfiguredTemplate> {
        self.find_package_templates(package)
            .into_iter()
            .filter(|t| t.config.versions.is_empty() || t.config.versions.contains(version))
            .collect::<Vec<_>>()
    }

    /// Find a package templates for the requested package, if any.
    ///
    /// Either a package name or filename can be provided.
    pub fn find_package_templates(&self, name: &PkgName) -> Vec<&ConfiguredTemplate> {
        if let Some(templates) = self.templates.get(name) {
            templates.iter().collect()
        } else {
            Default::default()
        }
    }

    /// Find package templates by their file path, if any.
    pub fn find_package_template_by_file(
        &self,
        file: &std::path::Path,
    ) -> Vec<&ConfiguredTemplate> {
        self.templates
            .values()
            .flat_map(|templates| templates.iter())
            .filter(|t| t.template.file_path() == file)
            .collect()
    }

    /// Load an additional template into this workspace from an arbitrary path on disk.
    ///
    /// No checks are done to ensure that this template has not already been loaded
    /// or that it actually appears in/logically belongs in this workspace.
    pub fn load_template_file<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<&mut ConfiguredTemplate, error::BuildError> {
        self.load_template_file_with_config(path, Default::default())
    }

    /// Load an additional template into this workspace from an arbitrary path on disk.
    ///
    /// No checks are done to ensure that this template actually appears in or
    /// logically belongs in this workspace.
    pub fn load_template_file_with_config<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
        config: crate::file::TemplateConfig,
    ) -> Result<&mut ConfiguredTemplate, error::BuildError> {
        let template = spk_schema::SpecTemplate::from_file(path.as_ref()).map_err(|source| {
            error::BuildError::TemplateLoadError {
                file: path.as_ref().to_owned(),
                source,
            }
        })?;
        tracing::trace!(
            "Load template into workspace: {:?} [{}]",
            path.as_ref(),
            template.name().map(|n| n.as_str()).unwrap_or("<no name>")
        );
        let Some(name) = template.name() else {
            return Err(error::BuildError::UnnamedTemplate {
                file: path.as_ref().to_owned(),
            });
        };
        let loaded_path = template.file_path();
        let by_name = self.templates.entry(name.clone()).or_default();
        let existing = by_name
            .iter()
            .position(|t| t.template.file_path() == loaded_path);
        if let Some(existing) = existing {
            by_name[existing].config.update(config);
            Ok(&mut by_name[existing])
        } else {
            by_name.push(ConfiguredTemplate { template, config });
            Ok(by_name.last_mut().expect("just pushed something"))
        }
    }
}

/// The result of the [`Workspace::find_package_template`] function.
#[derive(Debug)]
pub enum FindPackageTemplateResult<'a> {
    /// A non-ambiguous package template file was found
    Found(&'a ConfiguredTemplate),
    /// No package was specifically requested, and there are multiple
    /// files in the current repository.
    MultipleTemplateFiles(Vec<&'a ConfiguredTemplate>),
    /// No package was specifically requested, and there no template
    /// files in the current repository.
    NoTemplateFiles,
    /// The template file was not found
    NotFound(String),
}

impl<'a> FindPackageTemplateResult<'a> {
    ///True if a template file was found
    pub fn is_found(&self) -> bool {
        matches!(self, Self::Found { .. })
    }

    /// Prints error messages and exits if no template file was found
    pub fn must_be_found(self) -> &'a ConfiguredTemplate {
        match self {
            Self::Found(template) => return template,
            Self::MultipleTemplateFiles(templates) => {
                let mut here = std::env::current_dir().unwrap_or_default();
                here = here.canonicalize().unwrap_or(here);
                tracing::error!("Multiple package specs in current workspace:");
                for configured in templates {
                    // attempt to strip the current working directory from each path
                    // because in most cases it was loaded from the active workspace
                    // and the additional path prefix is just noise
                    let path = configured.template.file_path();
                    let path = path.strip_prefix(&here).unwrap_or(path).to_string_lossy();
                    let mut versions = configured
                        .config
                        .versions
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ");
                    if versions.is_empty() {
                        versions.push('?');
                    }
                    tracing::error!(" - {path} versions=[{versions}]",);
                }
                tracing::error!(" > ensure that you specify a package name, file path or version");
            }
            Self::NoTemplateFiles => {
                tracing::error!("No package specs found in current workspace");
                tracing::error!(" > please specify a filepath");
            }
            Self::NotFound(request) => {
                tracing::error!("Spec file not found for '{request}', or the file does not exist");
            }
        }
        std::process::exit(1);
    }
}

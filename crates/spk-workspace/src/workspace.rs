// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use spk_schema::name::{PkgName, PkgNameBuf};
use spk_schema::{SpecTemplate, Template, TemplateExt};

use crate::error;

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

pub struct ConfiguredTemplate {
    pub template: Arc<SpecTemplate>,
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

    /// Get the default package template file for the workspace.
    ///
    /// This only works if the workspace has a single template file
    /// that matches the workspace glob patterns.
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
            let files = self
                .templates
                .values()
                .flat_map(|templates| templates.iter().map(|t| t.template.file_path().to_owned()))
                .collect();
            return FindPackageTemplateResult::MultipleTemplateFiles(files);
        };

        FindPackageTemplateResult::Found(template)
    }

    /// Find a package template file for the requested package, if any.
    ///
    /// This function will use the current directory and the provided
    /// package name or filename to try and discover the matching
    /// yaml template file.
    pub fn find_package_template<S>(&self, package: &S) -> FindPackageTemplateResult
    where
        S: AsRef<str>,
    {
        let package = package.as_ref();

        if let Ok(name) = spk_schema::name::PkgName::new(package) {
            match self.templates.get(name) {
                Some(templates) if templates.len() == 1 => {
                    return FindPackageTemplateResult::Found(&templates[0]);
                }
                Some(templates) => {
                    return FindPackageTemplateResult::MultipleTemplateFiles(
                        templates
                            .iter()
                            .map(|t| t.template.file_path().to_owned())
                            .collect(),
                    );
                }
                None => {}
            }
        }

        for entry in self.templates.values().flatten() {
            if entry.template.file_path() == std::path::Path::new(package) {
                return FindPackageTemplateResult::Found(entry);
            }
        }

        FindPackageTemplateResult::NotFound(package.to_owned())
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
    /// No checks are done to ensure that this template has not already been loaded
    /// or that it actually appears in/logically belongs in this workspace.
    pub fn load_template_file_with_config<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
        config: crate::file::TemplateConfig,
    ) -> Result<&mut ConfiguredTemplate, error::BuildError> {
        let template = spk_schema::SpecTemplate::from_file(path.as_ref())
            .map(Arc::new)
            .map_err(|source| error::BuildError::TemplateLoadError {
                file: path.as_ref().to_owned(),
                source,
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
        let by_name = self.templates.entry(name.clone()).or_default();
        by_name.push(ConfiguredTemplate { template, config });
        Ok(by_name.last_mut().expect("just pushed something"))
    }
}

/// The result of the [`Workspace::find_package_template`] function.
#[derive(Debug)]
pub enum FindPackageTemplateResult<'a> {
    /// A non-ambiguous package template file was found
    Found(&'a ConfiguredTemplate),
    /// No package was specifically requested, and there are multiple
    /// files in the current repository.
    MultipleTemplateFiles(BTreeSet<std::path::PathBuf>),
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
            Self::MultipleTemplateFiles(files) => {
                tracing::error!("Multiple package specs in current workspace:");
                for file in files {
                    tracing::error!("- {}", file.into_os_string().to_string_lossy());
                }
                tracing::error!(" > please specify a package name or filepath");
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

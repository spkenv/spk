// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::sync::Arc;

use spk_schema::name::PkgNameBuf;
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
    /// package, and templates may also not have a package name
    /// defined inside.
    pub(crate) templates: HashMap<Option<PkgNameBuf>, Vec<Arc<SpecTemplate>>>,
}

impl Workspace {
    pub fn builder() -> crate::builder::WorkspaceBuilder {
        crate::builder::WorkspaceBuilder::default()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Option<PkgNameBuf>, &Arc<SpecTemplate>)> {
        self.templates
            .iter()
            .flat_map(|(key, entries)| entries.iter().map(move |e| (key, e)))
    }

    pub fn default_package_template(&self) -> FindPackageTemplateResult {
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
                .flat_map(|templates| templates.iter().map(|t| t.file_path().to_owned()))
                .collect();
            return FindPackageTemplateResult::MultipleTemplateFiles(files);
        };

        FindPackageTemplateResult::Found(Arc::clone(template))
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

        if let Ok(name) = spk_schema::name::PkgNameBuf::try_from(package) {
            match self.templates.get(&Some(name)) {
                Some(templates) if templates.len() == 1 => {
                    return FindPackageTemplateResult::Found(Arc::clone(&templates[0]));
                }
                Some(templates) => {
                    return FindPackageTemplateResult::MultipleTemplateFiles(
                        templates.iter().map(|t| t.file_path().to_owned()).collect(),
                    );
                }
                None => {}
            }
        }

        for template in self.templates.values().flatten() {
            if template.file_path() == std::path::Path::new(package) {
                return FindPackageTemplateResult::Found(Arc::clone(template));
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
    ) -> Result<&Arc<SpecTemplate>, error::BuildError> {
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
        let by_name = self.templates.entry(template.name().cloned()).or_default();
        by_name.push(template);
        Ok(by_name.last().expect("just pushed something"))
    }
}

/// The result of the [`Workspace::find_package_template`] function.
// We are okay with the large variant here because it's specifically
// used as the positive result of the function, with the others simply
// denoting unique error cases.
#[allow(clippy::large_enum_variant)]
pub enum FindPackageTemplateResult {
    /// A non-ambiguous package template file was found
    Found(Arc<SpecTemplate>),
    /// No package was specifically requested, and there are multiple
    /// files in the current repository.
    MultipleTemplateFiles(Vec<std::path::PathBuf>),
    /// No package was specifically requested, and there no template
    /// files in the current repository.
    NoTemplateFiles,
    NotFound(String),
}

impl FindPackageTemplateResult {
    pub fn is_found(&self) -> bool {
        matches!(self, Self::Found { .. })
    }

    /// Prints error messages and exits if no template file was found
    pub fn must_be_found(self) -> Arc<SpecTemplate> {
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

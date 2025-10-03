// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::str::FromStr;

use spk_schema::name::{PkgName, PkgNameBuf};
use spk_schema::template::DiscoverVersions;
use spk_schema::version_range::{LowestSpecifiedRange, Ranged};
use spk_schema::{SpecTemplate, Template, TemplateExt};

use crate::error::{self, BuildError};

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
#[derive(Debug, Default, Clone)]
pub struct Workspace {
    root: Option<std::path::PathBuf>,
    /// Spec templates available in this workspace.
    ///
    /// A workspace may contain multiple recipes for a single
    /// package.
    pub(crate) templates: HashMap<PkgNameBuf, Vec<SpecTemplate>>,
}

impl Workspace {
    /// Create a new workspace [`crate::builder::WorkspaceBuilder`].
    pub fn builder() -> crate::builder::WorkspaceBuilder {
        crate::builder::WorkspaceBuilder::default()
    }

    /// The logical root directory for this workspace.
    ///
    /// May be none in cases where the workspace was constructed
    /// manually or is the default.
    pub fn root(&self) -> Option<&std::path::Path> {
        self.root.as_deref()
    }

    /// Iterate over all templates in the workspace.
    pub fn iter(&self) -> impl Iterator<Item = (&PkgName, &SpecTemplate)> {
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
            return Err(FindPackageTemplateError::NoTemplateFiles);
        };

        if iter.next().is_some() {
            let all = self.templates.values().flatten().cloned().collect();
            return Err(FindPackageTemplateError::MultipleTemplates(all));
        };

        Ok(template)
    }

    /// Find a package template by name or filepath.
    ///
    /// If there is no existing template loaded for the provided argument
    /// but it is a valid file path then load it into the workspace.
    pub fn find_or_load_package_template<S>(
        &mut self,
        package: S,
    ) -> Result<&SpecTemplate, FindOrLoadPackageTemplateError>
    where
        S: AsRef<str>,
    {
        if let Err(FindPackageTemplateError::NotFound(_)) =
            self.find_package_template(package.as_ref())
            && std::fs::exists(package.as_ref()).ok().unwrap_or_default()
        {
            self.load_template_file(package.as_ref())?;
        }
        // NOTE: it would be preferable not to run this function twice and instead return
        // the value of the first call when it succeeds but doing so results in the
        // inability to take a mutable reference of self for the loading logic above.
        // This appears to be a limitation of the compiler so maybe it can be reworked
        // in the future
        self.find_package_template(package).map_err(From::from)
    }

    /// Find a package template file for the requested package, if any.
    ///
    /// A package name, name with version, or filename can be provided.
    pub fn find_package_template<S>(&self, package: S) -> FindPackageTemplateResult<'_>
    where
        S: AsRef<str>,
    {
        let package = package.as_ref();
        let found = if let Ok(name) = spk_schema::name::PkgName::new(package) {
            tracing::debug!("Find package template by name: {name}");
            self.find_package_templates(name)
        } else if let Ok(ident) = spk_schema::VersionIdent::from_str(package) {
            // The lowest specified range is preferred when valid, because it
            // allows for the user to specify something like `python/3` to disambiguate
            // between major versions without needing to select an exact/complete version.
            let range = LowestSpecifiedRange::new(ident.version().clone());
            tracing::debug!(
                "Find package template for version: {}/{range}",
                ident.name()
            );
            self.find_package_template_for_version(ident.name(), range)
        } else {
            tracing::debug!("Find package template by path: {package}");
            self.find_package_template_by_file(std::path::Path::new(package))
        };

        if found.is_empty() {
            return Err(FindPackageTemplateError::NotFound(package.to_owned()));
        }
        if found.len() > 1 {
            return Err(FindPackageTemplateError::MultipleTemplates(
                found.into_iter().cloned().collect(),
            ));
        }
        Ok(found[0])
    }

    /// Like [`Self::find_package_template`], but further filters by package version.
    pub fn find_package_template_for_version<R: Ranged>(
        &self,
        package: &PkgName,
        range: R,
    ) -> Vec<&SpecTemplate> {
        self.find_package_templates(package)
            .into_iter()
            .filter(|t| {
                t.discover_versions()
                    .is_ok_and(|v| v.iter().any(|v| range.is_applicable(v).is_ok()))
            })
            .collect::<Vec<_>>()
    }

    /// Find a package templates for the requested package, if any.
    ///
    /// Either a package name or filename can be provided.
    pub fn find_package_templates(&self, name: &PkgName) -> Vec<&SpecTemplate> {
        if let Some(templates) = self.templates.get(name) {
            templates.iter().collect()
        } else {
            Default::default()
        }
    }

    /// Like [`Self::find_package_templates`], but returns mutable references.
    pub fn find_package_templates_mut(&mut self, name: &PkgName) -> Vec<&mut SpecTemplate> {
        if let Some(templates) = self.templates.get_mut(name) {
            templates.iter_mut().collect()
        } else {
            Default::default()
        }
    }

    /// Find package templates by their file path, if any.
    pub fn find_package_template_by_file(&self, file: &std::path::Path) -> Vec<&SpecTemplate> {
        // Attempt to canonicalize `file` using the same function that the
        // workspace uses as it locates files, to have a chance of matching
        // one of the entries in the workspace by comparing to its
        // `file_path()`.
        let file_path = dunce::canonicalize(file).unwrap_or_else(|_| file.to_owned());
        self.templates
            .values()
            .flat_map(|templates| templates.iter())
            .filter(|t| t.file_path() == file_path)
            .collect()
    }

    /// Load an additional template into this workspace from an arbitrary path on disk.
    ///
    /// No checks are done to ensure that this template has not already been loaded
    /// or that it actually appears in/logically belongs in this workspace.
    pub fn load_template_file<P: AsRef<std::path::Path>>(
        &mut self,
        path: P,
    ) -> Result<&mut SpecTemplate, error::BuildError> {
        let template = spk_schema::SpecTemplate::from_file(path.as_ref()).map_err(|source| {
            error::BuildError::TemplateLoadError {
                file: path.as_ref().to_owned(),
                source: Box::new(source),
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
        let by_name = self.templates.entry(name.clone()).or_default();
        by_name.push(template);
        Ok(by_name.last_mut().expect("just pushed something"))
    }
}

/// Possible errors from the [`Workspace::find_or_load_package_template`] function.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[diagnostic(
    url(
        "https://spkenv.dev/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spk::generic"))
    )
)]
pub enum FindOrLoadPackageTemplateError {
    /// The template could not be found
    #[error(transparent)]
    #[diagnostic(forward(0))]
    FindPackageTemplateError(#[from] FindPackageTemplateError),
    /// The template file could not be loaded or had errors
    #[error(transparent)]
    #[diagnostic(forward(0))]
    BuildError(#[from] BuildError),
}

/// The result of the [`Workspace::find_package_template`] function.
pub type FindPackageTemplateResult<'workspace> =
    Result<&'workspace SpecTemplate, FindPackageTemplateError>;

/// Possible errors for [`Workspace::find_package_template`] and related functions.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[diagnostic(
    url(
        "https://spkenv.dev/error_codes#{}",
        self.code().unwrap_or_else(|| Box::new("spk::generic"))
    )
)]
pub enum FindPackageTemplateError {
    /// No package was specifically requested, and there are multiple
    /// files in the current repository.
    #[error("Multiple package specs in current workspace:\n{}", self.formatted_packages_list())]
    #[diagnostic(help = "ensure that you specify a package name, file path or version")]
    MultipleTemplates(Vec<SpecTemplate>),
    /// No package was specifically requested, and there no template
    /// files in the current repository.
    #[error("No package specs found in current workspace")]
    #[diagnostic(help = "please specify a spec file path")]
    NoTemplateFiles,
    /// The template file was not found
    #[error("Spec file not found for '{0}', or the file does not exist")]
    NotFound(String),
}

impl FindPackageTemplateError {
    /// Generates a list-formatted, one-per-line string of the available templates
    /// when the error contains such information. Eg for [`FindPackageTemplateError::MultipleTemplates`].
    fn formatted_packages_list(&self) -> String {
        let Self::MultipleTemplates(templates) = self else {
            return String::new();
        };
        let mut lines = Vec::with_capacity(templates.len());
        let mut here = std::env::current_dir().unwrap_or_default();
        here = here.canonicalize().unwrap_or(here);
        for configured in templates {
            // attempt to strip the current working directory from each path
            // because in most cases it was loaded from the active workspace
            // and the additional path prefix is just noise
            let path = configured.file_path();
            let path = path.strip_prefix(&here).unwrap_or(path).to_string_lossy();
            let mut versions = configured
                .discover_versions()
                .inspect_err(|err| {
                    tracing::warn!(?path, "encountered error discovering versions: {err}")
                })
                .unwrap_or_default()
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            if versions.is_empty() {
                versions.push_str("<any>");
            }
            lines.push(format!(" - {path} versions=[{versions}]"));
        }
        lines.join("\n")
    }
}

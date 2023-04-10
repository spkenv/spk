// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::iter::FromIterator;
use std::sync::Arc;

use colored::Colorize;
use spfs::Digest;
use spk_schema::foundation::format::{
    FormatChangeOptions,
    FormatOptionMap,
    FormatRequest,
    FormatSolution,
};
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::option_map::OptionMap;
use spk_schema::foundation::version::VERSION_SEP;
use spk_schema::ident::{PkgRequest, RequestedBy};
use spk_schema::name::{PkgNameBuf, RepositoryNameBuf};
use spk_schema::prelude::*;
use spk_schema::version::Version;
use spk_schema::{BuildEnv, BuildIdent, Package, Spec, SpecRecipe, VersionIdent};
use spk_storage::RepositoryHandle;

use crate::{Error, Result};

const SOLUTION_FORMAT_EMPTY_REPORT: &str = "Nothing Installed";
const SOLUTION_FORMAT_HEADING: &str = "Installed Packages:\n";
const SOLUTION_FORMAT_FOOTER: &str = "Number of Packages:";

const PACKAGE_COLUMN: usize = 0;
const HIGHEST_VERSION_COLUMN: usize = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageSource {
    Repository {
        /// the actual repository that this package was loaded from
        repo: Arc<RepositoryHandle>,
        /// the components that can be used for this package from the repository
        components: HashMap<Component, spfs::encoding::Digest>,
    },
    /// The package needs to be build from the given recipe.
    BuildFromSource {
        /// The recipe that this package is to be built from.
        recipe: Arc<SpecRecipe>,
    },
    /// The package was embedded in another.
    Embedded { parent: BuildIdent },
    /// Only for a package being used in spk' automated (unit) test code
    /// when the source of the package is not relevant for the test.
    SpkInternalTest,
}

impl PackageSource {
    pub fn is_build_from_source(&self) -> bool {
        matches!(self, Self::BuildFromSource { .. })
    }

    pub async fn read_recipe(&self, ident: &VersionIdent) -> Result<Arc<SpecRecipe>> {
        match self {
            PackageSource::BuildFromSource { recipe } => Ok(Arc::clone(recipe)),
            PackageSource::Repository { repo, .. } => Ok(repo.read_recipe(ident).await?),
            PackageSource::Embedded { .. } => {
                // TODO: what are the implications of this?
                Err(Error::String("Embedded package has no recipe".into()))
            }
            PackageSource::SpkInternalTest => Err(Error::String(
                "Spk Internal test package has no recipe. Please use another PackageSource value if you need to read a recipe during this test.".into(),
            )),
        }
    }
}

impl Ord for PackageSource {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;

        use PackageSource::*;
        match (self, other) {
            (this @ Repository { .. }, other @ Repository { .. }) => this.cmp(other),
            (Repository { .. }, BuildFromSource { .. } | Embedded { .. } | SpkInternalTest) => {
                Ordering::Less
            }
            (BuildFromSource { .. } | Embedded { .. } | SpkInternalTest, Repository { .. }) => {
                Ordering::Greater
            }
            (Embedded { .. }, Embedded { .. }) => Ordering::Equal,
            (Embedded { .. }, SpkInternalTest) => Ordering::Greater,
            (SpkInternalTest, Embedded { .. }) => Ordering::Less,
            (Embedded { .. } | SpkInternalTest, BuildFromSource { .. }) => Ordering::Less,
            (BuildFromSource { .. }, Embedded { .. } | SpkInternalTest) => Ordering::Greater,
            (BuildFromSource { recipe: this }, BuildFromSource { recipe: other }) => {
                this.ident().cmp(other.ident())
            }
            (SpkInternalTest, SpkInternalTest) => Ordering::Equal,
        }
    }
}

impl PartialOrd for PackageSource {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Represents a package request that has been resolved.
#[derive(Clone, Debug)]
pub struct SolvedRequest {
    pub request: PkgRequest,
    pub spec: Arc<Spec>,
    pub source: PackageSource,
}

impl SolvedRequest {
    pub fn is_source_build(&self) -> bool {
        matches!(self.source, PackageSource::BuildFromSource { .. })
    }

    /// Format this solved request as an installed package(build)
    pub fn format_as_installed_package(&self) -> String {
        let mut installed =
            PkgRequest::from_ident(self.spec.ident().to_any(), RequestedBy::DoesNotMatter);

        let mut repo_name: Option<RepositoryNameBuf> = None;
        if let PackageSource::Repository { repo, components } = &self.source {
            repo_name = Some(repo.name().to_owned());

            let mut installed_components = self.request.pkg.components.clone();
            if installed_components.remove(&Component::All) {
                installed_components.extend(components.keys().cloned());
            }
            installed.pkg.components = installed_components;
        }

        // Pass zero verbosity to format_request(), via the format
        // change options, to stop it outputting the internal details.
        installed.format_request(
            repo_name.as_ref(),
            self.spec.name(),
            &FormatChangeOptions::default(),
        )
    }

    /// Format the packages that required this solved request as one
    /// of their dependencies.
    pub(crate) fn format_package_requesters(&self) -> String {
        let requested_by: Vec<String> = self
            .request
            .get_requesters()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<String>>();
        format!("(required by {})", requested_by.join(", "))
    }

    /// Format the options for this solved request (build)
    pub(crate) fn format_package_options(&self) -> String {
        let options = self.spec.option_values();
        options.format_option_map()
    }

    /// Returns the spfs layers in a resolved request, from its source
    /// repo, in a mapping of components to layers. This results in an
    /// error for packages without a source repo: an
    /// EmbeddedHasNoComponentLayers error if the package is embedded,
    /// a String error if the package is a source package, and a
    /// SpkInternalTestHasNoComponentLayers error if the package comes
    /// from an internal test.
    pub fn component_layers(&self) -> Result<HashMap<Component, Digest>> {
        let spfs_layers = match &self.source {
            PackageSource::Embedded { .. } => {
                // Embedded builds are provided by another package in
                // the solve. They do not have a layer of their own.
                return Err(Error::EmbeddedHasNoComponentLayers);
            }
            PackageSource::BuildFromSource { .. } => {
                // Packages that need building do not have layers yet.
                return Err(Error::String(format!("Cannot bake, solution requires packages that need building - Request for: {}, Resolved to: {}", self.request.pkg, self.spec.ident())));
            }
            PackageSource::Repository {
                repo: _,
                components,
            } => {
                let mut requested_components = self.request.pkg.components.clone();
                if requested_components.remove(&Component::All) {
                    components.clone()
                } else {
                    components
                        .iter()
                        .filter_map(|(c, l)| {
                            if requested_components.contains(c) {
                                Some((c.clone(), *l))
                            } else {
                                None
                            }
                        })
                        .collect::<HashMap<Component, Digest>>()
                }
            }
            PackageSource::SpkInternalTest => {
                // Internal test builds don't have a layer of
                // their own so they can be skipped over.
                return Err(Error::SpkInternalTestHasNoComponentLayers);
            }
        };

        Ok(spfs_layers)
    }

    /// Get the name of the repo for this solved request, if it comes
    /// from a repo, otherwise return None.
    pub fn repo_name(&self) -> Option<RepositoryNameBuf> {
        match &self.source {
            PackageSource::Embedded { .. } => {
                // These do not come directly from a repo. Their
                // parent/containing package might, but that's not
                // available here yet.
                None
            }
            PackageSource::BuildFromSource { .. } => {
                // Packages that need building are not in a repo yet.
                None
            }
            PackageSource::Repository {
                repo,
                components: _,
            } => Some(repo.name().into()),
            PackageSource::SpkInternalTest => None,
        }
    }
}

/// A pairing of a solved request and a list of the components (names)
/// it provides.
pub struct LayerPackageAndComponents<'a>(pub &'a SolvedRequest, pub Vec<Component>);

/// Helper to get a mapping from spfs layers (digests) to packages and
/// their components' names, for the given list of solved requests.
pub fn get_spfs_layers_to_packages<'a>(
    //resolved_items: &Vec<SolvedRequest>,
    resolved_items: &'a std::slice::Iter<'a, SolvedRequest>,
) -> Result<HashMap<Digest, LayerPackageAndComponents<'a>>> {
    // The layer(s) for the packages come from their source repos
    let mut layers_to_packages: HashMap<Digest, LayerPackageAndComponents> = HashMap::new();
    for resolved in resolved_items.clone() {
        let spfs_layers = match resolved.component_layers() {
            Ok(layers) => layers,
            Err(Error::EmbeddedHasNoComponentLayers) => continue,
            Err(Error::SpkInternalTestHasNoComponentLayers) => continue,
            Err(err) => return Err(err),
        };

        for (component, layer) in spfs_layers.iter() {
            let entry = layers_to_packages
                .entry(*layer)
                .or_insert(LayerPackageAndComponents(resolved, vec![]));
            entry.1.push(component.clone());
        }
    }

    Ok(layers_to_packages)
}

/// Represents a set of resolved packages.
#[derive(Clone, Debug, Default)]
pub struct Solution {
    options: OptionMap,
    resolved: Vec<SolvedRequest>,
}

impl Solution {
    pub fn new(options: OptionMap) -> Self {
        Self {
            options,
            resolved: Default::default(),
        }
    }

    pub fn items(&self) -> std::slice::Iter<'_, SolvedRequest> {
        self.resolved.iter()
    }

    pub fn get<S: AsRef<str>>(&self, name: S) -> Option<&SolvedRequest> {
        self.resolved
            .iter()
            .find(|r| r.request.pkg.name.as_str() == name.as_ref())
    }

    pub fn options(&self) -> &OptionMap {
        &self.options
    }

    /// The number of packages in this solution
    #[inline]
    pub fn len(&self) -> usize {
        self.resolved.len()
    }

    /// True if there are no packages in this solution
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.resolved.is_empty()
    }

    /// Add a resolved request to this solution
    pub fn add(&mut self, request: PkgRequest, spec: Arc<Spec>, source: PackageSource) {
        let existing = self.resolved.iter_mut().find(|r| r.request == request);
        let new = SolvedRequest {
            request,
            spec,
            source,
        };
        match existing {
            Some(existing) => {
                *existing = new;
            }
            None => self.resolved.push(new),
        }
    }

    /// Return the set of repositories in this solution.
    pub fn repositories(&self) -> Vec<Arc<RepositoryHandle>> {
        let mut seen = HashSet::new();
        let mut repos = Vec::new();
        for resolved in self.resolved.iter() {
            if let PackageSource::Repository { repo, .. } = &resolved.source {
                let addr = repo.address();
                if seen.contains(addr) {
                    continue;
                }
                repos.push(repo.clone());
                seen.insert(addr);
            }
        }
        repos
    }

    /// Return the data of this solution as environment variables.
    ///
    /// If base is given, also clean any existing, conflicting values.
    pub fn to_environment<V>(&self, base: Option<V>) -> HashMap<String, String>
    where
        V: IntoIterator<Item = (String, String)>,
    {
        let mut out = base
            .map(IntoIterator::into_iter)
            .map(HashMap::from_iter)
            .unwrap_or_default();

        out.retain(|name, _| !name.starts_with("SPK_PKG_"));

        out.insert("SPK_ACTIVE_PREFIX".to_owned(), "/spfs".to_owned());
        for resolved in self.resolved.iter() {
            let spec = &resolved.spec;
            out.insert(format!("SPK_PKG_{}", spec.name()), spec.ident().to_string());
            out.insert(
                format!("SPK_PKG_{}_VERSION", spec.name()),
                spec.version().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_BUILD", spec.name()),
                spec.ident().build().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_MAJOR", spec.name()),
                spec.version().major().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_MINOR", spec.name()),
                spec.version().minor().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_PATCH", spec.name()),
                spec.version().patch().to_string(),
            );
            out.insert(
                format!("SPK_PKG_{}_VERSION_BASE", spec.name()),
                spec.version()
                    .parts
                    .iter()
                    .map(|v| v.to_string())
                    .collect::<Vec<String>>()
                    .join(VERSION_SEP),
            );
        }

        out.extend(self.options.to_environment().into_iter());
        out
    }

    /// Helper to find the highest version number for package across
    /// all the given repositories.
    pub async fn find_highest_package_version(
        &self,
        name: PkgNameBuf,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<Arc<Version>> {
        let mut max_version = Arc::new(Version::default());
        for repo in repos.iter() {
            if let Some(highest_version) = repo.highest_package_version(&name).await? {
                if highest_version > max_version {
                    max_version = highest_version;
                }
            };
        }
        Ok(max_version)
    }

    /// Helper to get the highest versions of all packages in this `Solution` in all the
    /// given repositories.
    pub async fn get_all_highest_package_versions(
        &self,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<HashMap<PkgNameBuf, Arc<Version>>> {
        let mut highest_versions: HashMap<PkgNameBuf, Arc<Version>> = HashMap::new();

        for name in self.resolved.iter().map(|r| r.request.pkg.name.clone()) {
            let max_version = self
                .find_highest_package_version(name.clone(), repos)
                .await?;
            highest_versions.insert(name.clone(), max_version);
        }
        Ok(highest_versions)
    }

    /// Format the solution and include whether or not each resolved
    /// package is also the highest version available in the repositories
    pub async fn format_solution_with_highest_versions(
        &self,
        verbosity: u8,
        repos: &[Arc<RepositoryHandle>],
    ) -> Result<String> {
        if self.is_empty() {
            return Ok(SOLUTION_FORMAT_EMPTY_REPORT.to_string());
        }
        let highest_versions = self.get_all_highest_package_versions(repos).await?;

        Ok(self.format_solution_with_padding_and_highest(verbosity, &highest_versions))
    }

    fn format_solution_without_padding_or_highest(&self, verbosity: u8) -> String {
        let mut out = SOLUTION_FORMAT_HEADING.to_string();

        // The resolved packages are typically stored in resolve
        // order, but we want to display them here in alphabetical
        // order by package name.
        let mut required_items = self.resolved.clone();
        required_items.sort_by(|a, b| a.spec.name().cmp(b.spec.name()));

        let number_of_packages = required_items.len();
        for req in required_items {
            // Show the installed request with components and repo name included
            let _ = write!(out, "  {}", req.format_as_installed_package());

            if verbosity > 0 {
                // Show all the things that requested this package
                let _ = write!(out, " {} ", req.format_package_requesters());

                if verbosity > 1 {
                    // Show the options for this package (build)
                    let _ = write!(out, " {}", req.format_package_options());
                }
            }
            out.push('\n');
        }

        let _ = write!(out, " {SOLUTION_FORMAT_FOOTER} {number_of_packages}");
        out
    }

    fn format_solution_with_padding_and_highest(
        &self,
        verbosity: u8,
        highest_versions: &HashMap<PkgNameBuf, Arc<Version>>,
    ) -> String {
        let mut out = SOLUTION_FORMAT_HEADING.to_string();

        let required_items = self.items();
        let number_of_packages = required_items.len();

        let mut max_widths: Vec<usize> = vec![0, 0, 0, 0];
        let mut data: Vec<Vec<(usize, String)>> = Vec::with_capacity(number_of_packages);

        // This only pads the first 2 columns at the moment: the
        // packages and the highest_versions. The remaining columns
        // are unpadded.
        for req in required_items {
            let mut line: Vec<(usize, String)> = Vec::new();

            // Get installed request with components and repo name included
            let package = req.format_as_installed_package();

            let l = console::measure_text_width(&package);
            if l > max_widths[PACKAGE_COLUMN] {
                max_widths[PACKAGE_COLUMN] = l;
            }
            line.push((l, package));

            // Add whether this request is for the highest version of
            // the package, or what the highest version of the package is.
            let highest_label = match highest_versions.get(req.spec.name()) {
                Some(highest_version) => {
                    if *req.spec.ident().version() == **highest_version {
                        "highest".green()
                    } else {
                        highest_version.to_string().yellow()
                    }
                }
                None => "".black(),
            };

            let l = console::measure_text_width(&highest_label);
            if l > max_widths[HIGHEST_VERSION_COLUMN] {
                max_widths[HIGHEST_VERSION_COLUMN] = l;
            }
            line.push((l, highest_label.to_string()));

            // Optionally, add the last 2 columns: the things that
            // requested this package, and the package's options
            if verbosity > 0 {
                // Zero because not padding this value's column
                line.push((0, req.format_package_requesters()));

                if verbosity > 1 {
                    // Zero because not padding this value's column
                    line.push((0, req.format_package_options()));
                }
            }

            data.push(line);
        }

        // Output the data, one package per line, with padding between
        // the values on each line.
        for line in data {
            out.push_str("  ");

            for (col_index, (length, value)) in line.into_iter().enumerate() {
                let mut max_width = max_widths[col_index];
                if max_width == 0 {
                    max_width = length
                }
                let padding = " ".repeat(max_width - length);

                let _ = write!(out, "{value}{padding} ");
            }

            out.push('\n');
        }

        let _ = write!(out, " {SOLUTION_FORMAT_FOOTER} {number_of_packages}");
        out
    }
}

impl BuildEnv for Solution {
    type Package = Arc<Spec>;

    fn build_env(&self) -> Vec<Self::Package> {
        self.resolved
            .iter()
            .map(|resolved| Arc::clone(&resolved.spec))
            .collect::<Vec<_>>()
    }
}

impl FormatSolution for Solution {
    fn format_solution(&self, verbosity: u8) -> String {
        if self.is_empty() {
            return SOLUTION_FORMAT_EMPTY_REPORT.to_string();
        }

        self.format_solution_without_padding_or_highest(verbosity)
    }
}

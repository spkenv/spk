// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_stream::try_stream;
use futures::{Stream, TryStreamExt};
use itertools::Itertools;
use miette::Result;
use spfs::encoding::Digest;
use spk_schema::ident::{RangeIdent, parse_ident_range};
use spk_schema::ident_build::Build;
use spk_schema::ident_component::Component;
use spk_schema::ident_ops::parsing::KNOWN_REPOSITORY_NAMES;
use spk_schema::name::PkgNameBuf;
use spk_schema::version::Version;
use spk_schema::{BuildIdent, Deprecate, Package, VersionIdent};

use crate::walker::{
    RepoWalkerBuilder,
    RepoWalkerFilter,
    RepoWalkerItem,
    WalkedBuild,
    WalkedComponent,
    WalkedFile,
    WalkedPackage,
    WalkedVersion,
};
use crate::{Error, RepositoryHandle};

/// The storage path separator used between directory levels
pub const LEVEL_SEPARATOR: char = '/';

/// Substrings that indicate components in various formats
const COMPONENTS_MARKER: &str = "/:";
const COMPONENTS_SEPARATOR: &str = ":";

/// Indices for determining parts of a package path format or package
/// request format
const REPO_INDEX: usize = 0;
const NAME_INDEX: usize = 1;
const VERSION_INDEX: usize = 2;
const BUILD_INDEX: usize = 3;

/// Disk usage of a entry
#[derive(Clone, Debug, Default, Eq, Ord, PartialOrd, PartialEq)]
pub struct EntryDiskUsage {
    path: Vec<Arc<String>>,
    size: u64,
    digest: Digest,
}

impl EntryDiskUsage {
    pub fn new(path: Vec<Arc<String>>, size: u64, digest: Digest) -> Self {
        Self { path, size, digest }
    }

    /// Return a list of the path pieces to this entry
    pub fn path(&self) -> &Vec<Arc<String>> {
        &self.path
    }

    /// Return the raw bytes size
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Return the size as human readable string with units.
    pub fn human_readable(&self) -> String {
        spfs::io::format_size(self.size)
    }

    /// Return the spfs digest of this entry
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
}

/// Stores the values of the package being walked.
/// Starting from the repo name, it will store the
/// absolute path up to an entry blob from the /spfs dir.
#[derive(Clone, Debug)]
pub struct PackageDiskUsage {
    pub repo_name: String,
    pub pkg: PkgNameBuf,
    pub version: Arc<Version>,
    pub build: Build,
    pub component: Component,
    pub entry: EntryDiskUsage,
    pub deprecated: bool,
}

impl PackageDiskUsage {
    /// Construct an empty PackageDiskUsage
    fn new(pkgname: PkgNameBuf, repo_name: String) -> Self {
        Self {
            pkg: pkgname,
            version: Version::default().into(),
            build: Build::empty().clone(),
            component: Component::default_for_build(),
            entry: EntryDiskUsage::default(),
            deprecated: false,
            repo_name,
        }
    }

    /// Constructs a path from the values in PackageDiskUsage.
    pub fn flatten_path(&self) -> Vec<String> {
        let component = format!(":{}", self.component);
        let mut path = vec![
            self.repo_name.clone(),
            self.pkg.to_string(),
            self.version.to_string(),
            self.build.to_string(),
            component,
        ];

        self.entry
            .path()
            .iter()
            .for_each(|p| path.push(p.to_string()));

        path
    }

    /// Calculate the depth of this PackageDiskUsage.
    pub fn depth(&self) -> usize {
        // The 5 comes from accounting for the repo, pkg, version,
        // build, and component fields/levels that each
        // PackageDiskUsage has.
        let depth = 5 + self.entry.path().len();
        depth
    }

    /// Generates a partial path from the stored values from
    /// PackageDiskUsage down to the given depth. Returns None if
    /// there aren't enough values to reach the given depth.
    fn generate_partial_path(&self, depth: usize) -> Option<String> {
        let mut abs_path = self.flatten_path();
        let max_depth = abs_path.len();

        if depth.lt(&max_depth) {
            abs_path.truncate(depth);
            Some(format!(
                "{}{LEVEL_SEPARATOR}",
                abs_path.join(&LEVEL_SEPARATOR.to_string()),
            ))
        } else if depth.eq(&max_depth) {
            Some(abs_path.join(&LEVEL_SEPARATOR.to_string()))
        } else {
            None
        }
    }
}

/// Stores accumulated values for a group of entries being walked.
pub struct GroupedDiskUsage {
    pub grouping: String,
    pub size: u64,
    pub deprecated: bool,
}

impl GroupedDiskUsage {
    /// Return the size as human readable string with the appropriate
    /// units.
    pub fn human_readable(&self) -> String {
        spfs::io::format_size(self.size)
    }
}

/// Package level filters will be given the PackageDiskUsage entry and the initial input depth
pub type PackageDiskUsageFilterFunc<'a> =
    dyn Fn(&PackageDiskUsage, usize) -> bool + Send + Sync + 'a;

/// A stream of disk usage entries found by walking a list of spk
/// repos looking for a given package and file path below a given
/// depth. A DiskUsageRepoWalker can be used to return
/// PackageDiskUsage objects or GroupedDiskUsage objects depending on
/// whether the caller wants individual entries or summed up (grouped)
/// ones.
///
/// A DiskUsageRepoWalker supports various search and filtering
/// options but cannot be constructed directly. It is configured and
/// builds via a DiskUsageRepoWalkerBuilder. Call
/// individual_entries_du_walk() or grouped_du_walk() to get a stream
/// of individual or collated disk usage entries.
pub struct DiskUsageRepoWalker<'a> {
    /// Based on the du path format:
    ///  origin/package/version/build/component/dir/file/...

    /// This is number of levels in the path, e.g. if the du path is
    /// repo/package/version then the input depth would be 3. It is
    /// the depth given to the package du filter function calls.
    input_depth: usize,
    /// To count linked files separately for each link or only once
    count_links: bool,
    /// To include sizes from deprecated builds or not, used in the internal repo walker
    deprecated: bool,
    package_du_filter_func: Arc<PackageDiskUsageFilterFunc<'a>>,
    repo_walker_builder: RepoWalkerBuilder<'a>,
}

impl DiskUsageRepoWalker<'_> {
    /// Get a traversal of the disk usages of individual files in packages
    pub fn individual_entries_du_walk(
        &mut self,
    ) -> impl Stream<Item = Result<PackageDiskUsage>> + '_ {
        Box::pin(try_stream! {
            let repo_walker = self.repo_walker_builder
                .with_report_on_versions(true)
                .with_report_on_builds(true)
                .with_report_src_builds(true)
                .with_report_deprecated_builds(self.deprecated)
                .with_report_embedded_builds(false)
                .with_report_on_components(true)
                .with_report_on_files(true)
                .with_continue_on_error(true)
                .build();

            let mut traversal = repo_walker.walk();

            let mut visited_digests = HashSet::new();

            let mut current_repo_name = "";
            let mut pkg_name = Arc::new(PkgNameBuf::try_from("xyzzy").unwrap());
            let mut version = Arc::from(Version::new(0, 0, 0));
            let mut build = Build::Source;
            let mut component_name = Component::Run;
            let mut is_deprecated = false;

            while let Some(item) = traversal.try_next().await? {
                match item {
                    RepoWalkerItem::Package(package) => {
                        current_repo_name = package.repo_name;
                        pkg_name = package.name;
                    }
                    RepoWalkerItem::Version(version_item) => {
                        version = Arc::from(version_item.ident.version().clone());
                    }
                    RepoWalkerItem::Build(build_item) => {
                        build = build_item.spec.ident().build().clone();
                        is_deprecated = build_item.spec.is_deprecated();
                    }
                    RepoWalkerItem::Component(component) => {
                        component_name = component.name;
                    },
                    RepoWalkerItem::File(file) => {
                        let entry_size = if visited_digests.insert(file.entry.object) || self.count_links {
                            file.entry.size()
                        } else {
                            // Set to 0 because we don't need to calculate sizes
                            // when it is a duplicate and count_links is not enabled.
                            0
                        };

                        let disk_usage = EntryDiskUsage::new(
                            file.path_pieces.clone(),
                            entry_size,
                            file.entry.object,
                        );

                        // TODO: should Arc more things to reduce the
                        // repeating, change PackageDiskUsage?
                        let mut du = PackageDiskUsage::new((*pkg_name).clone(), current_repo_name.to_string());
                        du.version = Arc::clone(&version);
                        du.build = build.clone();
                        du.component = component_name.clone();
                        du.entry = disk_usage;
                        du.deprecated = is_deprecated;

                        if (self.package_du_filter_func)(&du, self.input_depth) {
                            yield du
                        }
                    },
                    _ => {}
                }
            }
        })
    }

    /// Get a traversal of the disk usage of items on the configured
    /// path grouped together (summed up), e.g. a package/version or
    /// each of the builds under a package/version/.
    pub fn grouped_du_walk(&mut self) -> impl Stream<Item = Result<GroupedDiskUsage>> + '_ {
        Box::pin(try_stream! {
            // Only holds the one group being counted at the current time.
            // Entries are removed and output as they are finished.
            let mut grouped_entries: HashMap<String, (u64, bool)> = HashMap::new();
            let depth = self.input_depth;

            // This uses the fine grained disk usage walker that
            // returns individual entries, and groups them up before
            // yielding a completed grouped entry.
            let mut walked = self.individual_entries_du_walk();
            while let Some(du) = walked.try_next().await? {
                // The partial path is the grouping for the entry we are working on
                let partial_path = match du.generate_partial_path(depth) {
                    Some(path) => path,
                    // Have not reached the depth we are interested in.
                    _ => continue,
                };

                // If the partial path does not exist and
                // grouped_entries is not empty, then the existing
                // path has finished calculating and is ready to send.
                // This happens in when the package path or file path
                // (originally from the du command) was given with a
                // trailing slash, e.g. origin/python/3.9.7/. The
                // trailing slash indicates "give me the du of each
                // thing at the level below the given path", so each
                // build in the example.
                //
                // In those cases, there will be multiple different
                // partial grouped paths as each thing (e.g. build) is
                // processed.
                if !grouped_entries.contains_key(&partial_path) && !grouped_entries.is_empty() {
                    for (grouping, (size, deprecated)) in grouped_entries.drain().take(1) {
                        yield GroupedDiskUsage {
                            grouping: grouping.to_string(),
                            size,
                            deprecated,
                        };
                    }
                }

                grouped_entries
                    .entry(partial_path)
                    .and_modify(|(size, _)| *size += du.entry.size())
                    .or_insert((du.entry.size(), du.deprecated));
            }

            // Need to clear the last object inside grouped_entries.
            for (grouping, (size, deprecated)) in grouped_entries.iter() {
                yield GroupedDiskUsage {
                    grouping: grouping.to_string(),
                    size: *size,
                    deprecated: *deprecated,
                };
            }
        })
    }
}

/// A builder for constructing a DiskUsageRepoWalker from various settings.
///
/// A disk usage repo walker can be made with:
/// ```
/// use spk_storage::DiskUsageRepoWalkerBuilder;
/// # use spk_storage::local_repository;
/// # use spk_storage::Result;
/// # use futures::executor::block_on;
/// # fn main() -> Result<()> {
/// # let mut repo = block_on(local_repository())?;
/// # let repos = vec!(("local".to_string(), repo.into()));
/// let mut du_walker_builder = DiskUsageRepoWalkerBuilder::new(&repos);
/// let mut du_walker = du_walker_builder.build();
/// # Ok(())
/// # }
/// ```
/// That returns a disk usage repo walker that will report on all
/// files, counting symlinks once, in directories in all components of
/// all non-deprecated builds of all versions of all packages.
///
/// Other walkers can be made by using the with_* methods on
/// DiskUsageRepoWalkerBuilder for configuration before calling
/// [Self::build] to make the DiskUsageRepoWalker, e.g.
/// ```
/// use spk_storage::DiskUsageRepoWalkerBuilder;
/// use spk_schema::ident::parse_version_ident;
/// # use spk_storage::local_repository;
/// # use spk_storage::Result;
/// # use futures::executor::block_on;
/// # fn main() -> Result<()> {
/// # let mut repo = block_on(local_repository())?;
/// # let repos = vec!(("local".to_string(), repo.into()));
/// let package_version = parse_version_ident("python/3.10.10")?;
///
/// let mut du_walker_builder = DiskUsageRepoWalkerBuilder::new(&repos);
/// let mut du_walker = du_walker_builder
///          .with_version_ident(package_version)
///          .with_count_links(false)
///          .with_deprecated(true)
///          .build();
/// # Ok(())
/// # }
/// ```
/// That disk usage walker will report on all files in the given file
/// path of packages that match the package path, even if deprecated,
/// and it will count links only once.
pub struct DiskUsageRepoWalkerBuilder<'a> {
    // Depth of given path to du based on this format:
    //   origin/package/version/build/component/file/path
    input_depth: usize,
    // To count linked files separately for each link or only once
    count_links: bool,
    // To include things in deprecated builds or not
    deprecated: bool,
    // Package disk usage entry filtering function
    package_du_filter_func: Arc<PackageDiskUsageFilterFunc<'a>>,
    repo_walker_builder: RepoWalkerBuilder<'a>,
}

impl<'a> DiskUsageRepoWalkerBuilder<'a> {
    pub fn new(repos: &'a Vec<(String, RepositoryHandle)>) -> Self {
        DiskUsageRepoWalkerBuilder {
            input_depth: 0,
            count_links: false,
            deprecated: false,
            // Only report on disk usage at, or below, the input depth.
            // This is from the cmd_du's "print all entries" use case.
            package_du_filter_func: Arc::new(|du: &PackageDiskUsage, depth| du.depth() >= depth),
            repo_walker_builder: RepoWalkerBuilder::new(repos),
        }
    }

    /// Given a DuSpec use it to filter what to get the disk usage on.
    /// This is helper method.
    pub fn with_du_spec(&mut self, du_spec: &DuSpec) -> Result<&mut Self> {
        if let Some(ref ident) = du_spec.ident {
            // Make a package filter from the package name in the ident
            let pkg_name = ident.name().to_string();
            self.with_package_filter(move |p| {
                RepoWalkerFilter::exact_package_name_filter(p, None, pkg_name.clone())
            });

            // Make a version filter from the version filter, if any,
            // and if it can be turned into a version.
            if !ident.version.is_empty() {
                match ident.version.clone().try_into_version() {
                    Ok(version_number) => {
                        self.with_version_filter(move |ver| {
                            RepoWalkerFilter::exact_match_version_filter(
                                ver,
                                version_number.to_string().clone(),
                            )
                        });
                    }
                    Err(err) => {
                        return Err(Error::String(format!(
                            "Unable to use '{}' because {err}",
                            ident.version
                        ))
                        .into());
                    }
                }
            }

            // Make a build filter from the build/digest part of the ident
            if let Some(build) = &ident.build {
                let build_id = build.to_string();
                self.repo_walker_builder.with_build_ident_filter(move |b| {
                    RepoWalkerFilter::exact_match_build_digest_filter(b, build_id.clone())
                });
            };

            // Make a file for the components, if any
            if !ident.components.is_empty() {
                let components = ident.components.clone();
                self.repo_walker_builder.with_component_filter(move |c| {
                    RepoWalkerFilter::allowed_components_filter(c, &components)
                });
            }
        }

        // The others part of a du spec are set by existing methods
        self.with_file_path(du_spec.file_path.clone());
        self.with_input_depth(du_spec.depth);

        Ok(self)
    }

    /// A helper method, given some below-a-component file path string
    /// use it to filter which directory and files within package
    /// components to get disk usage on.
    pub fn with_file_path(&mut self, file_path: Option<String>) -> &mut Self {
        self.repo_walker_builder.with_file_path(file_path);
        self
    }

    /// Set the depth of the initial du search path that will be
    /// passed to each call of the package disk usage filter function
    /// as those entries are walked. The default package du entry
    /// filter function relies on this being set.
    pub fn with_input_depth(&mut self, input_depth: usize) -> &mut Self {
        self.input_depth = input_depth;
        self
    }

    /// Set whether to count linked files separately each time (true),
    /// or just once (false), in disk usage calculations. Counting
    /// them as separate files will increase disk usage totals,
    /// counting them just once will count the first one and treat the
    /// rest as zero size. This is false by default to count link sizes
    /// are only once.
    pub fn with_count_links(&mut self, count_links: bool) -> &mut Self {
        self.count_links = count_links;
        self
    }

    /// Set whether to include deprecated builds in the disk usage
    /// walk. This is false by default exclude sizes from deprecated
    /// builds.
    pub fn with_deprecated(&mut self, deprecated: bool) -> &mut Self {
        self.deprecated = deprecated;
        self
    }

    /// Set a package disk usage filter function for disk usage
    /// results based on the PackageDiskUage object and initial input
    /// depth. The default filter is to include things for sizing at,
    /// or below, the configured input_depth.
    pub fn with_package_du_filter(
        &mut self,
        func: impl Fn(&PackageDiskUsage, usize) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.package_du_filter_func = Arc::new(func);
        self
    }

    /// Set up a filter based on a version ident
    pub fn with_version_ident(&mut self, version_ident: VersionIdent) -> &mut Self {
        self.repo_walker_builder.with_version_ident(version_ident);
        self
    }

    /// Set up a filter based on a build ident
    pub fn with_build_ident(&mut self, build_ident: BuildIdent) -> &mut Self {
        self.repo_walker_builder.with_build_ident(build_ident);
        self
    }

    /// Set up a filter function for packages based on their name.
    pub fn with_package_filter(
        &mut self,
        func: impl Fn(&WalkedPackage) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.repo_walker_builder.with_package_filter(func);
        self
    }

    /// Set up a filter function for versions based on their version
    /// number.
    pub fn with_version_filter(
        &mut self,
        func: impl Fn(&WalkedVersion) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.repo_walker_builder.with_version_filter(func);
        self
    }

    /// Set up a filter function for builds based on their build ident
    /// (digest). This is separate from [Self::with_build_spec_filter]
    /// because checking a build's ident is cheaper than reading in
    /// the build's spec to use in filtering.
    pub fn with_build_ident_filter(
        &mut self,
        func: impl Fn(&BuildIdent) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.repo_walker_builder.with_build_ident_filter(func);
        self
    }

    /// Set up a filter function for builds based on their build spec.
    /// This is separate from [Self::with_build_ident_filter] because
    /// reading a build's spec in is more expensive than just checking
    /// a build's ident. But it needed to access some data,
    /// e.g. deprecation status or install requirements.
    pub fn with_build_spec_filter(
        &mut self,
        func: impl Fn(&WalkedBuild) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.repo_walker_builder.with_build_spec_filter(func);
        self
    }

    /// Set up a filter function for components based on the component
    /// name.
    pub fn with_component_filter(
        &mut self,
        func: impl Fn(&WalkedComponent) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.repo_walker_builder.with_component_filter(func);
        self
    }

    /// Set up a filter function for files (dirs and files) based on
    /// the spfs entry and its parent path.
    pub fn with_file_filter(
        &mut self,
        func: impl Fn(&WalkedFile) -> bool + Send + Sync + 'a,
    ) -> &mut Self {
        self.repo_walker_builder.with_file_filter(func);
        self
    }

    /// Create a DiskUsageRepoWalker using the builder's current settings.
    pub fn build(&self) -> DiskUsageRepoWalker {
        DiskUsageRepoWalker {
            input_depth: self.input_depth,
            count_links: self.count_links,
            deprecated: self.deprecated,
            package_du_filter_func: self.package_du_filter_func.clone(),
            repo_walker_builder: self.repo_walker_builder.clone(),
        }
    }
}

/// A helper to return the disk usage in bytes of the given package
/// version, only counting the given builds (which must be a subset of
/// the version's builds or this will error). This is typically used
/// after a spk build that built a subset of the version's available
/// builds.
pub async fn get_version_builds_disk_usage(
    repos: &Vec<(String, RepositoryHandle)>,
    package_version: &VersionIdent,
    builds: &[BuildIdent],
) -> Result<u64> {
    // 1 for repo + 1 for package name + 1 for version number
    let input_depth = 3;

    let mut walker_builder = DiskUsageRepoWalkerBuilder::new(repos);
    // The build ident filter lets this count the size of these builds
    // within the package version without double counting shared files.
    let mut du_walker = walker_builder
        .with_version_ident(package_version.clone())
        .with_input_depth(input_depth)
        .with_build_ident_filter(move |b| builds.contains(b))
        .build();
    let mut walked = du_walker.grouped_du_walk();

    if let Some(grouped_entry) = walked.try_next().await? {
        // There should only be one entry because this was given a
        // version ident (pkg/ver) as a starting point and told to
        // group by that version ident.
        Ok(grouped_entry.size)
    } else {
        Err(Error::DiskUsageVersionNotFound(package_version.clone()).into())
    }
}

/// A helper to return the disk usage in bytes of the given build (all
/// components) in the given repo(s).
pub async fn get_build_disk_usage(
    repos: &Vec<(String, RepositoryHandle)>,
    build_ident: &BuildIdent,
) -> Result<u64> {
    // Starting depth is: 1 for repo, + 1 for package name, + 1 for
    // version number, + 1 for build digest.
    let input_depth = 4;

    let mut walker_builder = DiskUsageRepoWalkerBuilder::new(repos);
    let mut du_walker = walker_builder
        .with_build_ident(build_ident.clone())
        .with_input_depth(input_depth)
        .build();
    let mut walked = du_walker.grouped_du_walk();

    if let Some(grouped_entry) = walked.try_next().await? {
        // There should be only one entry, because this was given a
        // build ident (pkg/ver/build) as a starting point and told to
        // group by that build ident.
        Ok(grouped_entry.size)
    } else {
        Err(Error::DiskUsageBuildNotFound(build_ident.clone()).into())
    }
}

/// A helper to return the grouped disk usage of a given set of
/// components in a specific build in a repo.
///
/// For example, to get the size of the 'run' component of
/// pkg/ver/build, or the size of the 'run' and 'docs' components of
/// pkg/ver/build, as part of a set of resolved package builds (a
/// solution).
pub async fn get_components_disk_usage(
    repo: Arc<RepositoryHandle>,
    build_ident: Arc<BuildIdent>,
    components: &HashMap<Component, Digest>,
) -> Result<GroupedDiskUsage> {
    let mut disk_usage = GroupedDiskUsage {
        grouping: build_ident.to_string(),
        size: 0,
        // This doesn't care about deprecation states
        deprecated: false,
    };

    let mut visited_digests = HashSet::new();
    let count_links = false;

    // This sets up a walker but it isn't going to walk the normal way
    // down the repo and packages and so on. It is going to start at
    // the file stream for the given build.
    let no_repos = Vec::new();
    let mut repo_walker_builder = RepoWalkerBuilder::new(&no_repos);
    let repo_walker = repo_walker_builder
        // Need to set this to have the file_stream emit files
        .with_report_on_files(true)
        .with_continue_on_error(true)
        .build();

    // Add the size of each components to the grouped disk usage size
    // to get a total.
    for (component, digest) in components {
        // This walks the file level directly without going through
        // the whole repo hierarchy provided by a RepoWalker's walk.
        let walked_component = WalkedComponent {
            repo_name: repo.name(),
            build: Arc::clone(&build_ident),
            name: component.clone(),
            digest: Arc::new(*digest),
        };
        let mut traversal = repo_walker.file_stream(&repo, walked_component);

        while let Some(file) = traversal.try_next().await? {
            let entry_size = if visited_digests.insert(file.entry.object) || count_links {
                file.entry.size()
            } else {
                // Set to 0 because we don't need to calculate sizes
                // if it is a duplicate or count_links is not enabled.
                0
            };
            disk_usage.size += entry_size;
        }
    }

    // Return the build's grouped result for the given components
    Ok(disk_usage)
}

/// A helper to return the given size in bytes as human readable
/// string with the appropriate units.
pub fn human_readable(size: u64) -> String {
    spfs::io::format_size(size)
}

/// The search paths and depth settings for a du walker
#[derive(Debug)]
pub struct DuSpec {
    pub repo_name: Option<String>,
    pub ident: Option<RangeIdent>,
    pub file_path: Option<String>,
    pub depth: usize,
}

/// A helper to examine a "repo path" (probably from a command line)
/// and work out what it represents: a repo or package identifier, in
/// either package request, or du components path format, with, or
/// without, a sub-components level file path.
///
/// Returns a DuSpec struct if the path was valid for use with a
/// DiskUsageRepoWalkerBuilder
pub fn extract_du_spec_from_path(path: &str) -> Result<DuSpec> {
    // A trailing / indicates an extra level of depth, which can be
    // important for grouped up disk usage.
    let depth_adjustment = if path.ends_with(LEVEL_SEPARATOR) {
        1
    } else {
        0
    };
    // Remove trailing /'s to simplify the path for parsing.
    let trimmed_path = path.trim_end_matches(LEVEL_SEPARATOR);

    let du_parts = if KNOWN_REPOSITORY_NAMES.contains(trimmed_path) {
        // It was a known repo name only. That could also be a package
        // name, but that is unlikely and ambiguous so treat it as
        // just a repo name.
        DuSpec {
            repo_name: Some(trimmed_path.to_string()),
            ident: None,
            file_path: None,
            depth: 1 + depth_adjustment,
        }
    } else {
        match parse_ident_range(trimmed_path) {
            Ok(mut ident) => {
                // It was in the package request format:
                //   repo/package:components/version/build
                // but didn't contain any trailing /file/path
                let repo_name = ident.repository_name.clone().map(|rn| rn.to_string());
                // The repo name is removed from the ident because it
                // is stored separately in the DuSpec
                ident.repository_name = None;

                // Work out the depth from the given path but account
                // for option components and repo name, if any.
                let mut depth =
                    trimmed_path.split(LEVEL_SEPARATOR).collect_vec().len() + depth_adjustment;
                if repo_name.is_none() {
                    // Repo level counts as one even if it was not
                    // specified in the package request format.
                    depth += 1;
                }
                if !ident.components.is_empty() {
                    // Components count as an extra level for package
                    // request format, but not the path format.
                    depth += 1;
                }

                DuSpec {
                    repo_name,
                    ident: Some(ident),
                    file_path: None,
                    depth,
                }
            }
            Err(e) => {
                // It might be in path format:
                //   repo/package/version/build/:components/file/path
                // if it has a components level after the build level
                // and not next to the package name.
                if trimmed_path.contains(COMPONENTS_MARKER) {
                    // This has a components section in one of the path sub-directories.
                    // It is in the du path form, e.g.:
                    //    repo/pkg/version/build/:components/...
                    let parts: Vec<_> = trimmed_path.split(&LEVEL_SEPARATOR.to_string()).collect();

                    let index = match parts
                        .iter()
                        .position(|&p| p.starts_with(COMPONENTS_SEPARATOR))
                    {
                        Some(i) => i,
                        None => {
                            // We checked for /:, the COMPONENTS_MARKER,
                            // in the path before splitting it up so this
                            // should not happen.
                            return Err(Error::String(format!(
                                "No index position for start of components in du path format string that contains a components marker. This should not happen to: '{parts:?}'."
                            ))
                                       .into());
                        }
                    };

                    if index <= BUILD_INDEX {
                        // The :components piece occurs too early in
                        // the du path format, so the du path has not
                        // been specified correctly. Some sections
                        // have been left out.
                        let n = BUILD_INDEX - index + 1;
                        return Err(Error::String(format!("Disk usage path '{path}' is missing {n} sections before the :components section.\nPlease check you have specified the repo, package, version, and build id sections.\nDid you mean to specify it as package request instead?\n  (package:components/version/build/file/path...)")).into());
                    }

                    // The path will include at least pkg/ver/build/:run
                    // but it might not start with a repo name, or have
                    // a file path after the components section. Rearrange
                    // it into a package ident/request form to make it
                    // possible to check via parsers.
                    let (repo_name, possible_ident) =
                        if KNOWN_REPOSITORY_NAMES.contains(&parts[REPO_INDEX]) {
                            // This starts with a known repo.
                            (
                                Some(parts[REPO_INDEX].to_string()),
                                [
                                    &format!("{}{}", parts[NAME_INDEX], parts[index]),
                                    parts[VERSION_INDEX],
                                    parts[BUILD_INDEX],
                                ]
                                .join(&LEVEL_SEPARATOR.to_string()),
                            )
                        } else {
                            // This doesn't start with a known repo,
                            // so the indices are shifted.
                            (
                                None,
                                [
                                    &format!("{}{}", parts[REPO_INDEX], parts[index]),
                                    parts[VERSION_INDEX],
                                    parts[BUILD_INDEX],
                                ]
                                .join(&LEVEL_SEPARATOR.to_string()),
                            )
                        };

                    // Parse the rest as a package ident
                    let ident = match parse_ident_range(possible_ident) {
                        Ok(i) => i,
                        Err(err) => return Err(Error::String(format!("{err}\nUnable to parse '{path}' as a disk usage formatted path:\n  (repo/package/version/build/:components/file/path...)\nDid you mean to specify it as package request instead?\n  (package:components/version/build/file/path...)")).into())
                    };

                    // All the pieces beyond the /: components section
                    // treated as a file path inside the package build
                    // (i.e. things under the components).
                    let file_path = if index + 1 < parts.len() {
                        Some(parts[index + 1..].join(&LEVEL_SEPARATOR.to_string()))
                    } else {
                        None
                    };

                    // Work out the depth from levels in the given path
                    let depth =
                        trimmed_path.split(LEVEL_SEPARATOR).collect_vec().len() + depth_adjustment;

                    DuSpec {
                        repo_name,
                        ident: Some(ident),
                        file_path,
                        depth,
                    }
                } else {
                    // This doesn't parse as a request, so it's
                    // probably in path form, but it doesn't have a
                    // /:components section or that would have been
                    // detected earlier, so this is an error.
                    return Err(e.into());
                }
            }
        }
    };

    Ok(du_parts)
}

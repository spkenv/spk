use crate::{api, Result};

pub trait Repository {
    /// Return the set of known packages in this repo.
    fn list_packages(&self) -> Result<Vec<String>>;

    /// Return the set of versions available for the named package.
    fn list_package_versions(&self, name: &str) -> Result<Vec<api::Version>>;

    /// Return the set of builds for the given package name and version.
    fn list_package_builds(&self, pkg: &api::Ident) -> Result<Vec<api::Ident>>;

    /// Read a package spec file for the given package, version and optional build.
    ///
    /// # Errors:
    /// - PackageNotFoundError: If the package, version, or build does not exist
    fn read_spec(&self, pkg: &api::Ident) -> Result<api::Spec>;

    ///Identify the payload for the identified binary package and build options.
    ///
    /// he given build options should be resolved using the package spec
    /// before calling this function, unless the exact complete set of options
    /// can be known deterministically.
    fn get_package(&self, pkg: &api::Ident) -> Result<spfs::encoding::Digest>;

    /// Publish a package spec to this repository.
    ///
    /// The published spec represents all builds of a single version.
    /// The source package, or at least one binary package should be
    /// published as well in order to make the spec usable in environments.
    ///
    /// # Errors:
    /// - VersionExistsError: if the spec a this version is already present
    fn publish_spec(&mut self, spec: api::Spec) -> Result<()>;

    /// Remove a package version from this repository.
    ///
    /// This will not untag builds for this package, but make it unresolvable
    /// and unsearchable. It's recommended that you remove all existing builds
    /// before removing the spec in order to keep the repository clean.
    fn remove_spec(&mut self, pkg: &api::Ident) -> Result<()>;

    /// Publish a package spec to this repository.
    ///
    /// Same as 'publish_spec' except that it clobbers any existing
    /// spec at this version
    fn force_publish_spec(&mut self, spec: api::Spec) -> Result<()>;

    /// Publish a binary package to this repository.
    ///
    /// The published digest is expected to identify an spfs layer which contains
    /// the propery constructed binary package files and metadata.
    fn publish_package(&mut self, spec: api::Spec, digest: spfs::encoding::Digest) -> Result<()>;

    /// Remove a package from this repository.
    ///
    /// The given package identifier must identify a full package build
    fn remove_package(&mut self, pkg: &api::Ident) -> Result<()>;
}

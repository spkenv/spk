// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use crate::Result;

#[cfg(test)]
#[path = "./sources_test.rs"]
mod sources_test;

/// Denotes an error during the build process.
#[derive(Debug)]
pub struct CollectionError {
    pub message: String,
}

impl CollectionError {
    pub fn new_error(format_args: std::fmt::Arguments) -> crate::Error {
        crate::Error::Collection(Self {
            message: std::fmt::format(format_args),
        })
    }
}


class SourcePackageBuilder:
    """Builds a source package.

    >>> (
    ...     SourcePackageBuilder
    ...     .from_spec(api.Spec.from_dict({
    ...         "pkg": "my-pkg",
    ...      }))
    ...     .build()
    ... )
    my-pkg/0.0.0/src
    """

    def __init__(self) -> None:

        self._spec: Optional[api.Spec] = None
        self._repo: Optional[storage.Repository] = None

    @staticmethod
    def from_spec(spec: api.Spec) -> "SourcePackageBuilder":

        builder = SourcePackageBuilder()
        builder._spec = spec
        return builder

    def with_target_repository(
        self, repo: storage.Repository
    ) -> "SourcePackageBuilder":
        """Set the repository that the created package should be published to."""

        self._repo = repo
        return self

    def build(self) -> api.Ident:
        """Build the requested source package."""

        assert (
            self._spec is not None
        ), "Target spec not given, did you use SourcePackagebuilder.from_spec?"

        if self._repo is not None:
            repo = self._repo
        else:
            repo = storage.local_repository()

        layer = collect_and_commit_sources(self._spec)
        spec = self._spec.copy()
        spec.pkg = spec.pkg.with_build(api.SRC)
        repo.publish_package(spec, layer)
        return spec.pkg


def collect_and_commit_sources(spec: api.Spec) -> spkrs.Digest:
    """Collect sources for the given spec and commit them into an spfs layer."""

    pkg = spec.pkg.with_build(api.SRC)
    spkrs.reconfigure_runtime(editable=True, reset=["*"], stack=[])

    source_dir = data_path(pkg)
    collect_sources(spec, source_dir)

    _LOGGER.info("Validating package source files...")
    try:
        spkrs.build.validate_source_changeset()
    except RuntimeError as e:
        raise CollectionError(str(e))

    return spkrs.commit_layer(spkrs.active_runtime())


def collect_sources(spec: api.Spec, source_dir: str) -> None:
    """Collect the sources for a spec in the given directory."""
    os.makedirs(source_dir)

    original_env = os.environ.copy()
    os.environ.update(get_package_build_env(spec))
    try:
        for source in spec.sources:
            target_dir = source_dir
            subdir = source.subdir
            if subdir:
                target_dir = os.path.join(source_dir, subdir.lstrip("/"))
            os.makedirs(target_dir, exist_ok=True)
            api.collect_source(source, target_dir)
    finally:
        os.environ.clear()
        os.environ.update(original_env)


/// Validate the set of diffs for a source package build.
///
/// # Errors:
///   - CollectionError: if any issues are identified in the changeset
pub fn validate_source_changeset<P: AsRef<relative_path::RelativePath>>(
    diffs: Vec<spfs::tracking::Diff>,
    source_dir: P,
) -> Result<()> {
    if diffs.is_empty() {
        return Err(CollectionError::new_error(format_args!(
            "No source files collected, source package would be empty"
        )));
    }

    let mut source_dir = source_dir.as_ref();
    source_dir = source_dir.strip_prefix("/spfs").unwrap_or(source_dir);
    for diff in diffs.into_iter() {
        if diff.mode == spfs::tracking::DiffMode::Unchanged {
            continue;
        }
        if diff.path.starts_with(&source_dir) {
            // the change is within the source directory
            continue;
        }
        if source_dir.starts_with(&diff.path) {
            // the path is to a parent directory of the source path
            continue;
        }
        return Err(CollectionError::new_error(format_args!(
            "Invalid source file path found: {} (not under {})",
            &diff.path, source_dir
        )));
    }
    Ok(())
}

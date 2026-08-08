// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use spfs::encoding::EMPTY_DIGEST;
use spfstest::spfstest;
use spk_schema::BuildIdent;
use spk_schema::foundation::ident_component::Component;

use super::RuntimeRepository;
use crate::fixtures::spfs_runtime;

/// Regression test: `read_components_bulk` must not fail with
/// `PackageNotFound` for a package whose component markers exist in the
/// runtime's active changes but not in any committed layer.
///
/// This is the state of the package currently being built: `spk` writes its
/// spec and component marker files into /spfs before running the build
/// script, but the package's contents have not been committed to a layer yet.
/// Previously this caused `current_env` (and therefore `spk info` run from
/// inside a build script with no arguments) to fail with a "Package not found"
/// error about the package being built.
#[spfstest]
#[tokio::test]
async fn read_components_bulk_tolerates_package_being_built() {
    // Provides a clean, editable, isolated runtime with an empty layer stack.
    let _rt = spfs_runtime().await;

    let ident: BuildIdent = "being-built/1.0.0/3I42H3S6".parse().unwrap();

    // Lay out the package's component marker the way a build does, writing
    // directly into the live /spfs (the runtime's active changes) without
    // committing a layer for it.
    let pkg_dir = std::path::Path::new("/spfs/spk/pkg").join(ident.to_string());
    spfs::runtime::makedirs_with_perms(&pkg_dir, 0o777)
        .expect("should be able to create the package metadata dir under /spfs");
    std::fs::File::create(pkg_dir.join(format!("{}.cmpt", Component::Run)))
        .expect("should be able to create the run component marker");

    let repo = RuntimeRepository::default();
    let results = repo
        .read_components_bulk(&[&ident])
        .await
        .expect("reading components for a package being built must not error");

    let empty: spfs::encoding::Digest = EMPTY_DIGEST.into();
    assert_eq!(
        results[0].get(&Component::Run),
        Some(&empty),
        "the run component of a package being built (not yet committed to a \
         layer) should resolve to the empty digest rather than erroring"
    );
}

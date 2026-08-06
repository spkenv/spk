// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt;
use rstest::{fixture, rstest};
use spfs::prelude::*;
use spfstest::spfstest;
use spk_cmd_build::build_package;
use spk_schema::foundation::build_ident;
use spk_schema::foundation::fixtures::*;
use spk_schema::foundation::ident_component::Component;
use spk_schema::spec;
use spk_solve::{DecisionFormatterBuilder, RepositoryHandle, SolverExt, SolverMut, StepSolver};
use spk_solve_macros::pinned_request;
use spk_storage::IndexedRepository;
use spk_storage::fixtures::*;
use tokio::pin;

use super::{ResolvedLayer, ResolvedLayers};
use crate::{Error, solution_to_resolved_runtime_layers};

/// Build a single-layer [`ResolvedLayers`] pointing at the given repository.
fn resolved_layers_for(repo: Arc<RepositoryHandle>, digest: spfs::Digest) -> ResolvedLayers {
    ResolvedLayers(vec![ResolvedLayer {
        digest,
        spec: Arc::new(spec!({"pkg": "my-pkg/1.0.0/3I42H3S6"})),
        component: Component::Run,
        repo,
    }])
}

#[fixture]
fn solver() -> StepSolver {
    StepSolver::default()
}

#[spfstest]
/// If two layers contribute files to the same subdirectory, the Manifest is
/// expected to contain both files.
#[rstest]
#[case::cli("cli")]
#[case::checks("checks")]
#[case::resolvo("resolvo")]
#[tokio::test]
async fn get_environment_filesystem_merges_directories(
    tmpdir: tempfile::TempDir,
    // TODO: test with all solvers
    mut solver: StepSolver,
    #[case] solver_to_run: &str,
) {
    let rt = spfs_runtime().await;

    build_package!(
        tmpdir,
        "one.spk.yaml",
        br#"
api: v0/package
pkg: one/1.0.0

build:
  script:
    - mkdir "$PREFIX"/subdir
    - touch "$PREFIX"/subdir/one.txt
"#,
        solver_to_run
    );

    build_package!(
        tmpdir,
        "two.spk.yaml",
        br#"
api: v0/package
pkg: two/1.0.0

build:
  script:
    - mkdir "$PREFIX"/subdir
    - touch "$PREFIX"/subdir/two.txt
"#,
        solver_to_run
    );

    let formatter = DecisionFormatterBuilder::default()
        .with_verbosity(0)
        .build();

    solver.add_repository(Arc::clone(&rt.tmprepo));
    solver.add_request(pinned_request!("one"));
    solver.add_request(pinned_request!("two"));

    let solution = solver.run_and_log_resolve(&formatter).await.unwrap();

    let resolved_layers = solution_to_resolved_runtime_layers(&solution).unwrap();

    let mut conflicting_packages = HashMap::new();
    let environment = resolved_layers
        .get_environment_filesystem(
            build_ident!("does-not-matter/1.0.0/src"),
            &mut conflicting_packages,
        )
        .await
        .unwrap();

    assert!(environment.get_path("subdir/one.txt").is_some());
    assert!(environment.get_path("subdir/two.txt").is_some());
}

#[rstest]
#[tokio::test]
async fn test_iter_entries_rejects_layers_without_spfs_storage() {
    let repo = make_repo(RepoKind::Mem).await;
    let layers = resolved_layers_for(Arc::clone(&repo.repo), empty_layer_digest());

    let entries = layers.iter_entries();
    pin!(entries);

    assert!(
        matches!(
            entries.next().await,
            Some(Err(Error::NonSpfsLayerInResolvedLayers))
        ),
        "a layer from a repository with no spfs storage cannot be read"
    );
}

#[rstest]
#[tokio::test]
async fn test_iter_entries_reads_through_an_indexed_repo() {
    // An indexed repository wraps the spfs repository it indexes. Reading the
    // layer must follow through to that storage instead of treating the layer
    // as if it were not backed by spfs at all.
    let repo = make_repo(RepoKind::Spfs).await;
    let RepositoryHandle::SPFS(spfs_repo) = &*repo.repo else {
        panic!("the spfs test fixture should produce an spfs repository");
    };

    let mut contents = spfs::tracking::Manifest::<()>::default();
    contents.mkdirs("subdir").unwrap();
    contents.mkfile("subdir/file.txt").unwrap();
    let manifest = contents.to_graph_manifest();
    let layer = spfs::graph::Layer::new(manifest.digest().unwrap());
    spfs_repo.write_object(&manifest).await.unwrap();
    spfs_repo.write_object(&layer).await.unwrap();

    let indexed = RepositoryHandle::Indexed(
        IndexedRepository::generate_from_repo(Arc::clone(&repo.repo))
            .await
            .unwrap(),
    );

    let layers = resolved_layers_for(Arc::new(indexed), layer.digest().unwrap());

    let entries = layers.iter_entries();
    pin!(entries);

    let mut paths = Vec::new();
    while let Some(entry) = entries.next().await {
        let (path, _entry, _layer) =
            entry.expect("entries of an indexed repository should be readable");
        paths.push(path.to_string());
    }

    assert!(
        paths.iter().any(|path| path.ends_with("subdir/file.txt")),
        "the layer's files should be reachable through the index, got: {paths:?}"
    );
}

#[spfstest]
#[rstest]
#[tokio::test]
async fn test_pull_resolved_runtime_layers_warns_for_layers_without_spfs_storage() {
    let _rt = spfs_runtime().await;

    // A digest that is not in the isolated local repository, so pulling it is
    // actually attempted.
    let missing_digest = spfs::Digest::from_bytes(&[0xab; spfs::encoding::DIGEST_SIZE]).unwrap();

    let repo = make_repo(RepoKind::Mem).await;
    let layers = resolved_layers_for(Arc::clone(&repo.repo), missing_digest);

    // The layer cannot be localized, but that is reported as a warning rather
    // than failing the whole operation.
    let stack = crate::pull_resolved_runtime_layers(&layers)
        .await
        .expect("layers that cannot be localized should not fail the pull");

    assert_eq!(stack, vec![missing_digest]);

    let local_repo = spk_storage::local_repository().await.unwrap();
    assert!(
        !local_repo.has_object(missing_digest).await,
        "nothing should have been synced from a repository with no spfs storage"
    );
}

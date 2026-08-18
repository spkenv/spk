// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;

use clap::Parser;
use rstest::rstest;
use spfs::RemoteAddress;
use spfs::config::Remote;
use spfs::graph::object::Enum;
use spfs::prelude::*;
use spfstest::spfstest;
use spk_build::{BinaryPackageBuilder, BuildSource};
use spk_cli_common::Run;
use spk_schema::foundation::option_map;
use spk_schema::{Package, recipe};
use spk_solve::SolverImpl;
use spk_storage::fixtures::*;
use spk_storage::{FlatBufferRepoIndex, RepositoryIndexMut};

use super::Render;

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    render: Render,
}

fn step_solver() -> SolverImpl {
    SolverImpl::Step(spk_solve::StepSolver::default())
}

async fn make_origin_package_with_missing_local_manifests(
    rt: &mut RuntimeLock,
) -> (TempRepo, String, HashSet<spfs::encoding::Digest>) {
    let origin_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: origin_repo.address().clone(),
        }),
    )
    .unwrap();

    let recipe = recipe!(
        { "pkg": "render-fallback-test/1.0.0",
          "build": {
              "auto_host_vars": "None",
              "script": "mkdir -p /spfs/render-fallback && echo hello > /spfs/render-fallback/hello.txt"
          }
        }
    );

    origin_repo.publish_recipe(&recipe).await.unwrap();
    let (built_spec, components) =
        BinaryPackageBuilder::from_recipe_with_solver(recipe, step_solver())
            .with_source(BuildSource::LocalPath(".".into()))
            .with_repository(rt.tmprepo.clone())
            .build_and_publish(&option_map! {}, &*origin_repo)
            .await
            .unwrap();
    let origin_spfs = spfs::get_config()
        .unwrap()
        .try_get_remote("origin")
        .await
        .unwrap()
        .expect("origin should be configured");

    let local_repo = spfs::get_config()
        .unwrap()
        .get_local_repository_handle()
        .await
        .unwrap();

    for layer_digest in components.values() {
        spfs::Syncer::new(&local_repo, &origin_spfs)
            .sync_digest(*layer_digest)
            .await
            .unwrap();
    }

    let mut removed_manifest_digests = HashSet::new();
    for layer_digest in components.values() {
        let layer = origin_spfs.read_object(*layer_digest).await.unwrap();
        let Enum::Layer(layer) = layer.into_enum() else {
            panic!("component layer digest should point to a layer object");
        };

        let manifest_digest = *layer
            .manifest()
            .expect("built component layer should reference a manifest");
        removed_manifest_digests.insert(manifest_digest);

        if local_repo.has_object(manifest_digest).await {
            local_repo.remove_object(manifest_digest).await.unwrap();
        }
        assert!(
            !local_repo.has_object(manifest_digest).await,
            "local repo should be missing the manifest so render must fall back"
        );
        assert!(
            origin_spfs.has_object(manifest_digest).await,
            "origin repo should still contain the manifest"
        );
    }

    (
        origin_repo,
        format!("{}", built_spec.ident().clone().to_version_ident()),
        removed_manifest_digests,
    )
}

#[spfstest]
#[rstest]
#[case::plain_origin(false)]
#[case::indexed_origin(true)]
#[tokio::test]
async fn test_render_repairs_missing_local_objects_via_origin_fallback(
    #[case] use_indexed_origin: bool,
) {
    let mut rt = spfs_runtime().await;
    let (_origin_repo, request, removed_manifest_digests) =
        make_origin_package_with_missing_local_manifests(&mut rt).await;

    if use_indexed_origin {
        let no_metric_name: Option<String> = None;
        let origin_repo = spk_storage::remote_repository("origin").await.unwrap();
        FlatBufferRepoIndex::index_repo(
            &vec![("origin".to_string(), origin_repo.into())],
            &no_metric_name,
        )
        .await
        .unwrap();
    }

    let target_dir = rt.tmpdir.path().join(if use_indexed_origin {
        "indexed"
    } else {
        "plain"
    });
    let mut argv = vec![
        "render".to_string(),
        "--no-host".to_string(),
        "--enable-repo".to_string(),
        "origin".to_string(),
    ];
    if use_indexed_origin {
        argv.push("--index-use".to_string());
        argv.push("enabled".to_string());
    }
    argv.push(request);
    argv.push(target_dir.display().to_string());

    let mut opt = Opt::try_parse_from(argv).unwrap();
    if use_indexed_origin {
        let repos = opt
            .render
            .solver
            .repos
            .get_repos_for_non_destructive_operation()
            .await
            .unwrap();
        assert!(
            repos
                .iter()
                .any(|(_, repo)| matches!(repo, spk_storage::RepositoryHandle::Indexed(_))),
            "render should be given an indexed origin repository when index use is enabled"
        );
    }

    opt.render
        .run()
        .await
        .expect("render should succeed by falling back to origin");

    assert!(
        std::fs::read_dir(&target_dir).unwrap().next().is_some(),
        "render target should contain files"
    );

    let local_repo = spfs::get_config()
        .unwrap()
        .get_opened_local_repository()
        .await
        .unwrap();
    let mut repaired_count = 0;
    for digest in removed_manifest_digests {
        if local_repo.has_object(digest).await {
            repaired_count += 1;
        }
    }
    assert!(
        repaired_count > 0,
        "at least one missing manifest should have been repaired from origin"
    );
}

#[spfstest]
#[rstest]
#[tokio::test]
async fn test_render_local_package_with_origin_dependency_index_enabled() {
    let mut rt = spfs_runtime().await;
    let origin_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: origin_repo.address().clone(),
        }),
    )
    .unwrap();

    let dep_recipe = recipe!(
        { "pkg": "render-dep/1.0.0",
          "build": {
              "auto_host_vars": "None",
              "script": "mkdir -p /spfs/render-dep && echo dep > /spfs/render-dep/dep.txt"
          }
        }
    );
    origin_repo.publish_recipe(&dep_recipe).await.unwrap();
    let (_dep_spec, dep_components) =
        BinaryPackageBuilder::from_recipe_with_solver(dep_recipe, step_solver())
            .with_source(BuildSource::LocalPath(".".into()))
            .with_repository(rt.tmprepo.clone())
            .build_and_publish(&option_map! {}, &*origin_repo)
            .await
            .unwrap();
    let origin_spfs = spfs::get_config()
        .unwrap()
        .try_get_remote("origin")
        .await
        .unwrap()
        .expect("origin should be configured");
    let local_repo = spfs::get_config()
        .unwrap()
        .get_local_repository_handle()
        .await
        .unwrap();
    for layer_digest in dep_components.values() {
        spfs::Syncer::new(&local_repo, &origin_spfs)
            .sync_digest(*layer_digest)
            .await
            .unwrap();
    }
    for layer_digest in dep_components.values() {
        let layer = origin_spfs.read_object(*layer_digest).await.unwrap();
        let Enum::Layer(layer) = layer.into_enum() else {
            continue;
        };
        if local_repo.has_object(*layer_digest).await {
            local_repo.remove_object(*layer_digest).await.unwrap();
        }
        if let Some(manifest_digest) = layer.manifest().copied()
            && local_repo.has_object(manifest_digest).await
        {
            local_repo.remove_object(manifest_digest).await.unwrap();
        }
    }
    for layer_digest in dep_components.values() {
        assert!(
            !local_repo.has_object(*layer_digest).await,
            "dependency layer should be absent locally before render"
        );
        assert!(
            origin_spfs.has_object(*layer_digest).await,
            "dependency layer should remain available in origin"
        );
    }

    let app_recipe = recipe!(
        { "pkg": "render-app/1.0.0",
          "install": {"requirements": [{"pkg": "render-dep/1.0.0"}]},
          "build": {
              "auto_host_vars": "None",
              "script": "mkdir -p /spfs/render-app && echo app > /spfs/render-app/app.txt"
          }
        }
    );
    rt.tmprepo.publish_recipe(&app_recipe).await.unwrap();
    let (app_spec, _) = BinaryPackageBuilder::from_recipe_with_solver(app_recipe, step_solver())
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    assert!(
        !dep_components.is_empty(),
        "dependency package should produce at least one component"
    );

    let no_metric_name: Option<String> = None;
    let origin_repo_for_index = spk_storage::remote_repository("origin").await.unwrap();
    FlatBufferRepoIndex::index_repo(
        &vec![("origin".to_string(), origin_repo_for_index.into())],
        &no_metric_name,
    )
    .await
    .unwrap();

    let target_dir = rt.tmpdir.path().join("local-app-with-origin-dep-indexed");
    let mut opt = Opt::try_parse_from([
        "render",
        "--no-host",
        "--enable-repo",
        "origin",
        "--index-use",
        "enabled",
        &format!("{}", app_spec.ident().clone().to_version_ident()),
        target_dir.to_str().unwrap(),
    ])
    .unwrap();

    let repos = opt
        .render
        .solver
        .repos
        .get_repos_for_non_destructive_operation()
        .await
        .unwrap();
    assert!(
        repos
            .iter()
            .any(|(_, repo)| matches!(repo, spk_storage::RepositoryHandle::Indexed(_))),
        "render should resolve the origin dependency through an indexed repository"
    );

    opt.render
        .run()
        .await
        .expect("render should localize the origin dependency through its indexed repository");
    assert!(target_dir.join("render-app/app.txt").is_file());
    assert!(target_dir.join("render-dep/dep.txt").is_file());
}

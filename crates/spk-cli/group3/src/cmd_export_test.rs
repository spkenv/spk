// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Parser;
use rstest::rstest;
use spfs::prelude::*;
use spfstest::spfstest;
use spk_build::{BinaryPackageBuilder, BuildSource};
use spk_schema::foundation::option_map;
use spk_schema::{Package, recipe};
use spk_solve::SolverImpl;
use spk_storage::fixtures::*;
use spk_storage::{FlatBufferRepoIndex, RepositoryIndexMut};

use super::{Export, Run};

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    export: Export,
}

fn step_solver() -> SolverImpl {
    SolverImpl::Step(spk_solve::StepSolver::default())
}

fn resolvo_solver() -> SolverImpl {
    SolverImpl::Resolvo(spk_solve::ResolvoSolver::default())
}

#[spfstest]
#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_export_works_with_missing_builds(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;

    let spec = recipe!(
        {
            "pkg": "spk-export-test/0.0.1",
            "build": {
                "options": [
                    {"var": "color"},
                ],
                "script": "touch /spfs/file.txt",
            },
        }
    );
    rt.tmprepo.publish_recipe(&spec).await.unwrap();
    let (blue_spec, _) =
        BinaryPackageBuilder::from_recipe_with_solver(spec.clone(), solver.clone())
            .with_source(BuildSource::LocalPath(".".into()))
            .build_and_publish(option_map! {"color" => "blue"}, &*rt.tmprepo)
            .await
            .unwrap();
    let (red_spec, _) = BinaryPackageBuilder::from_recipe_with_solver(spec, solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {"color" => "red"}, &*rt.tmprepo)
        .await
        .unwrap();

    // Now that these two builds are created, remove the `spk/pkg` tags for one
    // of them. The publish is still expected to succeed; it should publish
    // the remaining valid build.
    let repo = match &*rt.tmprepo {
        spk_storage::RepositoryHandle::SPFS(spfs) => {
            for spec in [
                format!("{}", blue_spec.ident().build()),
                format!("{}/build", blue_spec.ident().build()),
                format!("{}/run", blue_spec.ident().build()),
            ] {
                let tag = spfs::tracking::TagSpec::parse(format!(
                    "spk/pkg/spk-export-test/0.0.1/{spec}",
                ))
                .unwrap();
                spfs.remove_tag_stream(&tag).await.unwrap();
            }
            spfs
        }
        _ => panic!("only implemented for spfs repos"),
    };

    let filename = rt.tmpdir.path().join("archive.spk");
    filename.ensure();
    spk_storage::export_package(
        &[repo],
        red_spec
            .ident()
            .clone()
            .to_version_ident()
            .to_any_ident(None),
        &filename,
    )
    .await
    .expect("failed to export");
    let mut actual = Vec::new();
    let mut tarfile = tar::Archive::new(std::fs::File::open(&filename).unwrap());
    for entry in tarfile.entries().unwrap() {
        let filename = entry.unwrap().path().unwrap().to_string_lossy().to_string();
        if filename.contains('/') && !filename.contains("tags") {
            // ignore specific object data for this test
            continue;
        }
        actual.push(filename);
    }
    actual.sort();
    assert_eq!(
        actual,
        vec![
            "VERSION".to_string(),
            "objects".to_string(),
            "payloads".to_string(),
            "renders".to_string(),
            "tags".to_string(),
            "tags/spk".to_string(),
            "tags/spk/pkg".to_string(),
            "tags/spk/pkg/spk-export-test".to_string(),
            "tags/spk/pkg/spk-export-test/0.0.1".to_string(),
            format!(
                "tags/spk/pkg/spk-export-test/0.0.1/{}",
                red_spec.ident().build()
            ),
            format!(
                "tags/spk/pkg/spk-export-test/0.0.1/{}.tag",
                red_spec.ident().build()
            ),
            format!(
                "tags/spk/pkg/spk-export-test/0.0.1/{}/build.tag",
                red_spec.ident().build()
            ),
            format!(
                "tags/spk/pkg/spk-export-test/0.0.1/{}/run.tag",
                red_spec.ident().build()
            ),
            "tags/spk/spec".to_string(),
            "tags/spk/spec/spk-export-test".to_string(),
            "tags/spk/spec/spk-export-test/0.0.1".to_string(),
            "tags/spk/spec/spk-export-test/0.0.1.tag".to_string(),
            format!(
                "tags/spk/spec/spk-export-test/0.0.1/{}.tag",
                red_spec.ident().build()
            ),
        ]
    );
}

#[spfstest]
#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_export_works_with_an_indexed_repo(#[case] solver: SolverImpl) {
    // When index use is enabled, the repositories handed to the command are
    // indexed repositories that wrap the underlying spfs storage. Export must
    // still find that storage rather than rejecting the repository outright.
    let rt = spfs_runtime().await;

    let spec = recipe!(
        {
            "pkg": "spk-export-index-test/0.0.1",
            "build": {
                "auto_host_vars": "None",
                "script": "touch /spfs/file.txt",
            },
        }
    );
    rt.tmprepo.publish_recipe(&spec).await.unwrap();
    let (built_spec, _) = BinaryPackageBuilder::from_recipe_with_solver(spec, solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    // This is not running an indexer, so there isn't a
    // metric name..
    let no_metric_name: Option<String> = None;

    // Write out an index so that `--index-use enabled` produces an indexed
    // repository instead of falling back to the plain spfs one.
    let local_repo = spk_storage::local_repository().await.unwrap();
    FlatBufferRepoIndex::index_repo(
        &vec![("local".to_string(), local_repo.into())],
        &no_metric_name,
    )
    .await
    .unwrap();

    let filename = rt.tmpdir.path().join("indexed-archive.spk");
    // `--enable-repo local` is needed as well; the local repository is
    // otherwise added ahead of the code that applies index use.
    let mut opt = Opt::try_parse_from([
        "export",
        "--enable-repo",
        "local",
        "--index-use",
        "enabled",
        &format!("{}", built_spec.ident().clone().to_version_ident()),
        filename.to_str().unwrap(),
    ])
    .unwrap();

    // Loading an index can quietly fall back to the plain spfs repository, so
    // check that this test is really exercising the wrapped case.
    let repos = opt
        .export
        .repos
        .get_repos_for_non_destructive_operation()
        .await
        .unwrap();
    assert!(
        repos
            .iter()
            .any(|(_, repo)| matches!(repo, spk_storage::RepositoryHandle::Indexed(_))),
        "the export command should have been given an indexed repository"
    );

    opt.export
        .run()
        .await
        .expect("export should succeed against an indexed repository");

    let mut tags = Vec::new();
    let mut tarfile = tar::Archive::new(std::fs::File::open(&filename).unwrap());
    for entry in tarfile.entries().unwrap() {
        let name = entry.unwrap().path().unwrap().to_string_lossy().to_string();
        if name.ends_with(".tag") {
            tags.push(name);
        }
    }
    assert!(
        tags.iter()
            .any(|tag| tag.contains("spk/pkg/spk-export-index-test/0.0.1")),
        "the exported archive should contain the package's build tags, got: {tags:?}"
    );
}

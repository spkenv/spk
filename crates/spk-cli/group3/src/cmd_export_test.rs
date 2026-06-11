// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Parser;
use futures::TryStreamExt;
use rstest::rstest;
use spfs::prelude::*;
use spfstest::spfstest;
use spk_build::{BinaryPackageBuilder, BuildSource};
use spk_cli_common::Run;
use spk_schema::foundation::option_map;
use spk_schema::{Package, recipe};
use spk_solve::SolverImpl;
use spk_storage::fixtures::*;

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    export: super::Export,
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

#[tokio::test]
async fn test_export_rejects_positional_file_and_file_flag_together() {
    // In single-package mode the output path may be given either as a
    // positional FILE or via --file/-f, but supplying both at once is
    // ambiguous and must be rejected.
    let mut opt = Opt::try_parse_from([
        "export",
        "spk-export-test/1.0.0",
        "positional.spk",
        "--file",
        "flag.spk",
    ])
    .unwrap();
    let result = opt.export.run().await;
    let err = result.expect_err("supplying both a positional FILE and --file should error");
    assert!(
        err.to_string().contains("not both"),
        "unexpected error message: {err}"
    );
}

#[test]
fn test_derive_env_filename_from_requests() {
    assert_eq!(
        super::Export::derive_env_filename(&[
            "spk-solve-export-top/1.0.0".to_string(),
            "spk-solve-export-dep/1.0.0".to_string(),
        ]),
        std::path::PathBuf::from("spk-solve-export-top-1.0.0-plus-1.spk")
    );
}

#[spfstest]
#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_export_solve_mode_only_exports_selected_builds(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;

    let dep_spec = recipe!(
        {
            "pkg": "spk-solve-export-selected-build/1.0.0",
            "build": {
                "options": [
                    {"var": "color"},
                ],
                "script": "touch /spfs/dep-file.txt",
            },
        }
    );
    let top_spec = recipe!(
        {
            "pkg": "spk-solve-export-selected-build-top/1.0.0",
            "build": {"script": "touch /spfs/top-file.txt"},
            "install": {
                "requirements": [{"pkg": "spk-solve-export-selected-build/1.0.0"}]
            },
        }
    );
    rt.tmprepo.publish_recipe(&dep_spec).await.unwrap();
    let (blue_dep, _) =
        BinaryPackageBuilder::from_recipe_with_solver(dep_spec.clone(), solver.clone())
            .with_source(BuildSource::LocalPath(".".into()))
            .build_and_publish(option_map! {"color" => "blue"}, &*rt.tmprepo)
            .await
            .unwrap();
    let (red_dep, _) = BinaryPackageBuilder::from_recipe_with_solver(dep_spec, solver.clone())
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {"color" => "red"}, &*rt.tmprepo)
        .await
        .unwrap();
    rt.tmprepo.publish_recipe(&top_spec).await.unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(top_spec, solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let filename = rt.tmpdir.path().join("selected-build-export.spk");
    let mut opt = Opt::try_parse_from([
        "export",
        "--local-repo-only",
        "--index-use",
        "disabled",
        "--opt",
        "color=red",
        "--env",
        "spk-solve-export-selected-build-top/1.0.0",
        "--file",
        &filename.to_string_lossy(),
    ])
    .unwrap();
    let result = opt.export.run().await;
    assert!(matches!(result, Ok(0)), "solve export should not fail");

    let mut entries = Vec::new();
    let mut tarfile = tar::Archive::new(std::fs::File::open(&filename).unwrap());
    for entry in tarfile.entries().unwrap() {
        entries.push(entry.unwrap().path().unwrap().to_string_lossy().to_string());
    }

    let red_build_tag = format!(
        "tags/spk/pkg/spk-solve-export-selected-build/1.0.0/{}",
        red_dep.ident().build()
    );
    let blue_build_tag = format!(
        "tags/spk/pkg/spk-solve-export-selected-build/1.0.0/{}",
        blue_dep.ident().build()
    );
    let red_build_spec_tag = format!(
        "tags/spk/spec/spk-solve-export-selected-build/1.0.0/{}.tag",
        red_dep.ident().build()
    );
    let blue_build_spec_tag = format!(
        "tags/spk/spec/spk-solve-export-selected-build/1.0.0/{}.tag",
        blue_dep.ident().build()
    );

    assert!(
        entries.iter().any(|entry| entry == &red_build_tag),
        "archive should include the selected dependency build"
    );
    assert!(
        entries.iter().any(|entry| entry == &red_build_spec_tag),
        "archive should include the selected dependency build spec"
    );
    assert!(
        entries.iter().all(|entry| entry != &blue_build_tag),
        "archive should not include an unselected dependency build"
    );
    assert!(
        entries.iter().all(|entry| entry != &blue_build_spec_tag),
        "archive should not include an unselected dependency build spec"
    );
}

#[spfstest]
#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_export_solve_mode_exports_whole_solution(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;

    let dep_spec = recipe!(
        {
            "pkg": "spk-solve-export-dep/1.0.0",
            "build": {"script": "touch /spfs/dep-file.txt"},
        }
    );
    let top_spec = recipe!(
        {
            "pkg": "spk-solve-export-top/1.0.0",
            "build": {"script": "touch /spfs/top-file.txt"},
            "install": {"requirements": [{"pkg": "spk-solve-export-dep/1.0.0"}]},
        }
    );
    rt.tmprepo.publish_recipe(&dep_spec).await.unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(dep_spec, solver.clone())
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();
    rt.tmprepo.publish_recipe(&top_spec).await.unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(top_spec, solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let filename = rt.tmpdir.path().join("solution-export.spk");
    let mut opt = Opt::try_parse_from([
        "export",
        "--local-repo-only",
        "--index-use",
        "disabled",
        "--env",
        "spk-solve-export-top/1.0.0",
        "--file",
        &filename.to_string_lossy(),
    ])
    .unwrap();
    let result = opt.export.run().await;
    assert!(matches!(result, Ok(0)), "solve export should not fail");

    let mut entries = Vec::new();
    let mut tarfile = tar::Archive::new(std::fs::File::open(&filename).unwrap());
    for entry in tarfile.entries().unwrap() {
        entries.push(entry.unwrap().path().unwrap().to_string_lossy().to_string());
    }
    entries.sort();
    assert!(
        entries
            .iter()
            .any(|entry| entry == "tags/spk/pkg/spk-solve-export-top/1.0.0"),
        "archive should include top package tag directory"
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry == "tags/spk/pkg/spk-solve-export-dep/1.0.0"),
        "archive should include dependency package tag directory"
    );

    let local_repo = spk_storage::local_repository().await.unwrap();
    let tar_repo = spfs::storage::tar::TarRepository::open(&filename)
        .await
        .unwrap();
    let tar_repo: spfs::storage::RepositoryHandle = tar_repo.into();
    let env_spec = tar_repo
        .iter_tags()
        .map_ok(|(spec, _)| spec)
        .try_collect()
        .await
        .unwrap();
    let syncer = spfs_cli_common::Sync {
        sync: false,
        resync: true,
        check: false,
        max_concurrent_manifests: 10,
        max_concurrent_payloads: 10,
        progress: None,
    }
    .get_syncer(&local_repo, &local_repo);
    let sync_result = syncer.clone_with_source(&tar_repo).sync_env(env_spec).await;
    assert!(
        sync_result.is_ok(),
        "import-equivalent sync should not fail"
    );
}

#[spfstest]
#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_export_env_mode_multiple_requests_with_custom_filename(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;

    // Two independent packages so we can request both at once and confirm
    // that --env accepts multiple requests alongside a custom output file.
    let first_spec = recipe!(
        {
            "pkg": "spk-env-export-first/1.0.0",
            "build": {"script": "touch /spfs/first-file.txt"},
        }
    );
    let second_spec = recipe!(
        {
            "pkg": "spk-env-export-second/1.0.0",
            "build": {"script": "touch /spfs/second-file.txt"},
        }
    );
    rt.tmprepo.publish_recipe(&first_spec).await.unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(first_spec, solver.clone())
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();
    rt.tmprepo.publish_recipe(&second_spec).await.unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(second_spec, solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let filename = rt.tmpdir.path().join("custom-multi-export.spk");
    let mut opt = Opt::try_parse_from([
        "export",
        "--local-repo-only",
        "--index-use",
        "disabled",
        "--env",
        "spk-env-export-first/1.0.0",
        "spk-env-export-second/1.0.0",
        "--file",
        &filename.to_string_lossy(),
    ])
    .unwrap();

    // Both requests should be parsed into env_requests, leaving the custom
    // --file to be used verbatim as the output path.
    assert_eq!(
        opt.export.env_requests,
        vec![
            "spk-env-export-first/1.0.0".to_string(),
            "spk-env-export-second/1.0.0".to_string(),
        ],
    );
    assert_eq!(opt.export.output_file.as_deref(), Some(filename.as_path()));

    let result = opt.export.run().await;
    assert!(matches!(result, Ok(0)), "env export should not fail");

    // The custom filename must be honored rather than a derived name.
    assert!(
        filename.exists(),
        "archive should be written to the custom --file path"
    );

    let mut entries = Vec::new();
    let mut tarfile = tar::Archive::new(std::fs::File::open(&filename).unwrap());
    for entry in tarfile.entries().unwrap() {
        entries.push(entry.unwrap().path().unwrap().to_string_lossy().to_string());
    }
    assert!(
        entries
            .iter()
            .any(|entry| entry == "tags/spk/pkg/spk-env-export-first/1.0.0"),
        "archive should include the first requested package"
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry == "tags/spk/pkg/spk-env-export-second/1.0.0"),
        "archive should include the second requested package"
    );
}

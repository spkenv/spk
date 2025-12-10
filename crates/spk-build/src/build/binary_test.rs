// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::path::PathBuf;

use rstest::rstest;
use spfs::encoding::EMPTY_DIGEST;
use spfs::prelude::*;
use spk_schema::foundation::env::data_path;
use spk_schema::foundation::fixtures::*;
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::{opt_name, option_map, version_ident};
use spk_schema::ident::{PkgRequest, RangeIdent, Request};
use spk_schema::{
    ComponentSpecList,
    Components,
    FromYaml,
    OptionMap,
    Package,
    Recipe,
    SpecRecipe,
    recipe,
};
use spk_solve::{Solution, SolverImpl};
use spk_solve_macros::make_repo;
use spk_storage::fixtures::*;
use spk_storage::{self as storage, Repository};

use super::{BinaryPackageBuilder, BuildSource};
use crate::build::SourcePackageBuilder;

#[rstest]
fn test_split_manifest_permissions() {
    use spfs::tracking::{Entry, EntryKind, Manifest};
    let mut manifest = Manifest::default();
    let dir = manifest.mkdir("bin").unwrap();
    dir.mode = 0o754;
    manifest
        .mknod(
            "bin/runme",
            Entry {
                kind: EntryKind::Blob(0),
                object: EMPTY_DIGEST.into(),
                mode: 0o555,
                entries: Default::default(),
                user_data: (),
                legacy_size: 0,
            },
        )
        .unwrap();
    let pkg = "mypkg/1.0.0/3I42H3S6".parse().unwrap();
    let spec = ComponentSpecList::default();
    let components = super::split_manifest_by_component(&pkg, &manifest, &spec).unwrap();
    let run = components.get(&Component::Run).unwrap();
    assert_eq!(run.get_path("bin").unwrap().mode, 0o754);
    assert_eq!(run.get_path("bin/runme").unwrap().mode, 0o555);
}

#[rstest]
#[tokio::test]
async fn test_empty_var_option_is_not_a_request() {
    let recipe = SpecRecipe::from_yaml(
        r#"{
            pkg: mypackage/1.0.0,
            build: {
                auto_host_vars: None,
                options: [
                    {var: something}
                ]
            }
        }"#,
    )
    .unwrap();
    let requirements = recipe.get_build_requirements(&option_map! {}).unwrap();
    assert!(
        requirements.is_empty(),
        "a var option with empty value should not create a solver request"
    )
}

#[rstest]
fn test_var_with_build_assigns_build() {
    let recipe = SpecRecipe::from_yaml(
        r#"{
        pkg: mypackage/1.0.0,
        build: {
            options: [
                {pkg: my-dep}
            ]
        }
    }"#,
    )
    .unwrap();
    // Assuming there is a request for a version with a specific build...
    let requirements = recipe
        .get_build_requirements(&option_map! {"my-dep" => "1.0.0/QYB6QLCN"})
        .unwrap();
    assert!(!requirements.is_empty());
    // ... a requirement is generated for that specific build.
    assert!(matches!(
        requirements.first().unwrap(),
        Request::Pkg(PkgRequest {
            pkg: RangeIdent { name, build: Some(digest), .. },
            ..
        })
     if name.as_str() == "my-dep" && digest.digest() == "QYB6QLCN"));
}

fn step_solver() -> SolverImpl {
    SolverImpl::Step(spk_solve::StepSolver::default())
}

fn resolvo_solver() -> SolverImpl {
    SolverImpl::Resolvo(spk_solve::ResolvoSolver::default())
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_workdir(tmpdir: tempfile::TempDir, #[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    let out_file = tmpdir.path().join("out.log");
    let recipe = recipe!({
        "pkg": "test/1.0.0",
        "build": {
            "script": format!("echo $PWD > {out_file:?}"),
            "validation": {
                "rules": [{"allow": "EmptyPackage"}]
            }
        }
    });

    rt.tmprepo.publish_recipe(&recipe).await.unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(recipe, solver)
        .with_source(BuildSource::LocalPath(tmpdir.path().to_owned()))
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let out = std::fs::read_to_string(out_file).unwrap();
    assert_eq!(
        out.trim(),
        dunce::canonicalize(&tmpdir)
            .expect("tmpdir can be canonicalized")
            .to_string_lossy()
    );
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_package_options(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    let dep_spec = recipe!(
        {"pkg": "dep/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    );
    let spec = recipe!(
        {
            "pkg": "top/1.2.3+r.2",
            "build": {
                "script": [
                    "set -ex",
                    "touch /spfs/top-file",
                    "test -f /spfs/dep-file",
                    "env | grep SPK",
                    r#"test ! -x "$SPK_PKG_dep""#,
                    r#"test "$SPK_PKG_dep_VERSION" == "1.0.0""#,
                    r#"test "$SPK_OPT_dep" == "1.0.0""#,
                    r#"test "$SPK_PKG_NAME" == "top""#,
                    r#"test "$SPK_PKG_VERSION" == "1.2.3+r.2""#,
                    r#"test "$SPK_PKG_VERSION_MAJOR" == "1""#,
                    r#"test "$SPK_PKG_VERSION_MINOR" == "2""#,
                    r#"test "$SPK_PKG_VERSION_PATCH" == "3""#,
                    r#"test "$SPK_PKG_VERSION_BASE" == "1.2.3""#,
                ],
                "options": [{"pkg": "dep"}],
            },
        }
    );

    rt.tmprepo.publish_recipe(&dep_spec).await.unwrap();

    BinaryPackageBuilder::from_recipe_with_solver(dep_spec, solver.clone())
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    rt.tmprepo.publish_recipe(&spec).await.unwrap();
    let variant = option_map! {
        // option should be set in final published spec
        "dep" => "2.0.0",
        // specific option takes precedence
        "top.dep" => "1.0.0",
    };
    let (spec, _) = BinaryPackageBuilder::from_recipe_with_solver(spec, solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(variant, &*rt.tmprepo)
        .await
        .unwrap();

    let build_options = rt
        .tmprepo
        .read_package(spec.ident())
        .await
        .unwrap()
        .option_values();
    assert_eq!(
        build_options.get(opt_name!("dep")),
        Some(&String::from("~1.0.0"))
    );
}

#[rstest]
#[case::camel_case("fromBuildEnv")]
#[case::lower_case("frombuildenv")]
#[tokio::test]
async fn test_build_package_pinning(
    #[case] from_build_env_str: &str,
    #[values(step_solver(), resolvo_solver())] solver: SolverImpl,
) {
    let rt = spfs_runtime().await;
    let dep_spec = recipe!(
        {"pkg": "dep/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    );
    let spec = recipe!(
        {
            "pkg": "top/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "options": [{"pkg": "dep/1.0.0"}],
            },
            "install": {"requirements": [{"pkg": "dep", from_build_env_str: "~x.x"}]},
        }
    );

    rt.tmprepo.publish_recipe(&dep_spec).await.unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(dep_spec, solver.clone())
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();
    rt.tmprepo.publish_recipe(&spec).await.unwrap();
    let (spec, _) = BinaryPackageBuilder::from_recipe_with_solver(spec, solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let spec = rt.tmprepo.read_package(spec.ident()).await.unwrap();
    let req = spec.runtime_requirements().first().unwrap().clone();
    match req {
        Request::Pkg(req) => {
            assert_eq!(&req.pkg.to_string(), "dep/~1.0");
        }
        _ => panic!("expected a package request"),
    }
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_package_pinning_optional_requirement(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    let dep1_spec = recipe!(
        {"pkg": "dep1/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    );
    let dep2_spec = recipe!(
        {"pkg": "dep2/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    );
    let spec = recipe!(
        {
            "pkg": "top/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "variants": [
                    { "dep1": "1.0.0" },
                    { "dep2": "1.0.0" },
                ],
            },
            "install": {"requirements": [
                {"pkg": "dep1", "fromBuildEnv": true, "ifPresentInBuildEnv": true},
                {"pkg": "dep2", "fromBuildEnv": true, "ifPresentInBuildEnv": true},
            ]},
        }
    );

    for dep_spec in [dep1_spec, dep2_spec] {
        rt.tmprepo.publish_recipe(&dep_spec).await.unwrap();

        BinaryPackageBuilder::from_recipe_with_solver(dep_spec, solver.clone())
            .with_source(BuildSource::LocalPath(".".into()))
            .with_repository(rt.tmprepo.clone())
            .build_and_publish(option_map! {}, &*rt.tmprepo)
            .await
            .unwrap();
    }

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let default_variants = spec.default_variants(&OptionMap::default());
    for (variant, expected_dep) in default_variants.iter().zip(["dep1", "dep2"].iter()) {
        let (spec, _) = BinaryPackageBuilder::from_recipe_with_solver(spec.clone(), solver.clone())
            .with_source(BuildSource::LocalPath(".".into()))
            .with_repository(rt.tmprepo.clone())
            .build_and_publish(variant, &*rt.tmprepo)
            .await
            .unwrap();

        let spec = rt.tmprepo.read_package(spec.ident()).await.unwrap();
        let req = spec.runtime_requirements().first().unwrap().clone();
        match req {
            Request::Pkg(req) => {
                assert_eq!(req.pkg.to_string(), format!("{expected_dep}/Binary:1.0.0"));
            }
            _ => panic!("expected a package request"),
        }
    }
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_package_pinning_optional_requirement_without_frombuildenv(
    #[case] solver: SolverImpl,
) {
    let rt = spfs_runtime().await;
    let dep1_spec = recipe!(
        {"pkg": "dep1/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    );
    let dep2_spec = recipe!(
        {"pkg": "dep2/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    );
    let spec = recipe!(
        {
            "pkg": "top/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "variants": [
                    { "dep1": "1.0.0" },
                    { "dep2": "1.0.0" },
                ],
            },
            "install": {"requirements": [
                {"pkg": "dep1", "ifPresentInBuildEnv": true},
                {"pkg": "dep2", "ifPresentInBuildEnv": true},
            ]},
        }
    );

    for dep_spec in [dep1_spec, dep2_spec] {
        rt.tmprepo.publish_recipe(&dep_spec).await.unwrap();

        BinaryPackageBuilder::from_recipe_with_solver(dep_spec, solver.clone())
            .with_source(BuildSource::LocalPath(".".into()))
            .with_repository(rt.tmprepo.clone())
            .build_and_publish(option_map! {}, &*rt.tmprepo)
            .await
            .unwrap();
    }

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let default_variants = spec.default_variants(&OptionMap::default());
    for (variant, expected_dep) in default_variants.iter().zip(["dep1", "dep2"].iter()) {
        let (spec, _) = BinaryPackageBuilder::from_recipe_with_solver(spec.clone(), solver.clone())
            .with_source(BuildSource::LocalPath(".".into()))
            .with_repository(rt.tmprepo.clone())
            .build_and_publish(variant, &*rt.tmprepo)
            .await
            .unwrap();

        let spec = rt.tmprepo.read_package(spec.ident()).await.unwrap();
        let req = spec.runtime_requirements().first().unwrap().clone();
        match req {
            Request::Pkg(req) => {
                assert_eq!(req.pkg.to_string(), *expected_dep);
            }
            _ => panic!("expected a package request"),
        }
    }
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_var_pinning_optional_requirement(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    let dep2_spec = recipe!(
        {"pkg": "dep2/1.0.0", "build": {
           "options": [{"var": "color"}],
           "script": "touch /spfs/dep-file"}
        }
    );
    let spec = recipe!(
        {
            "pkg": "top/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "variants": [
                    { "dep2": "1.0.0" },
                    { "dep2": "1.0.0", "dep2.color": "blue" },
                ],
            },
            "install": {"requirements": [
                {"pkg": "dep2", "fromBuildEnv": true},
                {"var": "dep2.color", "fromBuildEnv": true, "ifPresentInBuildEnv": true},
            ]},
        }
    );

    for dep_spec in [dep2_spec] {
        rt.tmprepo.publish_recipe(&dep_spec).await.unwrap();

        BinaryPackageBuilder::from_recipe_with_solver(dep_spec, solver.clone())
            .with_source(BuildSource::LocalPath(".".into()))
            .with_repository(rt.tmprepo.clone())
            .build_and_publish(option_map! {}, &*rt.tmprepo)
            .await
            .unwrap();
    }

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let default_variants = spec.default_variants(&OptionMap::default());
    for (variant, expected_dep) in default_variants.iter().zip(
        [
            // first variant should not have any var requirements
            None,
            // second variant does
            Some("var: dep2.color/blue".to_string()),
        ]
        .into_iter(),
    ) {
        let (spec, _) = BinaryPackageBuilder::from_recipe_with_solver(spec.clone(), solver.clone())
            .with_source(BuildSource::LocalPath(".".into()))
            .with_repository(rt.tmprepo.clone())
            .build_and_publish(variant, &*rt.tmprepo)
            .await
            .unwrap();

        let spec = rt.tmprepo.read_package(spec.ident()).await.unwrap();
        let req = spec
            .runtime_requirements()
            .iter()
            .find(|r| matches!(r, Request::Var(_)))
            .map(ToString::to_string);
        assert_eq!(req, expected_dep);
    }
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_package_missing_deps(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    let spec = recipe!(
        {
            "pkg": "dep/1.0.0",
            "build": {"script": "touch /spfs/dep-file"},
            "install": {"requirements": [{"pkg": "does-not-exist"}]},
        }
    );
    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    // should not fail to resolve build env and build even though
    // runtime dependency is missing in the current repos
    BinaryPackageBuilder::from_recipe_with_solver(spec, solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_var_pinning(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    let dep_spec = recipe!(
        {
            "pkg": "dep/1.0.0",
            "build": {
                "script": "touch /spfs/dep-file",
                "options": [{"var": "depvar/depvalue"}],
            },
        }
    );
    let spec = recipe!(
        {
            "pkg": "top/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "options": [
                    {"pkg": "dep/1.0.0"},
                    {"var": "topvar/topvalue"},
                ],
            },
            "install": {
                "requirements": [
                    {"var": "topvar", "fromBuildEnv": true},
                    {"var": "dep.depvar", "fromBuildEnv": true},
                ]
            },
        }
    );

    rt.tmprepo.publish_recipe(&dep_spec).await.unwrap();
    rt.tmprepo.publish_recipe(&spec).await.unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(dep_spec, solver.clone())
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();
    let (spec, _) = BinaryPackageBuilder::from_recipe_with_solver(spec, solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let spec = rt.tmprepo.read_package(spec.ident()).await.unwrap();
    let top_req = spec.runtime_requirements().first().unwrap().clone();
    match top_req {
        Request::Var(r) => assert_eq!(r.value.as_pinned(), Some("topvalue")),
        _ => panic!("expected var request"),
    }
    let depreq = spec.runtime_requirements()[1].clone();
    match depreq {
        Request::Var(r) => assert_eq!(r.value.as_pinned(), Some("depvalue")),
        _ => panic!("expected var request"),
    }
}

#[rstest]
#[tokio::test]
async fn embedded_stub_build_var_pinning() {
    let repo = make_repo!(
        [
            {
                "pkg": "parent/1.0.0",
                "install": {
                    "embedded": [
                        {
                            "pkg": "python/3.10.8",
                            "build": {
                                "options": [
                                    {
                                        "var": "abi/cp310"
                                    }
                                ]
                            }
                        }
                    ]
                }
            }
        ]
    );
    let stub_ident = repo
        .list_package_builds(&version_ident!("python/3.10.8"))
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let stub = repo.read_embed_stub(&stub_ident).await.unwrap();
    let python_abi = stub
        .get_build_options()
        .iter()
        .find_map(|opt| {
            opt.clone()
                .into_var()
                .and_then(|var_opt| (var_opt.var == "abi").then_some(var_opt))
        })
        .unwrap();
    assert_eq!(
        // This "not-cp310" will be ignored if the var is already pinned,
        // otherwise it will end up returning whatever value we provide as a
        // default. Passing None here returns the default value and would be a
        // false positive.
        python_abi.get_value(Some("not-cp310")),
        Some("cp310".to_string()),
        "expect var options in an embedded package to be pinned when saved as an embedded stub"
    );
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_bad_options(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    let spec = recipe!(
        {
            "pkg": "my-package/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "options": [{"var": "debug/off", "choices": ["on", "off"]}],
            },
        }
    );
    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let res = BinaryPackageBuilder::from_recipe_with_solver(spec, solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {"debug" => "false"}, &*rt.tmprepo)
        .await;

    assert!(
        matches!(
            res,
            Err(crate::Error::SpkSpecError(ref schema_err)) if matches!(&**schema_err, spk_schema::Error::String(_))
        ),
        "got {res:?}"
    );
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_package_source_cleanup(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    let spec = recipe!(
        {
            "pkg": "spk-test/1.0.0+beta.1",
            "sources": [
                {"path": "../../.site/spi/.spdev.yaml"},
                {"path": "../../examples", "subdir": "examples"},
            ],
            "build": {
                "script": [
                    "ls -la",
                    "mkdir build",
                    "touch build/some_build_file.out",
                    "touch examples/some_build_file.out",
                    "mkdir examples/build",
                    "touch examples/build/some_build_file.out",
                ]
            },
        }
    );
    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (src_pkg, _) = SourcePackageBuilder::from_recipe(spec.clone())
        .build_and_publish(".", &*rt.tmprepo)
        .await
        .unwrap();

    let (pkg, _) = BinaryPackageBuilder::from_recipe_with_solver(spec, solver)
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let digest = *storage::local_repository()
        .await
        .unwrap()
        .read_components(pkg.ident())
        .await
        .unwrap()
        .get(&Component::Run)
        .unwrap();
    let config = spfs::get_config().unwrap();
    let repo = config.get_local_repository().await.unwrap();
    let layer = repo.read_layer(digest).await.unwrap();

    let manifest_digest = match layer.manifest() {
        None => panic!("This layer should have a manifest digest!"),
        Some(d) => d,
    };
    let manifest = repo
        .read_manifest(*manifest_digest)
        .await
        .unwrap()
        .to_tracking_manifest();

    let entry = manifest.get_path(data_path(src_pkg.ident()));
    assert!(
        entry.is_none() || entry.unwrap().entries.is_empty(),
        "no files should be committed from source path"
    );
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_filters_reset_files(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;

    // Create a package that can be used as a dependency...
    {
        let spec = recipe!(
            {
                "pkg": "pkg-dep/1.0.0",
                "build": {
                    "script": [
                        "mkdir -p /spfs/include/dep",
                        "touch /spfs/include/dep/a.h",
                        "touch /spfs/include/dep/b.h",
                        "touch /spfs/include/dep/c.h",
                    ]
                },
            }
        );
        rt.tmprepo.publish_recipe(&spec).await.unwrap();

        let _ = SourcePackageBuilder::from_recipe(spec.clone())
            .build_and_publish(".", &*rt.tmprepo)
            .await
            .unwrap();

        let _ = BinaryPackageBuilder::from_recipe_with_solver(spec, solver.clone())
            .with_repository(rt.tmprepo.clone())
            .build_and_publish(option_map! {}, &*rt.tmprepo)
            .await
            .unwrap();
    }

    // Create a package that does weird stuff with the dependency's files.
    {
        let spec = recipe!(
            {
                "pkg": "my-pkg/1.0.0",
                "build": {
                    "options": [
                        { "pkg": "pkg-dep" }
                    ],
                    "script": [
                        // The net result should be that these files from the
                        // dependency are unmodified.
                        "mv /spfs/include/{dep,.dep.save}",
                        "mv /spfs/include/{.dep.save,dep}",
                        // Let's create our own file too.
                        "touch /spfs/include/dep/ours.h",
                    ]
                },
            }
        );
        rt.tmprepo.publish_recipe(&spec).await.unwrap();

        let _ = SourcePackageBuilder::from_recipe(spec.clone())
            .build_and_publish(".", &*rt.tmprepo)
            .await
            .unwrap();

        let (pkg, _) = BinaryPackageBuilder::from_recipe_with_solver(spec, solver)
            .with_repository(rt.tmprepo.clone())
            .build_and_publish(option_map! {}, &*rt.tmprepo)
            .await
            .unwrap();

        let digest = *storage::local_repository()
            .await
            .unwrap()
            .read_components(pkg.ident())
            .await
            .unwrap()
            .get(&Component::Run)
            .unwrap();
        let config = spfs::get_config().unwrap();
        let repo = config.get_local_repository().await.unwrap();
        let layer = repo.read_layer(digest).await.unwrap();

        let manifest_digest = match layer.manifest() {
            None => panic!("This layer should have a manifest digest!"),
            Some(d) => d,
        };
        let manifest = repo
            .read_manifest(*manifest_digest)
            .await
            .unwrap()
            .to_tracking_manifest();
        // my-pkg should not have the headers from pkg-dep inside it.
        let entry = manifest.get_path("include/dep/a.h");
        assert!(
            entry.is_none(),
            "should not capture the files from the dependency"
        );
        // But it should have the new header it created.
        let entry = manifest.get_path("include/dep/ours.h");
        assert!(
            entry.is_some(),
            "should capture the files newly created in the build"
        );
    }
}

#[rstest]
#[tokio::test]
async fn test_default_build_component() {
    let _rt = spfs_runtime().await;
    let spec = recipe!(
        {
            "pkg": "mypkg/1.0.0",
            "sources": [],
            "build": {
                "auto_host_vars": "None",
                "options": [{"pkg": "somepkg/1.0.0"}],
                "script": "echo building...",
            },
        }
    );

    let requirements = spec.get_build_requirements(&option_map! {}).unwrap();
    assert_eq!(requirements.len(), 1, "should have one build requirement");
    let req = requirements.first().unwrap();
    match req {
        Request::Pkg(req) => {
            assert_eq!(
                req.pkg.components,
                vec![Component::default_for_build()].into_iter().collect(),
                "a build request with no components should have the default build component ({}) injected automatically",
                Component::default_for_build()
            );
        }
        _ => panic!("expected pkg request"),
    }
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_components_metadata(#[case] solver: SolverImpl) {
    let mut rt = spfs_runtime().await;
    let spec = recipe!(
        {
            "pkg": "mypkg/1.0.0",
            "sources": [],
            "build": {
                "script": "echo building...",
            },
            "components": [{
                "name": "custom",
            }]
        }
    );
    rt.tmprepo.publish_recipe(&spec).await.unwrap();
    let (spec, _) = BinaryPackageBuilder::from_recipe_with_solver(spec.clone(), solver)
        .with_source(BuildSource::LocalPath(".".into()))
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();
    let runtime_repo = storage::RepositoryHandle::new_runtime();
    let published = rt.tmprepo.read_components(spec.ident()).await.unwrap();
    for component in spec.components().iter() {
        let digest = published.get(&component.name).unwrap();
        rt.runtime.reset_all().unwrap();
        rt.runtime.status.stack.clear();
        rt.runtime.push_digest(*digest);
        rt.runtime.save_state_to_storage().await.unwrap();
        spfs::remount_runtime(&rt.runtime).await.unwrap();
        // the package should be "available" no matter what
        // component is installed
        let installed = runtime_repo.read_components(spec.ident()).await.unwrap();
        let expected = vec![(component.name.clone(), *digest)]
            .into_iter()
            .collect();
        assert_eq!(
            installed, expected,
            "runtime repo should only show installed components"
        );
    }
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_add_startup_files(tmpdir: tempfile::TempDir, #[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    let recipe = recipe!(
        {
            "pkg": "testpkg",
            "install": {
                "environment": [
                    {"set": "TESTPKG", "value": true},
                    {"append": "TESTPKG", "value": "append"},
                    {"prepend": "TESTPKG", "value": "1.7"},
                ]
            },
        }
    );
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();

    let spec = recipe
        .generate_binary_build(&option_map! {}, &Solution::default())
        .unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(recipe, solver)
        .with_prefix(tmpdir.path().into())
        .generate_startup_scripts(&spec)
        .unwrap();

    let bash_file = tmpdir.path().join("etc/spfs/startup.d/spk_testpkg.sh");
    assert!(bash_file.exists());
    let tcsh_file = tmpdir.path().join("etc/spfs/startup.d/spk_testpkg.csh");
    assert!(tcsh_file.exists());

    let bash_value = std::process::Command::new("bash")
        .args(["--norc", "-c"])
        .arg(format!("source {bash_file:?}; printenv TESTPKG"))
        .output()
        .unwrap()
        .stdout;

    assert_eq!(String::from_utf8_lossy(&bash_value), "1.7:true:append\n");

    let tcsh_value = std::process::Command::new("tcsh")
        .arg("-fc")
        .arg(format!("source {tcsh_file:?}; printenv TESTPKG"))
        .output()
        .unwrap()
        .stdout;

    assert_eq!(String::from_utf8_lossy(&tcsh_value), "1.7:true:append\n");
}

#[rstest]
#[tokio::test]
#[should_panic]
async fn test_build_multiple_priority_startup_files() {
    let rt = spfs_runtime().await;
    let recipe = recipe!(
        {
            "pkg": "testpkg",
            "install": {
                "environment": [
                    {"priority": 99},
                    {"priority": 10},
                ]
            },
        }
    );
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();

    let _ = recipe.generate_binary_build(&option_map! {}, &Solution::default());
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_priority_startup_files(tmpdir: tempfile::TempDir, #[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    let recipe = recipe!(
        {
            "pkg": "testpkg",
            "install": {
                "environment": [
                    {"priority": 99},
                ]
            },
        }
    );
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();

    let spec = recipe
        .generate_binary_build(&option_map! {}, &Solution::default())
        .unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(recipe, solver)
        .with_prefix(tmpdir.path().into())
        .generate_startup_scripts(&spec)
        .unwrap();

    let bash_file = tmpdir.path().join("etc/spfs/startup.d/99_spk_testpkg.sh");
    assert!(bash_file.exists());
    let tcsh_file = tmpdir.path().join("etc/spfs/startup.d/99_spk_testpkg.csh");
    assert!(tcsh_file.exists());
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_variable_substitution_in_build_env(
    tmpdir: tempfile::TempDir,
    #[case] solver: SolverImpl,
) {
    let rt = spfs_runtime().await;
    let dep_spec = recipe!(
        {
            "pkg": "dep/1.0.0",
            "build": {
                "script": "touch /spfs/dep-file",
                "options": [{"var": "depvar/depvalue"}],
            },
        }
    );
    let spec = recipe!(
        {
            "pkg": "testpkg/1.0.0",
            "build": {
                "script": [
                    "env",
                ],
                "options": [
                    {"pkg": "dep/1.0.0"},
                ],
            },
            "install": {
                "environment": [
                    {"set": "DEPVER1", "value": "$SPK_PKG_dep_VERSION_BASE"},
                    {"set": "DEPVER2", "value": "${SPK_PKG_dep_VERSION_BASE}"},
                    {"set": "AT_ENV_TIME", "value": "I'm using dep version $${DEPVER1}"}
                ]
            },
        }
    );

    rt.tmprepo.publish_recipe(&dep_spec).await.unwrap();
    rt.tmprepo.publish_recipe(&spec).await.unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(dep_spec, solver.clone())
        .with_source(BuildSource::LocalPath(tmpdir.path().to_owned()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    BinaryPackageBuilder::from_recipe_with_solver(spec, solver)
        .with_source(BuildSource::LocalPath(tmpdir.path().to_owned()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let bash_file = PathBuf::from("/spfs/etc/spfs/startup.d/spk_testpkg.sh");
    assert!(bash_file.exists());
    let tcsh_file = PathBuf::from("/spfs/etc/spfs/startup.d/spk_testpkg.csh");
    assert!(tcsh_file.exists());

    let bash_value = std::process::Command::new("bash")
        .args(["--norc", "-c"])
        .arg(format!("source {bash_file:?}; printenv AT_ENV_TIME"))
        .output()
        .unwrap()
        .stdout;

    assert_eq!(
        String::from_utf8_lossy(&bash_value),
        "I'm using dep version 1.0.0\n"
    );

    let bash_value = std::process::Command::new("bash")
        .args(["--norc", "-c"])
        .arg(format!("source {bash_file:?}; printenv DEPVER1"))
        .output()
        .unwrap()
        .stdout;

    assert_eq!(String::from_utf8_lossy(&bash_value), "1.0.0\n");

    let tcsh_value = std::process::Command::new("tcsh")
        .arg("-fc")
        .arg(format!("source {tcsh_file:?}; printenv AT_ENV_TIME"))
        .output()
        .unwrap()
        .stdout;

    assert_eq!(
        String::from_utf8_lossy(&tcsh_value),
        "I'm using dep version 1.0.0\n",
    );

    let tcsh_value = std::process::Command::new("tcsh")
        .arg("-fc")
        .arg(format!("source {tcsh_file:?}; printenv DEPVER2"))
        .output()
        .unwrap()
        .stdout;

    assert_eq!(String::from_utf8_lossy(&tcsh_value), "1.0.0\n");
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
#[serial_test::serial(env)] // env manipulation must be reliable
async fn test_dependant_variable_substitution_in_startup_files(
    tmpdir: tempfile::TempDir,
    #[case] solver: SolverImpl,
) {
    let rt = spfs_runtime().await;

    // Safety: this is unsafe. serial_test is used to prevent multiple tests
    // from changing the environment at the same time.
    unsafe {
        std::env::set_var("TEST", "This is a test");
    }

    let recipe = recipe!(
        {
            "pkg": "testpkg",
            "install": {
                "environment": [
                    {"set": "TESTPKG", "value": "${TEST}"},
                    {"set": "DEPENDANT_TESTPKG", "value": "$${TESTPKG}"},
                ]
            },
        }
    );
    rt.tmprepo.publish_recipe(&recipe).await.unwrap();

    let spec = recipe
        .generate_binary_build(&option_map! {}, &Solution::default())
        .unwrap();
    BinaryPackageBuilder::from_recipe_with_solver(recipe, solver)
        .with_prefix(tmpdir.path().into())
        .generate_startup_scripts(&spec)
        .unwrap();

    let bash_file = tmpdir.path().join("etc/spfs/startup.d/spk_testpkg.sh");
    assert!(bash_file.exists());
    let tcsh_file = tmpdir.path().join("etc/spfs/startup.d/spk_testpkg.csh");
    assert!(tcsh_file.exists());

    let bash_value = std::process::Command::new("bash")
        .args(["--norc", "-c"])
        .arg(format!("source {bash_file:?}; printenv DEPENDANT_TESTPKG"))
        .output()
        .unwrap()
        .stdout;

    assert_eq!(String::from_utf8_lossy(&bash_value), "This is a test\n");

    let tcsh_value = std::process::Command::new("tcsh")
        .arg("-fc")
        .arg(format!("source {tcsh_file:?}; printenv DEPENDANT_TESTPKG"))
        .output()
        .unwrap()
        .stdout;

    assert_eq!(String::from_utf8_lossy(&tcsh_value), "This is a test\n");
}

#[rstest]
fn test_path_and_parents() {
    use relative_path::RelativePathBuf;
    let path = RelativePathBuf::from("some/deep/path");
    let hierarchy = super::path_and_parents(path);
    assert_eq!(
        hierarchy,
        vec![
            RelativePathBuf::from("some/deep/path"),
            RelativePathBuf::from("some/deep"),
            RelativePathBuf::from("some"),
        ]
    );
}

#[rstest]
#[case::step(step_solver())]
#[case::resolvo(resolvo_solver())]
#[tokio::test]
async fn test_build_options_respect_components(#[case] solver: SolverImpl) {
    let rt = spfs_runtime().await;
    // Create a base package that has a couple components with unique
    // contents.
    let base_spec = recipe!(
        {
            "pkg": "base/1.0.0",
            "sources": [],
            "build": {
                "script": "echo run > /spfs/run ; echo build > /spfs/build",
            },
            "install": {
                "components": [
                    {
                        "name": "build",
                        "files": ["build"],
                    },
                    {
                        "name": "run",
                        "files": ["run"],
                    }
                ]
            }
        }
    );
    // Create a top package that depends on a specific component of base.
    let top_spec = recipe!(
        {
            "pkg": "top/1.0.0",
            "sources": [],
            "build": {
                "options": [
                    {
                        // Ask for the "run" component of pkg base.
                        "pkg": "base:run"
                    }
                ],
                "script": [
                    // This "build" file should not exist in our build env.
                    "test -f /spfs/build && exit 1",
                    // This "run" file should exist in our build env.
                    "test -f /spfs/run"
                ],
                "validation": {
                    "rules": [{"allow": "EmptyPackage"}]
                }
            },
        }
    );
    rt.tmprepo.publish_recipe(&base_spec).await.unwrap();
    rt.tmprepo.publish_recipe(&top_spec).await.unwrap();

    SourcePackageBuilder::from_recipe(base_spec.clone())
        .build_and_publish(".", &*rt.tmprepo)
        .await
        .unwrap();
    let _base_pkg = BinaryPackageBuilder::from_recipe_with_solver(base_spec, solver.clone())
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    SourcePackageBuilder::from_recipe(top_spec.clone())
        .build_and_publish(".", &*rt.tmprepo)
        .await
        .unwrap();

    let r = BinaryPackageBuilder::from_recipe_with_solver(top_spec, solver)
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(option_map! {}, &*rt.tmprepo)
        .await;

    if let Err(err) = r {
        println!("{err}");
        panic!("build script for 'top' expected to succeed");
    }
}

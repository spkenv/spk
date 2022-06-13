// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;
use spfs::{encoding::EMPTY_DIGEST, prelude::*};

use super::{BinaryPackageBuilder, BuildSource};
use crate::{
    api,
    build::SourcePackageBuilder,
    fixtures::*,
    storage::{self, Repository},
};

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
                kind: EntryKind::Blob,
                object: EMPTY_DIGEST.into(),
                mode: 0o555,
                size: 0,
                entries: Default::default(),
            },
        )
        .unwrap();
    let pkg = "mypkg".parse().unwrap();
    let spec = crate::api::ComponentSpecList::default();
    let components = super::split_manifest_by_component(&pkg, &manifest, &spec).unwrap();
    let run = components.get(&crate::api::Component::Run).unwrap();
    assert_eq!(run.get_path("bin").unwrap().mode, 0o754);
    assert_eq!(run.get_path("bin/runme").unwrap().mode, 0o555);
}

#[rstest]
fn test_empty_var_option_is_not_a_request() {
    let spec: crate::api::Spec = serde_yaml::from_str(
        r#"{
        pkg: mypackage/1.0.0,
        build: {
            options: [
                {var: something}
            ]
        }
    }"#,
    )
    .unwrap();
    let builder = super::BinaryPackageBuilder::from_spec(spec);
    let requirements = builder.get_build_requirements().unwrap();
    assert!(
        requirements.is_empty(),
        "a var option with empty value should not create a solver request"
    )
}

#[rstest]
fn test_build_workdir(tmpdir: tempdir::TempDir) {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let out_file = tmpdir.path().join("out.log");
    let mut spec = crate::spec!(
        {"pkg": "test/1.0.0"}
    );

    rt.tmprepo.publish_spec(spec.clone()).unwrap();
    spec.build.script = vec![format!("echo $PWD > {:?}", out_file)];

    BinaryPackageBuilder::from_spec(spec)
        .with_source(BuildSource::LocalPath(tmpdir.path().to_owned()))
        .build()
        .unwrap();

    let out = std::fs::read_to_string(out_file).unwrap();
    assert_eq!(
        out.trim(),
        &tmpdir
            .path()
            .canonicalize()
            .expect("tmpdir can be canonicalized")
            .to_string_lossy()
    );
}

#[rstest]
fn test_build_package_options() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let dep_spec = crate::spec!(
        {"pkg": "dep/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    );
    let spec = crate::spec!(
        {
            "pkg": "top/1.2.3+r.2",
            "build": {
                "script": [
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

    rt.tmprepo.publish_spec(dep_spec.clone()).unwrap();

    BinaryPackageBuilder::from_spec(dep_spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build()
        .unwrap();

    rt.tmprepo.publish_spec(spec.clone()).unwrap();
    let spec = BinaryPackageBuilder::from_spec(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        // option should be set in final published spec
        .with_option("dep", "2.0.0")
        // specific option takes precedence
        .with_option("top.dep", "1.0.0")
        .build()
        .unwrap();

    let build_options = rt
        .tmprepo
        .read_spec(&spec.pkg)
        .unwrap()
        .resolve_all_options(
            // given value should be ignored after build
            &crate::option_map! {"dep" => "7"},
        );
    assert_eq!(build_options.get("dep"), Some(&String::from("~1.0.0")));
}

#[rstest]
fn test_build_package_pinning() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let dep_spec = crate::spec!(
        {"pkg": "dep/1.0.0", "build": {"script": "touch /spfs/dep-file"}}
    );
    let spec = crate::spec!(
        {
            "pkg": "top/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "options": [{"pkg": "dep/1.0.0"}],
            },
            "install": {"requirements": [{"pkg": "dep", "fromBuildEnv": "~x.x"}]},
        }
    );

    rt.tmprepo.publish_spec(dep_spec.clone()).unwrap();
    BinaryPackageBuilder::from_spec(dep_spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build()
        .unwrap();
    rt.tmprepo.publish_spec(spec.clone()).unwrap();
    let spec = BinaryPackageBuilder::from_spec(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build()
        .unwrap();

    let spec = rt.tmprepo.read_spec(&spec.pkg).unwrap();
    let req = spec.install.requirements.get(0).unwrap();
    match req {
        api::Request::Pkg(req) => {
            assert_eq!(&req.pkg.to_string(), "dep/~1.0");
        }
        _ => panic!("expected a package request"),
    }
}

#[rstest]
fn test_build_package_missing_deps() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!(
        {
            "pkg": "dep/1.0.0",
            "build": {"script": "touch /spfs/dep-file"},
            "install": {"requirements": [{"pkg": "does-not-exist"}]},
        }
    );
    rt.tmprepo.publish_spec(spec.clone()).unwrap();

    // should not fail to resolve build env and build even though
    // runtime dependency is missing in the current repos
    BinaryPackageBuilder::from_spec(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build()
        .unwrap();
}

#[rstest]
fn test_build_var_pinning() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let dep_spec = crate::spec!(
        {
            "pkg": "dep/1.0.0",
            "build": {
                "script": "touch /spfs/dep-file",
                "options": [{"var": "depvar/depvalue"}],
            },
        }
    );
    let spec = crate::spec!(
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

    rt.tmprepo.publish_spec(dep_spec.clone()).unwrap();
    rt.tmprepo.publish_spec(spec.clone()).unwrap();
    BinaryPackageBuilder::from_spec(dep_spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build()
        .unwrap();
    let spec = BinaryPackageBuilder::from_spec(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build()
        .unwrap();

    let spec = rt.tmprepo.read_spec(&spec.pkg).unwrap();
    let top_req = spec.install.requirements.get(0).unwrap();
    match top_req {
        api::Request::Var(r) => assert_eq!(&r.value, "topvalue"),
        _ => panic!("expected var request"),
    }
    let depreq = spec.install.requirements.get(1).unwrap();
    match depreq {
        api::Request::Var(r) => assert_eq!(&r.value, "depvalue"),
        _ => panic!("expected var request"),
    }
}

#[rstest]
fn test_build_bad_options() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!(
        {
            "pkg": "my-package/1.0.0",
            "build": {
                "script": [
                    "touch /spfs/top-file",
                ],
                "options": [{"var": "debug", "choices": ["on", "off"]}],
            },
        }
    );
    rt.tmprepo.publish_spec(spec.clone()).unwrap();

    let res = BinaryPackageBuilder::from_spec(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_option("debug", "false")
        .build();

    assert!(matches!(res, Err(crate::Error::String(_))), "got {:?}", res);
}

#[rstest]
fn test_build_package_source_cleanup() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!(
        {
            "pkg": "spk-test/1.0.0+beta.1",
            "sources": [
                {"path": "./.site/spi/.spdev.yaml"},
                {"path": "./examples", "subdir": "examples"},
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
    rt.tmprepo.publish_spec(spec.clone()).unwrap();

    let src_pkg = SourcePackageBuilder::from_spec(spec.clone())
        .with_target_repository(rt.tmprepo.clone())
        .build()
        .unwrap();

    let pkg = BinaryPackageBuilder::from_spec(spec)
        .with_repository(rt.tmprepo.clone())
        .build()
        .unwrap();

    let digest = *crate::HANDLE
        .block_on(storage::local_repository())
        .unwrap()
        .get_package(&pkg.pkg)
        .unwrap()
        .get(&api::Component::Run)
        .unwrap();
    let config = spfs::get_config().unwrap();
    let repo = crate::HANDLE
        .block_on(config.get_local_repository())
        .unwrap();
    let layer = crate::HANDLE.block_on(repo.read_layer(digest)).unwrap();
    let manifest = crate::HANDLE
        .block_on(repo.read_manifest(layer.manifest))
        .unwrap()
        .unlock();
    let entry = manifest
        .get_path(crate::build::data_path(&src_pkg))
        .unwrap();
    assert!(
        entry.entries.is_empty(),
        "no files should be committed from source path"
    );
}

#[rstest]
fn test_build_package_requirement_propagation() {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let base_spec = crate::spec!(
        {
            "pkg": "base/1.0.0",
            "sources": [],
            "build": {
                "options": [{"var": "inherited/val", "inheritance": "Strong"}],
                "script": "echo building...",
            },
        }
    );
    let top_spec = crate::spec!(
        {
            "pkg": "top/1.0.0",
            "sources": [],
            "build": {"options": [{"pkg": "base"}], "script": "echo building..."},
        }
    );
    rt.tmprepo.publish_spec(base_spec.clone()).unwrap();
    rt.tmprepo.publish_spec(top_spec.clone()).unwrap();

    SourcePackageBuilder::from_spec(base_spec.clone())
        .with_target_repository(rt.tmprepo.clone())
        .build()
        .unwrap();
    let _base_pkg = BinaryPackageBuilder::from_spec(base_spec)
        .with_repository(rt.tmprepo.clone())
        .build()
        .unwrap();

    SourcePackageBuilder::from_spec(top_spec.clone())
        .with_target_repository(rt.tmprepo.clone())
        .build()
        .unwrap();
    let top_pkg = BinaryPackageBuilder::from_spec(top_spec)
        .with_repository(rt.tmprepo.clone())
        .build()
        .unwrap();

    assert_eq!(top_pkg.build.options.len(), 2, "should get option added");
    let opt = top_pkg.build.options.get(1).unwrap();
    match opt {
        api::Opt::Var(opt) => {
            assert_eq!(
                &opt.var, "base.inherited",
                "should be inherited as package option"
            );
            assert_eq!(
                opt.inheritance,
                api::Inheritance::Weak,
                "inherited option should have weak inheritance"
            );
        }
        _ => panic!("should be given inherited option"),
    }

    assert_eq!(
        top_pkg.install.requirements.len(),
        1,
        "should get install requirement"
    );
    let req = top_pkg.install.requirements.get(0).unwrap();
    match req {
        api::Request::Var(req) => {
            assert_eq!(
                req.var, "base.inherited",
                "should be inherited with package namespace"
            );
            assert!(!req.pin, "should not be pinned after build");
            assert_eq!(req.value, "val", "should be rendered to build time var");
        }
        _ => panic!("should be given var request"),
    }
}

#[rstest]
fn test_default_build_component() {
    let _guard = crate::HANDLE.enter();
    let _rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!(
        {
            "pkg": "mypkg/1.0.0",
            "sources": [],
            "build": {
                "options": [{"pkg": "somepkg/1.0.0"}],
                "script": "echo building...",
            },
        }
    );
    let builder = BinaryPackageBuilder::from_spec(spec);
    let requirements = builder.get_build_requirements().unwrap();
    assert_eq!(requirements.len(), 1, "should have one build requirement");
    let req = requirements.get(0).unwrap();
    match req {
        api::Request::Pkg(req) => {
            assert_eq!(req.pkg.components, vec![api::Component::Build].into_iter().collect(),
                "a build request with no components should have the default build component injected automatically"
            );
        }
        _ => panic!("expected pkg request"),
    }
}

#[rstest]
fn test_build_components_metadata() {
    let _guard = crate::HANDLE.enter();
    let mut rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!(
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
    rt.tmprepo.publish_spec(spec.clone()).unwrap();
    let spec = BinaryPackageBuilder::from_spec(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .build()
        .unwrap();
    let runtime_repo = storage::RepositoryHandle::new_runtime();
    let published = rt.tmprepo.get_package(&spec.pkg).unwrap();
    for component in spec.install.components.iter() {
        let digest = published.get(&component.name).unwrap();
        rt.runtime.reset_all().unwrap();
        rt.runtime.status.stack.clear();
        rt.runtime.push_digest(*digest);
        crate::HANDLE
            .block_on(rt.runtime.save_state_to_storage())
            .unwrap();
        crate::HANDLE
            .block_on(spfs::remount_runtime(&rt.runtime))
            .unwrap();
        // the package should be "available" no matter what
        // component is installed
        let installed = runtime_repo.get_package(&spec.pkg).unwrap();
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
fn test_build_add_startup_files(tmpdir: tempdir::TempDir) {
    let _guard = crate::HANDLE.enter();
    let rt = crate::HANDLE.block_on(spfs_runtime());
    let spec = crate::spec!(
        {
            "pkg": "testpkg",
            "install": {
                "environment": [
                    {"set": "TESTPKG", "value": true},
                    {"append": "TESTPKG", "value": "append"},
                    {"prepend": "TESTPKG", "value": 1.7},
                ]
            },
        }
    );
    rt.tmprepo.publish_spec(spec.clone()).unwrap();

    BinaryPackageBuilder::from_spec(spec)
        .with_prefix(tmpdir.path().into())
        .generate_startup_scripts()
        .unwrap();

    let bash_file = tmpdir.path().join("etc/spfs/startup.d/spk_testpkg.sh");
    assert!(bash_file.exists());
    let tcsh_file = tmpdir.path().join("etc/spfs/startup.d/spk_testpkg.csh");
    assert!(tcsh_file.exists());

    let bash_value = std::process::Command::new("bash")
        .args(&["--norc", "-c"])
        .arg(format!("source {bash_file:?}; printenv TESTPKG"))
        .output()
        .unwrap()
        .stdout;

    assert_eq!(bash_value.as_slice(), b"1.7:true:append\n");

    let tcsh_value = std::process::Command::new("tcsh")
        .arg("-fc")
        .arg(format!("source {tcsh_file:?}; printenv TESTPKG"))
        .output()
        .unwrap()
        .stdout;

    assert_eq!(tcsh_value.as_slice(), b"1.7:true:append\n");
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

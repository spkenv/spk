// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::fmt::Arguments;
use std::sync::Mutex;

use clap::Parser;
use itertools::Itertools;
use spfs::config::Remote;
use spfs::encoding::EMPTY_DIGEST;
use spfs::RemoteAddress;
use spk_build::{BinaryPackageBuilder, BuildSource};
use spk_schema::foundation::ident_component::Component;
use spk_schema::foundation::option_map;
use spk_schema::recipe;
use spk_solve::spec;
use spk_storage::fixtures::*;

use super::{Du, Output, Run};

#[derive(Default)]
struct OutputToVec {
    vec: Mutex<Vec<String>>,
    warnings: Mutex<Vec<String>>,
}

impl Output for OutputToVec {
    fn println(&self, line: Arguments) {
        self.vec.try_lock().unwrap().push(line.to_string());
    }

    fn warn(&self, line: Arguments) {
        self.warnings.try_lock().unwrap().push(line.to_string());
    }
}

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    du: Du<OutputToVec>,
}

#[tokio::test]
async fn test_du_trivially_works() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = recipe!(
        {"pkg": "my-pkg/1.0.0", "build": {"script": "echo Hello World!"}}
    );

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["du", "local/"]).unwrap();
    opt.du.run().await.unwrap();

    assert_eq!(opt.du.output.vec.lock().unwrap().len(), 8);
}

#[tokio::test]
async fn test_du_warnings_when_object_is_tree_or_blob() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    // publish package without publishing spec
    let components = vec![
        (Component::Run, EMPTY_DIGEST.into()),
        (Component::Build, EMPTY_DIGEST.into()),
    ]
    .into_iter()
    .collect();

    let recipe = recipe!({"pkg": "my-pkg/1.0.0"});
    remote_repo.publish_recipe(&recipe).await.unwrap();
    let spec = spec!({"pkg": "my-pkg/1.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(&spec, &components)
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["du", "origin/my-pkg/1.0.0/BGSHW3CN/", "-s"]).unwrap();
    opt.du.run().await.unwrap();
    assert_eq!(opt.du.output.warnings.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn test_du_non_existing_version() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = recipe!(
        {"pkg": "my-pkg/1.0.0", "build": {"script": "echo Hello World!"}}
    );

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["du", "local/my-pkg/1.0.1", "-s"]).unwrap();
    opt.du.run().await.unwrap();
    assert_eq!(opt.du.output.vec.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn test_du_out_of_range_input() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = recipe!(
        {"pkg": "my-pkg/1.0.0", "build": {"script": "echo Hello World!"}}
    );

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from([
        "du",
        "local/my-pkg/1.0.0/3I42H3S6/:build/spk/pkg/my-pkg/1.0.0/3I42H3S6/options.json/",
    ])
    .unwrap();
    opt.du.run().await.unwrap();
    assert_eq!(opt.du.output.vec.lock().unwrap().len(), 0);
}

#[tokio::test]
async fn test_du_is_not_counting_links() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = recipe!(
        {"pkg": "my-pkg/1.0.0", "build": {"script": "echo Hello World!"}}
    );

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["du", "local/my-pkg/1.0.0/3I42H3S6/", "-s"]).unwrap();
    opt.du.run().await.unwrap();
    let output_vec = opt.du.output.vec.lock().unwrap();
    let mut build_component_output = output_vec[0].split(' ').collect_vec();
    let mut run_component_output = output_vec[1].split(' ').collect_vec();

    run_component_output.retain(|c| !c.is_empty());
    build_component_output.retain(|c| !c.is_empty());

    assert_eq!(run_component_output[0].parse::<i32>().unwrap(), 0);
    assert_ne!(build_component_output[0].parse::<i32>().unwrap(), 0);
}

#[tokio::test]
async fn test_du_is_counting_links() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = recipe!(
        {"pkg": "my-pkg/1.0.0", "build": {"script": "echo Hello World!"}}
    );

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["du", "local/my-pkg/1.0.0/3I42H3S6/", "-sl"]).unwrap();
    opt.du.run().await.unwrap();
    let output_vec = opt.du.output.vec.lock().unwrap();
    let mut build_component_output = output_vec[0].split(' ').collect_vec();
    let mut run_component_output = output_vec[1].split(' ').collect_vec();

    run_component_output.retain(|c| !c.is_empty());
    build_component_output.retain(|c| !c.is_empty());

    assert_eq!(
        run_component_output[0].parse::<i32>().unwrap(),
        build_component_output[0].parse::<i32>().unwrap()
    );
}

#[tokio::test]
async fn test_du_total_size() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = recipe!(
        {"pkg": "my-pkg/1.0.0", "build": {"script": "echo Hello World!"}}
    );

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["du", "local/my-pkg/1.0.0/3I42H3S6/:build/", "-c"]).unwrap();
    opt.du.run().await.unwrap();

    let total = match opt.du.output.vec.lock().unwrap().pop() {
        Some(value) => {
            let mut values = value.split(' ').collect_vec();
            values.retain(|c| !c.is_empty());
            values[0].parse::<i32>().unwrap()
        }
        None => 0,
    };

    let mut calculated_total_size_from_output = 0;
    for output in opt.du.output.vec.lock().unwrap().iter() {
        let mut output_vec = output.split(' ').collect_vec();
        output_vec.retain(|c| !c.is_empty());
        calculated_total_size_from_output += output_vec[0].parse::<i32>().unwrap()
    }
    assert_eq!(total, calculated_total_size_from_output);
}

#[tokio::test]
async fn test_du_summarize_output_enabled() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = recipe!(
        {"pkg": "my-pkg/1.0.0", "build": {"script": "echo Hello World!"}}
    );

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["du", "local/my-pkg", "-s"]).unwrap();
    opt.du.run().await.unwrap();
    assert_eq!(opt.du.output.vec.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn test_du_summarize_output_is_not_enabled() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = recipe!(
        {"pkg": "my-pkg/1.0.0", "build": {"script": "echo Hello World!"}}
    );

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["du", "local/my-pkg"]).unwrap();
    opt.du.run().await.unwrap();
    assert_eq!(opt.du.output.vec.lock().unwrap().len(), 8); // Output should show 8 files. 4 from build and 4 from run.
}

#[tokio::test]
async fn test_deprecate_flag() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = recipe!(
        {"pkg": "my-pkg/1.0.0", "build": {"script": "echo Hello World!"}, "deprecated": true}
    );

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let mut opt_without_deprecate_flag = Opt::try_parse_from(["du", "local/my-pkg"]).unwrap();
    opt_without_deprecate_flag.du.run().await.unwrap();
    assert_eq!(
        opt_without_deprecate_flag
            .du
            .output
            .vec
            .lock()
            .unwrap()
            .len(),
        0
    );

    let mut opt_with_deprecate_flag = Opt::try_parse_from(["du", "local/my-pkg", "-ds"]).unwrap();
    opt_with_deprecate_flag.du.run().await.unwrap();
    assert_eq!(
        opt_with_deprecate_flag.du.output.vec.lock().unwrap().len(),
        1
    );
}

#[tokio::test]
async fn test_human_readable_flag() {
    let mut rt = spfs_runtime().await;
    let remote_repo = spfsrepo().await;
    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let spec = recipe!(
        {"pkg": "my-pkg/1.0.0", "build": {"script": "echo Hello World!"}}
    );

    rt.tmprepo.publish_recipe(&spec).await.unwrap();

    let (_spec, _) = BinaryPackageBuilder::from_recipe(spec)
        .with_source(BuildSource::LocalPath(".".into()))
        .with_repository(rt.tmprepo.clone())
        .build_and_publish(&option_map! {}, &*rt.tmprepo)
        .await
        .unwrap();

    let mut opt = Opt::try_parse_from(["du", "local/my-pkg/1.0.0/3I42H3S6/:build/", "-H"]).unwrap();
    opt.du.run().await.unwrap();

    let units = ["B", "Ki", "Mi", "Gi", "Ti"];
    assert!(opt
        .du
        .output
        .vec
        .lock()
        .unwrap()
        .iter()
        .any(|i| units.iter().any(|&u| i.contains(u))));
}

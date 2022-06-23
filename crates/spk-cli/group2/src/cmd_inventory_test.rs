// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use clap::Parser;
use rstest::rstest;
use spfs::RemoteAddress;
use spfs::config::Remote;
use spk_schema::foundation::ident_component::Component;
use spk_schema::recipe;
use spk_solve::spec;
use spk_storage::fixtures::*;

use super::{CommandArgs, Inventory, Output, Run};

#[derive(Default)]
struct OutputToVec {
    vec: Vec<String>,
    warnings: Vec<String>,
}

impl Output for OutputToVec {
    fn println(&mut self, line: String) {
        self.vec.push(line);
    }

    fn warn(&mut self, line: String) {
        self.warnings.push(line);
    }
}

#[derive(Parser)]
struct Opt {
    #[clap(flatten)]
    inventory: Inventory<OutputToVec>,
}

// Test helper for the inventory command test
async fn three_level_deps_repo() -> TempRepo {
    let remote_repo = spfsrepo().await;

    // Setup a few packages with dependencies:
    // parent -> middle -> lowest
    let recipe1 = recipe!({"pkg": "parent/1.0.0"});
    remote_repo.publish_recipe(&recipe1).await.unwrap();
    let recipe2 = recipe!({"pkg": "middle/2.0.0"});
    remote_repo.publish_recipe(&recipe2).await.unwrap();
    let recipe3 = recipe!({"pkg": "lowest/3.0.0"});
    remote_repo.publish_recipe(&recipe3).await.unwrap();

    let spec1 = spec!({"pkg": "parent/1.0.0/BGSHW3CN",
                       "install": { "requirements": [{ "pkg": "middle/1.0.0"}]}});
    remote_repo
        .publish_package(
            &spec1,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();
    let spec2 = spec!({"pkg": "middle/2.0.0/BGSHW3CN",
                       "install": { "requirements": [{ "pkg": "lowest/3.0.0"}] }});
    remote_repo
        .publish_package(
            &spec2,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    let spec3 = spec!({"pkg": "lowest/3.0.0/BGSHW3CN"});
    remote_repo
        .publish_package(
            &spec3,
            &vec![(Component::Run, empty_layer_digest())]
                .into_iter()
                .collect(),
        )
        .await
        .unwrap();

    remote_repo
}

/// Test the position args function
#[rstest]
fn test_inventory_get_positional_args() {
    let expected: Vec<String> = vec!["python".to_string()];

    let opt = Opt::try_parse_from(["inventory", "python"]).unwrap();
    let result = opt.inventory.get_positional_args();

    assert_eq!(result, expected)
}

/// Test the position args function, for coverage
#[rstest]
fn test_inventory_get_positional_args_when_no_package() {
    let expected: Vec<String> = Vec::new();

    let opt = Opt::try_parse_from(["inventory"]).unwrap();
    let result = opt.inventory.get_positional_args();

    assert_eq!(result, expected)
}

/// `spk inventory package` is expected to focus on that package and
/// show its dependencies at each depth, all its dependencies, and all
/// its callers.
///
#[tokio::test]
async fn test_inventory_a_package() {
    let mut rt = spfs_runtime().await;
    let remote_repo = three_level_deps_repo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let expected_output = [
        "Focusing on: parent",
        "\n\nDEPTH 0\n------------------------------",
        "lowest (clients=1, xdeps=0) deps = []",
        "\n\nDEPTH 1\n------------------------------",
        "middle (clients=1, xdeps=0) deps = [lowest]",
        "\nAll transitive dependencies of parent:",
        "--------------------------------------",
        "  - lowest  (depth: 0)",
        "  - middle  (depth: 1)",
        "\nAll packages that use parent:",
        "-----------------------------",
    ];

    // Test - spk inventory my-pkg
    let mut opt = Opt::try_parse_from(["inventory", "parent"]).unwrap();
    let result = opt.inventory.run().await;

    assert!(
        result.is_ok(),
        "'{result:?}': inventory run should be Ok(_) not an error.')"
    );

    println!("Captured output:\n{:?}", opt.inventory.output.vec);
    println!("Captured errors:\n{:?}", opt.inventory.output.warnings);

    assert!(!opt.inventory.output.vec.is_empty());
    assert!(opt.inventory.output.vec == expected_output);
}

/// `spk inventory package` is expected to focus on that package and
/// show its dependencies at each depth, all its dependencies, and all
/// its callers.
///
#[tokio::test]
async fn test_inventory_middle_package() {
    let mut rt = spfs_runtime().await;
    let remote_repo = three_level_deps_repo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let expected_output = [
        "Focusing on: middle",
        "\n\nDEPTH 0\n------------------------------",
        "lowest (clients=1, xdeps=0) deps = []",
        "\nAll transitive dependencies of middle:",
        "--------------------------------------",
        "  - lowest  (depth: 0)",
        "\nAll packages that use middle:",
        "-----------------------------",
        "  - parent  (dir) (depth: 2)",
    ];

    // Test - spk inventory my-pkg
    let mut opt = Opt::try_parse_from(["inventory", "middle"]).unwrap();
    let result = opt.inventory.run().await;

    assert!(
        result.is_ok(),
        "'{result:?}': inventory run should be Ok(_) not an error.')"
    );

    println!("Captured output:\n{:?}", opt.inventory.output.vec);
    println!("Captured errors:\n{:?}", opt.inventory.output.warnings);

    assert!(!opt.inventory.output.vec.is_empty());
    assert!(opt.inventory.output.vec == expected_output);
}

#[tokio::test]
async fn test_inventory_a_package_yaml_format() {
    let mut rt = spfs_runtime().await;
    let remote_repo = three_level_deps_repo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let expected_output = [
        "depths:\n- - name: lowest\n    depth: 1\n    direct_deps: []\n    num_clients: 1\n    num_all_deps: 0\n",
        "dependencies:\n- name: lowest\n  depth: 1\n  direct: false\n- name: middle\n  depth: 2\n  direct: true\n",
        "used_by: []\n",
    ];

    // Test - spk inventory my-pkg -f yaml
    let mut opt = Opt::try_parse_from(["inventory", "-f", "yaml", "parent"]).unwrap();
    let result = opt.inventory.run().await;

    assert!(
        result.is_ok(),
        "'{result:?}': inventory run should be Ok(_) not an error.')"
    );

    println!("Captured output:\n{:?}", opt.inventory.output.vec);
    println!("Captured errors:\n{:?}", opt.inventory.output.warnings);

    assert!(!opt.inventory.output.vec.is_empty());
    assert!(opt.inventory.output.vec == expected_output);
}

#[tokio::test]
async fn test_inventory_a_package_json_format() {
    let mut rt = spfs_runtime().await;
    let remote_repo = three_level_deps_repo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let expected_output = [
        "{\"depths\":[]}",
        "{\"dependencies\":[{\"name\":\"lowest\",\"depth\":1,\"direct\":true}]}",
        "{\"used_by\":[{\"name\":\"parent\",\"depth\":3,\"direct\":true}]}",
    ];

    // Test - spk inventory my-pkg -f yaml
    let mut opt = Opt::try_parse_from(["inventory", "-f", "json", "middle"]).unwrap();
    let result = opt.inventory.run().await;

    assert!(
        result.is_ok(),
        "'{result:?}': inventory run should be Ok(_) not an error.')"
    );

    println!("Captured output:\n{:?}", opt.inventory.output.vec);
    println!("Captured errors:\n{:?}", opt.inventory.output.warnings);

    assert!(!opt.inventory.output.vec.is_empty());
    assert!(opt.inventory.output.vec == expected_output);
}

/// `spk inventory` is expected to show all the packages are each
/// depth.
#[tokio::test]
async fn test_inventory_all() {
    let mut rt = spfs_runtime().await;
    let remote_repo = three_level_deps_repo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let expected_output = [
        "\n\nDEPTH 0\n------------------------------",
        "lowest (clients=1, xdeps=0) deps = []",
        "\n\nDEPTH 1\n------------------------------",
        "middle (clients=1, xdeps=0) deps = [lowest]",
        "\n\nDEPTH 2\n------------------------------",
        "parent (clients=0, xdeps=1) deps = [middle]",
    ];

    // Test - spk inventory
    let mut opt = Opt::try_parse_from(["inventory"]).unwrap();
    let result = opt.inventory.run().await;

    assert!(
        result.is_ok(),
        "'{result:?}': inventory run should be Ok(_) not an error.')"
    );

    println!("Captured output:\n{:?}", opt.inventory.output.vec);
    println!("Captured errors:\n{:?}", opt.inventory.output.warnings);

    assert!(!opt.inventory.output.vec.is_empty());
    assert!(opt.inventory.output.vec == expected_output);
}

#[tokio::test]
async fn test_inventory_all_yaml_format() {
    let mut rt = spfs_runtime().await;
    let remote_repo = three_level_deps_repo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let expected_output = [
        "depths:\n- - name: lowest\n    depth: 1\n    direct_deps: []\n    num_clients: 1\n    num_all_deps: 0\n- - name: middle\n    depth: 2\n    direct_deps:\n    - lowest\n    num_clients: 1\n    num_all_deps: 1\n",
    ];

    // Test - spk inventory -f yaml
    let mut opt = Opt::try_parse_from(["inventory", "-f", "yaml"]).unwrap();
    let result = opt.inventory.run().await;

    assert!(
        result.is_ok(),
        "'{result:?}': inventory run should be Ok(_) not an error.')"
    );

    println!("Captured output:\n{:?}", opt.inventory.output.vec);
    println!("Captured errors:\n{:?}", opt.inventory.output.warnings);

    assert!(!opt.inventory.output.vec.is_empty());
    assert!(opt.inventory.output.vec == expected_output);
}

#[tokio::test]
async fn test_inventory_all_json_format() {
    let mut rt = spfs_runtime().await;
    let remote_repo = three_level_deps_repo().await;

    rt.add_remote_repo(
        "origin",
        Remote::Address(RemoteAddress {
            address: remote_repo.address().clone(),
        }),
    )
    .unwrap();

    let expected_output = [
        "{\"depths\":[[{\"name\":\"lowest\",\"depth\":1,\"direct_deps\":[],\"num_clients\":1,\"num_all_deps\":0}],[{\"name\":\"middle\",\"depth\":2,\"direct_deps\":[\"lowest\"],\"num_clients\":1,\"num_all_deps\":1}]]}",
    ];

    // Test - spk inventory -f yaml
    let mut opt = Opt::try_parse_from(["inventory", "-f", "json"]).unwrap();
    let result = opt.inventory.run().await;

    assert!(
        result.is_ok(),
        "'{result:?}': inventory run should be Ok(_) not an error.')"
    );

    println!("Captured output:\n{:?}", opt.inventory.output.vec);
    println!("Captured errors:\n{:?}", opt.inventory.output.warnings);

    assert!(!opt.inventory.output.vec.is_empty());
    assert!(opt.inventory.output.vec == expected_output);
}

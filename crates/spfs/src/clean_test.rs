use rstest::rstest;

use super::{
    clean_untagged_objects, get_all_attached_objects, get_all_unattached_objects,
    get_all_unattached_payloads,
};

use crate::encoding::Encodable;
use crate::{graph, storage, tracking, Error};
use std::collections::HashSet;
use storage::prelude::*;

fixtures!();

#[rstest]
fn test_get_attached_objects(tmprepo: TempRepo) {
    let (_td, mut tmprepo) = tmprepo;
    let mut reader = "hello, world".as_bytes();
    let (payload_digest, _) = tmprepo.write_data(Box::new(&mut reader)).unwrap();
    let blob = graph::Blob::new(payload_digest, 0);
    tmprepo.write_blob(blob).unwrap();

    assert_eq!(
        get_all_attached_objects(&tmprepo).unwrap(),
        Default::default(),
        "single blob should not be attached"
    );
    let mut expected = HashSet::new();
    expected.insert(payload_digest);
    assert_eq!(
        get_all_unattached_objects(&tmprepo).unwrap(),
        expected,
        "single blob should be unattached"
    );
}

#[rstest]
fn test_get_attached_payloads(tmprepo: TempRepo) {
    let (_td, mut tmprepo) = tmprepo;
    let mut reader = "hello, world".as_bytes();
    let (payload_digest, _) = tmprepo.write_data(Box::new(&mut reader)).unwrap();
    let mut expected = HashSet::new();
    expected.insert(payload_digest);
    assert_eq!(
        get_all_unattached_payloads(&tmprepo).unwrap(),
        expected,
        "single payload should be attached when no blob"
    );

    let blob = graph::Blob::new(payload_digest, 0);
    tmprepo.write_blob(blob).unwrap();

    assert_eq!(
        get_all_unattached_payloads(&tmprepo).unwrap(),
        Default::default(),
        "single payload should be attached to blob"
    );
}

#[rstest]
fn test_get_attached_unattached_objects_blob(tmprepo: TempRepo) {
    let _guard = init_logging();
    let (tmpdir, mut tmprepo) = tmprepo;
    let data_dir = tmpdir.path().join("data");
    ensure(data_dir.join("file.txt"), "hello, world");

    let manifest = tmprepo.commit_dir(data_dir.as_path()).unwrap();
    let layer = tmprepo
        .create_layer(&graph::Manifest::from(&manifest))
        .unwrap();
    let tag = tracking::TagSpec::parse("my_tag").unwrap();
    tmprepo.push_tag(&tag, &layer.digest().unwrap()).unwrap();
    let blob_digest = manifest
        .root()
        .entries
        .get("file.txt")
        .expect("file should exist in committed manifest")
        .object;

    assert!(
        get_all_attached_objects(&tmprepo)
            .unwrap()
            .contains(&blob_digest),
        "blob in manifest in tag should be attached"
    );
    assert!(
        !get_all_unattached_objects(&tmprepo)
            .unwrap()
            .contains(&blob_digest),
        "blob in manifest in tag should not be unattached"
    );
}

#[rstest]
fn test_clean_untagged_objects(tmprepo: TempRepo) {
    let _guard = init_logging();

    let (tmpdir, mut tmprepo) = tmprepo;
    let data_dir_1 = tmpdir.path().join("data");
    ensure(data_dir_1.join("dir/dir/test.file"), "1 hello");
    ensure(data_dir_1.join("dir/dir/test.file2"), "1 hello, world");
    ensure(data_dir_1.join("dir/dir/test.file4"), "1 hello, world");
    ensure(data_dir_1.join("dir/dir/test.file4"), "1 hello, other");
    ensure(data_dir_1.join("dir/dir/test.file4"), "1 cleanme");
    let data_dir_2 = tmpdir.path().join("data2");
    ensure(data_dir_2.join("dir/dir/test.file"), "2 hello");
    ensure(data_dir_2.join("dir/dir/test.file2"), "2 hello, world");

    let manifest1 = tmprepo.commit_dir(data_dir_1.as_path()).unwrap();

    let manifest2 = tmprepo.commit_dir(data_dir_2.as_path()).unwrap();
    let layer = tmprepo
        .create_layer(&graph::Manifest::from(&manifest2))
        .unwrap();
    let tag = tracking::TagSpec::parse("tagged_manifest").unwrap();
    tmprepo.push_tag(&tag, &layer.digest().unwrap()).unwrap();

    clean_untagged_objects(&tmprepo).expect("failed to clean objects");

    for node in manifest1.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        let res = tmprepo.open_payload(&node.entry.object);
        if let Err(Error::UnknownObject(_)) = res {
            continue;
        }
        if let Err(err) = res {
            println!("{:?}", err);
        }
        panic!(
            "expected object to be cleaned but it was not: {:?}",
            node.entry.object
        );
    }

    for node in manifest2.walk() {
        if !node.entry.kind.is_blob() {
            continue;
        }
        tmprepo
            .open_payload(&node.entry.object)
            .expect("expected payload not to be cleaned");
    }
}

#[rstest]
fn test_clean_untagged_objects_layers_platforms(tmprepo: TempRepo) {
    let (_td, mut tmprepo) = tmprepo;
    let manifest = tracking::Manifest::default();
    let layer = tmprepo
        .create_layer(&graph::Manifest::from(&manifest))
        .unwrap();
    let platform = tmprepo
        .create_platform(vec![layer.digest().unwrap()])
        .unwrap();

    clean_untagged_objects(&tmprepo).expect("failed to clean objects");

    if let Err(Error::UnknownObject(_)) = tmprepo.read_layer(&layer.digest().unwrap()) {
        // ok
    } else {
        panic!("expected layer to be cleaned")
    }

    if let Err(Error::UnknownObject(_)) = tmprepo.read_platform(&platform.digest().unwrap()) {
        // ok
    } else {
        panic!("expected platform to be cleaned")
    }
}

#[rstest]
fn test_clean_manifest_renders(tmprepo: TempRepo) {
    let (tmpdir, tmprepo) = tmprepo;
    let mut tmprepo = match tmprepo {
        storage::RepositoryHandle::FS(repo) => repo,
        _ => {
            println!("Unsupported repo for this test");
            return;
        }
    };

    let data_dir = tmpdir.path().join("data");
    ensure(data_dir.join("dir/dir/file.txt"), "hello");
    ensure(data_dir.join("dir/name.txt"), "john doe");

    let manifest = tmprepo.commit_dir(data_dir.as_path()).unwrap();
    let layer = tmprepo
        .create_layer(&graph::Manifest::from(&manifest))
        .unwrap();
    let _platform = tmprepo
        .create_platform(vec![layer.digest().unwrap()])
        .unwrap();

    tmprepo
        .render_manifest(&graph::Manifest::from(&manifest))
        .unwrap();

    let files = list_files(tmprepo.objects.root());
    assert!(files.len() != 0, "should have stored data");

    clean_untagged_objects(&tmprepo.clone().into()).expect("failed to clean repo");

    let files = list_files(tmprepo.renders.unwrap().root());
    assert!(files.len() == 0, "should remove all created data files");
}

fn list_files<P: AsRef<std::path::Path>>(dirname: P) -> Vec<String> {
    let mut all_files = Vec::new();

    for entry in walkdir::WalkDir::new(dirname) {
        let entry = entry.expect("error while listing dir recursively");
        if entry.metadata().unwrap().is_dir() {
            continue;
        }
        all_files.push(entry.path().to_owned().to_string_lossy().to_string())
    }
    return all_files;
}

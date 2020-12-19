use rstest::{fixture, rstest};

use super::fs::FSDatabase;
use crate::graph::Manifest;
use crate::{encoding::Encodable, tracking};

#[fixture]
fn tmpdir() -> tempdir::TempDir {
    tempdir::TempDir::new("spfs-storage-").expect("failed to create dir for test")
}

#[rstest]
fn test_read_write_manifest(tmpdir: tempdir::TempDir) {
    let tmpdir = tmpdir.path();
    let storage = FSDatabase::new(tmpdir.join("storage"));

    std::fs::File::open(tmpdir.join("file.txt")).unwrap();
    let manifest = Manifest::from(&tracking::compute_manifest(&tmpdir).unwrap());
    storage
        .db
        .write_object(manifest)
        .expect("failed to write manifest");

    std::fs::write(tmpdir.join("file.txt"), "newrootdata").unwrap();
    let manifest2 = Manifest::from(&tracking::compute_manifest(tmpdir).unwrap());
    storage.db.write_object(manifest).unwrap();

    let digests: Vec<_> = storage.db.iter_digests().collect().unwrap();
    assert!(digests.contains(&manifest.digest().unwrap()));
}

#[rstest]
fn test_manifest_parity(tmpdir: tempdir::TempDir) {
    let tmpdir = tmpdir.path();
    let storage = FSDatabase::new(tmpdir.join("storage"));

    std::fs::write(tmpdir.join("dir/file.txt"), "").unwrap();
    let expected = tracking::compute_manifest(&tmpdir).unwrap();
    let storable = Manifest::from(&expected);
    storage.write_object(storable).unwrap();
    let out = storage.read_manifest(storable.digest()).unwrap();
    let actual = out.unlock();
    let mut diffs = tracking::compute_diff(&expected, actual);
    diffs = diffs
        .into_iter()
        .filter(|d| !d.mode.is_unchanged())
        .collect();

    for diff in diffs {
        println!("{}, {:?}", diff, diff.entries);
    }
    assert!(diffs.len() == 0, "Should read out the way it went in");
}

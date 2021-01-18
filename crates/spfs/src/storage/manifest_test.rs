use rstest::{fixture, rstest};

use crate::graph::{Database, DatabaseView, Manifest, Object};
use crate::storage::{fs::FSRepository, ManifestStorage};
use crate::{encoding::Encodable, tracking};

#[fixture]
fn tmpdir() -> tempdir::TempDir {
    tempdir::TempDir::new("spfs-storage-").expect("failed to create dir for test")
}

#[rstest]
#[tokio::test]
async fn test_read_write_manifest(tmpdir: tempdir::TempDir) {
    let tmpdir = tmpdir.path();
    let repo = FSRepository::create(tmpdir.join("repo")).unwrap();

    std::fs::File::open(tmpdir.join("file.txt")).unwrap();
    let manifest = Manifest::from(&tracking::compute_manifest(&tmpdir).unwrap());
    let expected = manifest.digest().unwrap();
    repo.write_object(&manifest.into())
        .expect("failed to write manifest");

    std::fs::write(tmpdir.join("file.txt"), "newrootdata").unwrap();
    let manifest2 = Manifest::from(&tracking::compute_manifest(tmpdir).unwrap());
    repo.write_object(&manifest2.into()).unwrap();

    let digests: crate::Result<Vec<_>> = repo.iter_digests().collect();
    let digests = digests.unwrap();
    assert!(digests.contains(&expected));
}

#[rstest]
#[tokio::test]
async fn test_manifest_parity(tmpdir: tempdir::TempDir) {
    let tmpdir = tmpdir.path();
    let storage = FSRepository::create(tmpdir.join("storage")).unwrap();

    std::fs::write(tmpdir.join("dir/file.txt"), "").unwrap();
    let expected = tracking::compute_manifest(&tmpdir).unwrap();
    let storable = Manifest::from(&expected);
    storage.write_object(&storable.into()).unwrap();
    let out = storage.read_manifest(&storable.digest().unwrap()).unwrap();
    let actual = out.unlock();
    let mut diffs = tracking::compute_diff(&expected, &actual);
    let diffs = diffs
        .into_iter()
        .filter(|d| !d.mode.is_unchanged())
        .collect();

    for diff in diffs {
        println!("{}, {:?}", diff, diff.entries);
    }
    assert!(diffs.len() == 0, "Should read out the way it went in");
}

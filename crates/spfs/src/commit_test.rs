use rstest::rstest;

use super::{commit_layer, commit_platform};
use crate::{runtime, Error};

// #[test]
fn test_commit_empty() {
    let tmpdir = tempdir::TempDir::new("spts-test").unwrap();

    let mut rt = runtime::Runtime::new(tmpdir.path()).unwrap();
    if let Err(Error::NothingToCommit(_)) = commit_layer(&mut rt) {
        // ok
    } else {
        panic!("expected nothing to commit")
    }

    if let Err(Error::NothingToCommit(_)) = commit_platform(&mut rt) {
        // ok
    } else {
        panic!("expected nothing to commit")
    }
}

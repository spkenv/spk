use rstest::fixture;
use tempfile::TempDir;

#[fixture]
pub fn tmpdir() -> TempDir {
    tempfile::Builder::new()
        .prefix("spfs-test-")
        .tempdir()
        .expect("failed to create dir for test")
}

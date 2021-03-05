use super::validate_build_changeset;

#[test]
fn test_validate_build_changeset_nothing() {
    let res = validate_build_changeset(vec![], "/spfs");
    assert!(res.is_err())
}

#[test]
fn test_validate_build_changeset_modified() {
    let res = validate_build_changeset(
        vec![spfs::tracking::Diff {
            path: "/spfs/file.txt".into(),
            mode: spfs::tracking::DiffMode::Changed,
            entries: None,
        }],
        "/spfs",
    );
    assert!(res.is_err())
}

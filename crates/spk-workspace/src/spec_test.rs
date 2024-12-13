use rstest::{fixture, rstest};

use super::Workspace;

#[fixture]
pub fn tmpdir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("spk-test-")
        .tempdir()
        .expect("create a temp directory for test files")
}

const EMPTY_WORKSPACE: &str = r#"
api: v0/workspace
recipes: []
"#;

#[rstest]
fn test_workspace_roundtrip() {
    let workspace = Workspace {
        recipes: vec![
            glob::Pattern::new("packages/*/*.spk.yml").unwrap(),
            glob::Pattern::new("platforms/*/*.spk.yml").unwrap(),
        ],
    };

    let serialized = serde_json::to_string(&workspace).unwrap();
    let deserialized: Workspace = serde_json::from_str(&serialized).unwrap();

    assert_eq!(workspace, deserialized);
}

#[rstest]
fn test_empty_workspace_loading(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path();
    std::fs::write(root.join(Workspace::FILE_NAME), EMPTY_WORKSPACE).unwrap();
    let _workspace = Workspace::load(root).expect("failed to load empty workspace");
}

#[rstest]
fn test_must_have_file(tmpdir: tempfile::TempDir) {
    let root = tmpdir.path();
    Workspace::load(root).expect_err("workspace should fail to load for empty dir");
}

#[rstest]
#[case("", "")]
#[case("my-workspace", "my-workspace/src/packages")]
#[should_panic]
#[case("my-workspace", "other-dir")]
#[case("", "")]
#[case("my-workspace", "my-workspace/src/packages")]
#[should_panic]
#[case("my-workspace", "other-dir/src/packages")]
fn test_workspace_discovery(
    tmpdir: tempfile::TempDir,
    #[case] workspace_root: &str,
    #[case] discovery_start: &str,
) {
    let dir = tmpdir.path();
    let root = dir.join(workspace_root);
    let cwd = dir.join(discovery_start);

    std::fs::create_dir_all(&cwd).unwrap();
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(Workspace::FILE_NAME), EMPTY_WORKSPACE).unwrap();

    Workspace::discover(&cwd).expect("failed to load workspace");
}

use rstest::rstest;

use super::Workspace;

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

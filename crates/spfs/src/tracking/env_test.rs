use rstest::rstest;

use super::EnvSpec;

#[rstest]
#[tokio::test]
async fn test_env_spec_validation() {
    let spec = EnvSpec::new("one+two").expect("failed to parse env spec");
    assert_eq!(spec.items.len(), 2);
}

#[rstest]
#[tokio::test]
async fn test_env_spec_empty() {
    EnvSpec::new("").expect_err("empty spec should be invalid");
}

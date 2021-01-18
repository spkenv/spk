use rstest::rstest;

use super::EnvSpec;

// #[test]
fn test_env_spec_validation() {
    let spec = EnvSpec::new("one+two").expect("failed to parse env spec");
    assert_eq!(spec.items.len(), 2);
}

// #[test]
fn test_env_spec_empty() {
    EnvSpec::new("").expect_err("empty spec should be invalid");
}

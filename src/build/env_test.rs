use rstest::rstest;

#[rstest]
#[case("$NOTHING", "$NOTHING")]
#[case("NOTHING", "NOTHING")]
#[case("$NOTHING:$SOMETHING", "$NOTHING:something")]
#[case("$SOMETHING:$NOTHING", "something:$NOTHING")]
#[case("${SOMETHING}$NOTHING", "something$NOTHING")]
fn test_expand_defined_args(#[case] value: String, #[case] expected: &str) {
    let get_var = |s: &str| match s {
        "SOMETHING" => Some("something".to_string()),
        _ => None,
    };
    assert_eq!(super::expand_defined_vars(value, get_var), expected)
}

#[rstest]
#[should_panic]
#[case("$NOTHING", "")]
#[case("NOTHING", "NOTHING")]
#[should_panic]
#[case("$NOTHING:$SOMETHING", "")]
#[case("$SOMETHING:other", "something:other")]
#[case("other${SOMETHING}other", "othersomethingother")]
fn test_expand_vars(#[case] value: String, #[case] expected: &str) {
    let get_var = |s: &str| match s {
        "SOMETHING" => Some("something".to_string()),
        _ => None,
    };
    assert_eq!(super::expand_vars(value, get_var).unwrap(), expected)
}

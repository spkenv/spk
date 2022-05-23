use rstest::rstest;

#[rstest]
#[case("lowercase")]
#[case("with-dashes")]
#[case("num000")]
#[case("000-000")]
#[case("-----")]
#[should_panic]
#[case("upperCase")] // no upper case
#[should_panic]
#[case("has_dashes")] // no underscores
#[should_panic]
#[case("name!!")] // no special characters
fn test_name_validation(#[case] input: &str) {
    super::validate_pkg_name(input).unwrap();
}

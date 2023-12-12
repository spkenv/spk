use rstest::rstest;

#[rstest]
#[case("lowercase")]
#[case("with-dashes")]
#[case("num000")]
#[case("000-000")]
#[should_panic]
#[case("-----")]
#[should_panic]
#[case("upperCase")] // no upper case
#[should_panic]
#[case("has_dashes")] // no underscores
#[should_panic]
#[case("name!!")] // no special characters
fn test_pkg_validation(#[case] input: &str) {
    super::validate_pkg_name(input).unwrap();
    super::validate_opt_name(input).expect("all valid package names should be valid option names");
}

#[rstest]
#[case("lowercase")]
#[case("with-dashes")]
#[case("with_underscores")]
#[case("num000")]
#[case("000-000")]
#[case("000_000")]
#[case("-----")]
#[case("_____")]
#[should_panic]
#[case("upperCase")] // no upper case
#[should_panic]
#[case("name!!")] // no special characters
fn test_opt_name_validation(#[case] input: &str) {
    super::validate_opt_name(input).unwrap();
}

#[rstest]
#[case("lowercase", "lowercase")]
#[case("with-dashes", "with-dashes")]
#[case("with_underscores", "with_underscores")]
#[case("num000", "num000")]
#[case("000-000", "000-000")]
#[case("000_000", "000_000")]
#[case("-----", "-----")]
#[case("_____", "_____")]
#[case("upperCase", "uppercase")]
#[case("name!!", "name")]
#[case("a", "a_")]
fn test_opt_name_buf_new_lossy(#[case] input: &str, #[case] result: &str) {
    let opt_name = super::OptNameBuf::new_lossy(&String::from(input));
    assert_eq!(opt_name.base_name(), result);
}

#[rstest]
#[case("my_opt", None, "my_opt")]
#[case("my-pkg.my_opt", Some("my-pkg"), "my_opt")]
fn test_opt_name_namespace(#[case] input: &str, #[case] ns: Option<&str>, #[case] name: &str) {
    let full = super::OptName::new(input).expect("invalid option name");
    let ns = ns.map(|ns| super::PkgName::new(ns).unwrap());
    assert_eq!(full.namespace(), ns);
    assert_eq!(full.base_name(), name);
}

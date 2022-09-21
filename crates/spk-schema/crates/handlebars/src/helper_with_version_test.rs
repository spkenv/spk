use rstest::rstest;
use serde_json::json;

#[rstest]
fn test_with_version_access_basic() {
    let options = json!({});
    static TPL: &str = r#"
{{#with-version "1.2.3.4.5-beta.1+r.0"}}
{{base}}
{{major}}
{{minor}}
{{patch}}
{{parts.[3]}}
{{parts.[4]}}
{{post.r}}
{{pre.beta}}
{{/with-version}}"#;
    static EXPECTED: &str = r#"
1.2.3.4.5
1
2
3
4
5
0
1
"#;
    let rendered = crate::render_template("mypackage.spk.yaml", TPL, &options)
        .expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}

#[rstest]
fn test_with_version_access_block_params() {
    let options = json!({"version": "1.2.3.4.5-beta.1+r.0"});
    static TPL: &str = r#"
{{#with-version version as |v|}}
{{version}}
{{v.base}}
{{v.major}}
{{v.minor}}
{{v.patch}}
{{v.parts.[3]}}
{{v.parts.[4]}}
{{v.post.r}}
{{v.pre.beta}}
{{/with-version}}"#;
    static EXPECTED: &str = r#"
1.2.3.4.5-beta.1+r.0
1.2.3.4.5
1
2
3
4
5
0
1
"#;
    let rendered = crate::render_template("mypackage.spk.yaml", TPL, &options)
        .expect("template should not fail to render");
    assert_eq!(rendered, EXPECTED);
}

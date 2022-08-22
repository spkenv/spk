// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rstest::rstest;

use super::{split_tag_spec, Tag, TagSpec};
use crate::{
    encoding,
    encoding::{Decodable, Encodable},
};

#[rstest(org, name, case("vfx", "2019"))]
fn test_tag_encoding(org: &str, name: &str) {
    let tag = Tag::new(
        Some(org.to_string()),
        name.to_string(),
        encoding::NULL_DIGEST.into(),
    )
    .expect("invalid case");
    let mut writer = Vec::new();
    tag.encode(&mut writer).expect("failed to encode tag");
    let mut reader = std::io::BufReader::new(writer.as_slice());
    let decoded = Tag::decode(&mut reader).expect("failed to decode tag");
    assert_eq!(tag, decoded);
}

#[rstest(raw, expected,
    case("vfx2019", (None, "vfx2019", 0)),
    case("spi/base", (Some("spi"), "base", 0)),
    case("spi/base~4", (Some("spi"), "base", 4)),
    case(
        "gitlab.spimageworks.com/spfs/spi/base",
        (Some("gitlab.spimageworks.com/spfs/spi"), "base", 0),
    ),
)]
fn test_tag_spec_split(raw: &str, expected: (Option<&str>, &str, u64)) {
    let actual = split_tag_spec(raw).expect("failed to split tag");
    assert_eq!(actual.org(), expected.0.map(|o| o.to_string()));
    assert_eq!(actual.name(), expected.1.to_string());
    assert_eq!(actual.version(), expected.2);
}

#[rstest]
fn test_tag_spec_class() {
    let src = "org/name~1";
    let spec = TagSpec::parse(src).expect("failed to create tag");
    assert_eq!(format!("{spec}"), src.to_string());
    assert_eq!(spec.org(), Some("org".to_string()));
    assert_eq!(spec.name(), "name");
    assert_eq!(spec.version(), 1);
}

#[rstest]
fn test_tag_spec_path() {
    let spec = TagSpec::parse("one_part").expect("failed to parse tag");
    assert_eq!(spec.path(), "one_part");

    let spec = TagSpec::parse("two/parts").expect("failed to parse tag");
    assert_eq!(spec.path(), "two/parts");
}

#[rstest]
fn test_tag_spec_validation() {
    TagSpec::parse("").expect_err("should fail when empty");
    TagSpec::parse("name~-1").expect_err("should fail with negative version");
    TagSpec::parse("name~1.23").expect_err("should fail with float");
}

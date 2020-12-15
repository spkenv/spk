use rstest::rstest;

use super::Platform;
use crate::encoding;
use crate::encoding::Encodable;

#[rstest]
fn test_platform_encoding() {
    let layers: Vec<encoding::Digest> =
        vec![encoding::EMPTY_DIGEST.into(), encoding::NULL_DIGEST.into()];
    let expected = Platform::new(layers).unwrap();

    let mut stream = Vec::new();
    expected.encode(&mut stream).unwrap();
    let actual = Platform::decode(&mut stream.as_slice()).unwrap();
    assert_eq!(actual, expected);
}

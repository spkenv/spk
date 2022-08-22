// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use rstest::rstest;

use super::FileMatcher;

#[rstest]
#[case(&[], "/file.txt", false)]
#[case(&["/file.txt"], "/file.txt", true)]
#[case(&["*.txt"], "/data/file.txt", true)]
#[case(&["file.txt/"], "/data/file.txt", false)]
fn test_file_matcher_matching(
    #[case] patterns: &[&str],
    #[case] path: &str,
    #[case] should_match: bool,
) {
    // we're not really testing gitignore here, just that the
    // semantics of our function works as expected
    let matcher = FileMatcher::new(patterns.iter().map(|s| s.to_string())).unwrap();
    assert_eq!(matcher.matches(path, path.ends_with('/')), should_match);
}

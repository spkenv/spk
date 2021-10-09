// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

/// Return 'name' if it's a validate package name
pub fn validate_name<S: AsRef<str>>(name: S) -> crate::Result<()> {
    let index = validate_source_str(&name, |c: char| c.is_ascii_alphanumeric() || c == '-');
    if index > -1 {
        let name = name.as_ref();
        let index = index as usize;
        let err_str = format!(
            "{} > {} < {}",
            &name[..index],
            name.chars().nth(index).unwrap(),
            &name[(index + 1)..]
        );
        Err(crate::Error::InvalidPackageName(format!(
            "Invalid package name at pos {}: {}",
            index, err_str
        )))
    } else {
        Ok(())
    }
}

/// Return 'name' if it's a validate pre/post release tag name
pub fn validate_tag_name<S: AsRef<str>>(name: S) -> crate::Result<()> {
    let index = validate_source_str(&name, |c: char| c.is_ascii_alphanumeric());
    if index > -1 {
        let name = name.as_ref();
        let index = index as usize;
        let err_str = format!(
            "{} > {} < {}",
            &name[..index],
            name.chars().nth(index).unwrap(),
            &name[(index + 1)..]
        );
        Err(crate::Error::InvalidVersionTag(format!(
            "Invalid release tag name at pos {}: {}",
            index, err_str
        )))
    } else {
        Ok(())
    }
}

/// Returns -1 if valid, or the index of the invalid character
fn validate_source_str<S, V>(source: S, validator: V) -> isize
where
    S: AsRef<str>,
    V: Fn(char) -> bool,
{
    let source = source.as_ref();
    for (i, c) in source.chars().enumerate() {
        if validator(c) {
            continue;
        }
        return i as isize;
    }
    return -1;
}

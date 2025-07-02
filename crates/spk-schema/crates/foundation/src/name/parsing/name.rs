// Copyright (c) Contributors to the SPK project.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/spkenv/spk

use std::collections::HashSet;

use nom::IResult;
use nom::bytes::complete::{is_not, take_till, take_while_m_n, take_while1};
use nom::character::complete::char;
use nom::combinator::{fail, map, not, peek, recognize, verify};
use nom::error::{ContextError, ParseError};
use nom::multi::many1;

use crate::name::{PkgName, RepositoryName};

#[inline]
pub fn is_legal_package_name_chr(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'
}

#[inline]
pub(crate) fn is_legal_repo_name_chr(c: char) -> bool {
    is_legal_package_name_chr(c)
}

#[inline]
pub(crate) fn is_legal_tag_name_chr(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

#[inline]
pub(crate) fn is_legal_tag_name_alpha_chr(c: char) -> bool {
    // Don't match any numbers
    c.is_ascii_alphabetic()
}

/// Parse a known repository name into a [`RepositoryName`].
///
/// This parser is for recognizing repository names that are
/// present in `known_repositories`.
///
/// See [repository_name] for parsing arbitrary repository
/// names.
pub fn known_repository_name<'a, 'i, E>(
    known_repositories: &'a HashSet<&str>,
) -> impl Fn(&'i str) -> IResult<&'i str, &'i RepositoryName, E> + 'a
where
    E: ParseError<&'i str> + ContextError<&'i str> + 'a,
{
    move |input| {
        let (input, name) = recognize(many1(is_not("/")))(input)?;
        if known_repositories.contains(name) {
            return Ok((
                input,
                // Safety: A known repository is assumed to be a valid name.
                unsafe { RepositoryName::from_str(name) },
            ));
        }
        fail("not a known repository")
    }
}

/// Parse a package name.
///
/// Examples:
/// - `"pkg1"`
/// - `"pkg-name"`
///
/// A package name must be at least [`PkgName::MIN_LEN`] characters and no
/// more than [`PkgName::MAX_LEN`] characters.
pub fn package_name<'a, E>(input: &'a str) -> IResult<&'a str, &'a PkgName, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    // Package names may not begin with a '-'
    let (input, _) = not(peek(char('-')))(input)?;

    map(
        take_while_m_n(
            PkgName::MIN_LEN,
            PkgName::MAX_LEN,
            is_legal_package_name_chr,
        ),
        |s: &str| {
            // Safety: we only generate valid package names
            unsafe { PkgName::from_str(s) }
        },
    )(input)
}

/// Parse a repository name.
///
/// Examples:
/// - `"repo1"`
/// - `"repo-name"`
pub fn repository_name<'a, E>(input: &'a str) -> IResult<&'a str, &'a RepositoryName, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(take_while1(is_legal_repo_name_chr), |s: &str| {
        // Safety: we only parse valid names.
        unsafe { RepositoryName::from_str(s) }
    })(input)
}

/// Parse a tag name.
///
/// A tag name refers to the string portion of a pre- or post-release on a
/// version. It may not consist of only numbers.
///
/// Examples:
/// - `"r"`
/// - `"alpha1"`
pub fn tag_name<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    verify(take_while1(is_legal_tag_name_chr), |s: &str| {
        // `s` must contain a non-numeric character
        take_till::<_, _, ()>(is_legal_tag_name_alpha_chr)(s)
            .map(|(remaining, _)| !remaining.is_empty())
            .unwrap_or(false)
    })(input)
}

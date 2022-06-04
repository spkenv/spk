// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::{
    bytes::complete::{is_not, take_while1, take_while_m_n},
    combinator::{fail, map, recognize},
    error::{context, ContextError, ParseError},
    multi::many1,
    IResult,
};

use crate::api::{PkgName, RepositoryName};

#[inline]
pub(crate) fn is_legal_package_name_chr(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-'
}

#[inline]
pub(crate) fn is_legal_repo_name_chr(c: char) -> bool {
    is_legal_package_name_chr(c)
}

#[inline]
pub(crate) fn is_legal_tag_name_chr(c: char) -> bool {
    c.is_ascii_alphanumeric()
}

pub(crate) fn known_repository_name<'a, 'i, E>(
    known_repositories: &'a HashSet<&str>,
) -> impl Fn(&'i str) -> IResult<&'i str, RepositoryName, E> + 'a
where
    E: ParseError<&'i str> + ContextError<&'i str> + 'a,
{
    move |input| {
        let (input, name) = recognize(many1(is_not("/")))(input)?;
        if known_repositories.contains(name) {
            return Ok((input, RepositoryName(name.to_owned())));
        }
        fail("not a known repository")
    }
}

pub(crate) fn package_name<'a, E>(input: &'a str) -> IResult<&'a str, &PkgName, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "package_name",
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
        ),
    )(input)
}

pub(crate) fn repository_name<'a, E>(input: &'a str) -> IResult<&'a str, RepositoryName, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    map(take_while1(is_legal_repo_name_chr), |s: &str| {
        RepositoryName(s.to_owned())
    })(input)
}

pub(crate) fn tag_name<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    take_while1(is_legal_tag_name_chr)(input)
}

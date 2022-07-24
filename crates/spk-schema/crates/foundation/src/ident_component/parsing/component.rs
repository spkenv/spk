// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;

use nom::{
    branch::alt,
    bytes::complete::take_while_m_n,
    character::complete::char,
    combinator::{cut, map},
    error::{ContextError, ParseError},
    multi::separated_list1,
    sequence::delimited,
    IResult,
};
use nom_supreme::tag::TagError;

use crate::ident_component::Component;
use crate::name::{parsing::is_legal_package_name_chr, PkgName};

/// Parse a component name into a [`Component`].
///
/// Examples:
/// - `"all"`
/// - `"legal-component-name"`
pub(crate) fn component<'a, E>(input: &'a str) -> IResult<&'a str, Component, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + TagError<&'a str, &'static str>,
{
    map(
        take_while_m_n(
            PkgName::MIN_LEN,
            PkgName::MAX_LEN,
            is_legal_package_name_chr,
        ),
        |s: &str| match s {
            "all" => Component::All,
            "build" => Component::Build,
            "run" => Component::Run,
            "src" => Component::Source,
            s => Component::Named(s.to_owned()),
        },
    )(input)
}

/// Parse a component group expression into a [`BTreeSet<Component>`].
///
/// This may be either a bare component name or a set defined with
/// `{}`.
///
/// Examples:
/// - `"comp-name"`
/// - `"{comp1,comp2}"`
pub fn components<'a, E>(input: &'a str) -> IResult<&'a str, BTreeSet<Component>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + TagError<&'a str, &'static str>,
{
    alt((
        delimited(
            char('{'),
            cut(map(separated_list1(char(','), cut(component)), |comps| {
                comps.into_iter().collect()
            })),
            cut(char('}')),
        ),
        map(component, |comp| BTreeSet::from([comp])),
    ))(input)
}

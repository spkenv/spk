// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::HashSet;

use nom::{
    branch::alt,
    bytes::complete::{tag, take_while_m_n},
    character::complete::char,
    combinator::map,
    error::VerboseError,
    multi::separated_list1,
    sequence::delimited,
    IResult,
};

use crate::api::{Component, PkgName};

use super::name::is_legal_package_name_chr;

pub(crate) fn component(input: &str) -> IResult<&str, Component, VerboseError<&str>> {
    alt((
        map(tag("all"), |_| Component::All),
        map(tag("run"), |_| Component::Run),
        map(tag("build"), |_| Component::Build),
        map(tag("src"), |_| Component::Source),
        map(
            take_while_m_n(
                PkgName::MIN_LEN,
                PkgName::MAX_LEN,
                is_legal_package_name_chr,
            ),
            |s: &str| Component::Named(s.to_owned()),
        ),
    ))(input)
}

pub(crate) fn components(input: &str) -> IResult<&str, HashSet<Component>, VerboseError<&str>> {
    alt((
        delimited(
            char('{'),
            map(separated_list1(char(','), component), |comps| {
                comps.into_iter().collect()
            }),
            char('}'),
        ),
        map(component, |comp| HashSet::from([comp])),
    ))(input)
}

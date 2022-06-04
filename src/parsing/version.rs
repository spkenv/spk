// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use nom::{
    character::complete::{char, digit1},
    combinator::{map_res, opt, recognize},
    error::{context, ContextError, FromExternalError, ParseError},
    multi::separated_list1,
    sequence::{pair, preceded, separated_pair},
    IResult,
};

use crate::api::{parse_version, Version};

use super::name::tag_name;

pub(crate) fn ptag<'a, E>(input: &'a str) -> IResult<&'a str, (&'a str, &'a str), E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    separated_pair(tag_name, char('.'), digit1)(input)
}

pub(crate) fn ptagset<'a, E>(input: &'a str) -> IResult<&'a str, Vec<(&'a str, &'a str)>, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    separated_list1(char(','), ptag)(input)
}

pub(crate) fn version<'a, E>(input: &'a str) -> IResult<&'a str, Version, E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, crate::error::Error>,
{
    map_res(version_str, parse_version)(input)
}

pub(crate) fn version_str<'a, E>(input: &'a str) -> IResult<&'a str, &'a str, E>
where
    E: ParseError<&'a str> + ContextError<&'a str>,
{
    context(
        "version_str",
        recognize(pair(
            separated_list1(char('.'), digit1),
            pair(
                opt(preceded(char('-'), ptagset)),
                opt(preceded(char('+'), ptagset)),
            ),
        )),
    )(input)
}

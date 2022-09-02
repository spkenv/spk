// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::str::FromStr;

use nom::bytes::complete::take_while1;
use nom::character::complete::{char, digit1};
use nom::combinator::{map, map_res, opt};
use nom::error::{ContextError, FromExternalError, ParseError};
use nom::sequence::{pair, preceded, separated_pair};
use nom::IResult;
use nom_supreme::tag::complete::tag;
use nom_supreme::tag::TagError;
use spk_schema::TestStage;

/// Variant specified by its 0-based index into the list of variants of a
/// recipe.
pub struct VariantIndex(pub usize);

/// Parse a package filename with a stage specifier.
///
/// Examples:
/// - "package.spk.yaml@build"
/// - "package.spk.yaml@build?v=1"
pub fn stage_specifier<'a, E>(
    input: &'a str,
) -> IResult<&'a str, (&'a str, TestStage, Option<VariantIndex>), E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, spk_schema::Error>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + TagError<&'a str, &'static str>,
{
    map(
        separated_pair(
            take_while1(|c| c != '@'),
            char('@'),
            pair(
                map_res(take_while1(|c| c != '?'), TestStage::from_str),
                opt(preceded(
                    tag("?v="),
                    map(map_res(digit1, |n: &str| n.parse::<usize>()), VariantIndex),
                )),
            ),
        ),
        |(package, (stage, build_variant))| (package, stage, build_variant),
    )(input)
}

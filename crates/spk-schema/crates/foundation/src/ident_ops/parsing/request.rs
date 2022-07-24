// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;

use crate::ident_component::{parsing::components, Component};
use crate::name::{parsing::package_name, PkgName};
use nom::{
    character::complete::char,
    combinator::{cut, map, opt},
    error::{ContextError, ParseError},
    sequence::{pair, preceded},
    IResult,
};
use nom_supreme::tag::TagError;

/// Parse a package name in the context of a range identity.
///
/// The package name must either be followed by a `/` or the end of input.
///
/// Examples:
/// - `"package-name"`
/// - `"package-name/"`
/// - `"package-name:comp"`
/// - `"package-name:{comp1,comp2}/"`
pub fn range_ident_pkg_name<'a, E>(
    input: &'a str,
) -> IResult<&'a str, (&PkgName, BTreeSet<Component>), E>
where
    E: ParseError<&'a str> + ContextError<&'a str> + TagError<&'a str, &'static str>,
{
    pair(
        package_name,
        map(
            opt(preceded(char(':'), cut(components))),
            |opt_components| opt_components.unwrap_or_default(),
        ),
    )(input)
}

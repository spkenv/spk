// Copyright (c) 2022 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::BTreeSet;

use nom::character::complete::char;
use nom::combinator::{cut, map, opt};
use nom::error::{ContextError, FromExternalError, ParseError};
use nom::sequence::{pair, preceded};
use nom::IResult;
use nom_supreme::tag::TagError;

use crate::ident_component::parsing::components;
use crate::ident_component::Component;
use crate::name::parsing::package_name;
use crate::name::PkgName;
use crate::version::parsing::version;
use crate::version::Version;

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

/// Parse a package name with optional components and optional version.
///
/// The package name must either be followed by a `/` or the end of input.
///
/// Examples:
/// - `"package-name"`
/// - `"package-name/"`
/// - `"package-name/1.0.0"`
/// - `"package-name:comp"`
/// - `"package-name:{comp1,comp2}/"`
/// - `"package-name:{comp1,comp2}/1.0.0"`
pub fn request_pkg_name_and_version<'a, E>(
    input: &'a str,
) -> IResult<&'a str, (&PkgName, BTreeSet<Component>, Option<Version>), E>
where
    E: ParseError<&'a str>
        + ContextError<&'a str>
        + FromExternalError<&'a str, std::num::ParseIntError>
        + FromExternalError<&'a str, crate::version::Error>
        + TagError<&'a str, &'static str>,
{
    map(
        pair(
            package_name,
            pair(
                map(
                    opt(preceded(char(':'), cut(components))),
                    |opt_components| opt_components.unwrap_or_default(),
                ),
                opt(preceded(char('/'), cut(version))),
            ),
        ),
        |(pkg_name, (components, opt_version))| (pkg_name, components, opt_version),
    )(input)
}

// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{collections::HashSet, convert::TryFrom, fmt::Write, str::FromStr};

use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, take_while1, take_while_m_n},
    character::complete::{char, digit1},
    combinator::{eof, fail, map, map_res, opt, peek, recognize},
    error::{context, convert_error, VerboseError},
    multi::{many1, separated_list1},
    sequence::{pair, preceded, separated_pair, terminated},
    IResult,
};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::api::PkgName;

use super::{parse_build, parse_version, Build, PkgNameBuf, Version};

#[cfg(test)]
#[path = "./ident_test.rs"]
mod ident_test;

/// Parse an identifier from a string.
///
/// This will panic if the identifier is wrong,
/// and should only be used for testing.
///
/// ```
/// # #[macro_use] extern crate spk;
/// # fn main() {
/// ident!("my-pkg/1.0.0");
/// # }
/// ```
#[macro_export]
macro_rules! ident {
    ($ident:literal) => {
        $crate::api::parse_ident($ident).unwrap()
    };
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RepositoryName(String);

/// Ident represents a package identifier.
///
/// The identifier is either a specific package or
/// range of package versions/releases depending on the
/// syntax and context
#[derive(Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct Ident {
    repository_name: Option<RepositoryName>,
    pub name: PkgNameBuf,
    pub version: Version,
    pub build: Option<Build>,
}

impl std::fmt::Debug for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Ident").field(&self.to_string()).finish()
    }
}

impl std::fmt::Display for Ident {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.name.as_str())?;
        if let Some(vb) = self.version_and_build() {
            f.write_char('/')?;
            f.write_str(vb.as_str())?;
        }
        Ok(())
    }
}

impl Ident {
    /// Return true if this identifier is for a source package.
    pub fn is_source(&self) -> bool {
        match &self.build {
            Some(build) => build.is_source(),
            None => false,
        }
    }

    /// Return a copy of this identifier with the given version number instead
    pub fn with_version(&self, version: Version) -> Ident {
        Self {
            repository_name: self.repository_name.clone(),
            name: self.name.clone(),
            version,
            build: self.build.clone(),
        }
    }

    /// Set the build component of this package identifier.
    pub fn set_build(&mut self, build: Option<Build>) {
        self.build = build;
    }

    /// Return a copy of this identifier with the given build replaced.
    pub fn with_build(&self, build: Option<Build>) -> Self {
        let mut new = self.clone();
        new.build = build;
        new
    }
}

impl Ident {
    pub fn new(name: PkgNameBuf) -> Self {
        Self {
            repository_name: Default::default(),
            name,
            version: Default::default(),
            build: Default::default(),
        }
    }

    /// A string containing the properly formatted name and version number
    ///
    /// This is the same as [`ToString::to_string`] when the build is None.
    pub fn version_and_build(&self) -> Option<String> {
        match &self.build {
            Some(build) => Some(format!("{}/{}", self.version, build.digest())),
            None => {
                if self.version.is_zero() {
                    None
                } else {
                    Some(self.version.to_string())
                }
            }
        }
    }
}

impl From<PkgNameBuf> for Ident {
    fn from(n: PkgNameBuf) -> Self {
        Self::new(n)
    }
}

impl TryFrom<&str> for Ident {
    type Error = crate::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl TryFrom<&String> for Ident {
    type Error = crate::Error;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<String> for Ident {
    type Error = crate::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

impl FromStr for Ident {
    type Err = crate::Error;

    /// Parse the given identifier string into this instance.
    fn from_str(source: &str) -> crate::Result<Self> {
        // TODO: this list of possible names should come from reading
        // the config file
        let known_repositories: HashSet<&'static str> =
            ["local", "origin"].iter().cloned().collect();

        #[inline]
        fn is_base32_digit(c: char) -> bool {
            ('A'..='Z').contains(&c) || ('2'..='7').contains(&c)
        }

        #[inline]
        fn is_legal_package_name_chr(c: char) -> bool {
            c.is_ascii_alphanumeric() || c == '-'
        }

        #[inline]
        fn is_legal_repo_name_chr(c: char) -> bool {
            is_legal_package_name_chr(c)
        }

        #[inline]
        fn is_legal_tag_name_chr(c: char) -> bool {
            c.is_ascii_alphanumeric()
        }

        fn package_name(input: &str) -> IResult<&str, &PkgName, VerboseError<&str>> {
            context(
                "package_name",
                map(
                    take_while_m_n(
                        PkgName::MIN_LEN,
                        PkgName::MAX_LEN,
                        is_legal_package_name_chr,
                    ),
                    |s: &str| {
                        // Safety: we only generate valid package names.
                        unsafe { PkgName::from_str(s) }
                    },
                ),
            )(input)
        }

        fn package_ident(input: &str) -> IResult<&str, Ident, VerboseError<&str>> {
            terminated(
                map(package_name, |name| Ident::new(name.to_owned())),
                peek(alt((tag("/"), eof))),
            )(input)
        }

        fn package_name_and_not_version(input: &str) -> IResult<&str, Ident, VerboseError<&str>> {
            let (tail, ident) = package_ident(input)?;
            // To disambiguate cases like:
            //    111/222
            // If "222" is a valid version string and is the end of input,
            // return an Error here so that "111" will be treated as the
            // package name instead of as a repository name.
            if terminated(version_str, eof)(input).is_ok() {
                return fail("could be version");
            }
            // To disambiguate cases like:
            //    222/333/44444444
            // If "333" is a valid version string and "44444444" is a
            // valid build string and is the end of input, return an Error
            // here so that "222" will be treated as the package name
            // instead of as a repository name.
            let prefixed = format!("/{}", input);
            if let Ok((_, (_version, Some(_build)))) =
                dbg!(terminated(version_and_build, eof)(dbg!(prefixed.as_str())))
            {
                return fail("could be a build");
            }
            Ok((tail, ident))
        }

        fn known_repository_name<'a>(
            known_repositories: &'a HashSet<&str>,
        ) -> impl Fn(&str) -> IResult<&str, &str, VerboseError<&str>> + 'a {
            move |input| {
                let (input, name) = recognize(many1(is_not("/")))(input)?;
                if known_repositories.contains(name) {
                    return Ok((input, name));
                }
                fail("not a known repository")
            }
        }

        fn repo_name<'a>(
            known_repositories: &'a HashSet<&'a str>,
        ) -> impl Fn(&str) -> IResult<&str, &str, VerboseError<&str>> + 'a {
            move |input| {
                // To disambiguate cases like:
                //    local/222
                // If "local" is a known repository name and "222" is a valid
                // package name and the end of input, treat the first component
                // as a repository name instead of a package name.
                alt((
                    terminated(
                        terminated(known_repository_name(known_repositories), char('/')),
                        peek(terminated(take_while1(is_legal_package_name_chr), eof)),
                    ),
                    terminated(
                        terminated(take_while1(is_legal_repo_name_chr), char('/')),
                        // Reject treating the consumed component as a repository name if the following
                        // components are more likely to mean the consumed component was actually a
                        // package name. This puts more emphasis on interpreting input the same as before
                        // repository names were added.
                        peek(package_name_and_not_version),
                    ),
                ))(input)
            }
        }

        fn tag_name(input: &str) -> IResult<&str, &str, VerboseError<&str>> {
            take_while1(is_legal_tag_name_chr)(input)
        }

        fn ptag(input: &str) -> IResult<&str, (&str, &str), VerboseError<&str>> {
            separated_pair(tag_name, char('.'), digit1)(input)
        }

        fn ptagset(input: &str) -> IResult<&str, Vec<(&str, &str)>, VerboseError<&str>> {
            separated_list1(char(','), ptag)(input)
        }

        fn version_str(input: &str) -> IResult<&str, &str, VerboseError<&str>> {
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

        fn base32_build(input: &str) -> IResult<&str, &str, VerboseError<&str>> {
            take_while_m_n(
                super::option_map::DIGEST_SIZE,
                super::option_map::DIGEST_SIZE,
                is_base32_digit,
            )(input)
        }

        fn build(input: &str) -> IResult<&str, &str, VerboseError<&str>> {
            preceded(
                char('/'),
                alt((
                    tag(super::build::SRC),
                    tag(super::build::EMBEDDED),
                    base32_build,
                )),
            )(input)
        }

        fn version_and_build(
            input: &str,
        ) -> IResult<&str, (Version, Option<Build>), VerboseError<&str>> {
            pair(
                preceded(
                    char('/'),
                    context("parse_version", map_res(version_str, parse_version)),
                ),
                opt(context("parse_build", map_res(build, parse_build))),
            )(input)
        }

        fn ident<'a, 'b>(
            known_repositories: &'a HashSet<&str>,
            input: &'b str,
        ) -> IResult<&'b str, Ident, VerboseError<&'b str>> {
            let (input, repository_name) = opt(repo_name(known_repositories))(input)?;
            let (input, mut ident) = package_ident(input)?;
            if let Some(repository_name) = repository_name {
                ident.repository_name = Some(RepositoryName(repository_name.to_owned()));
            }
            let (input, version_and_build) = opt(version_and_build)(input)?;
            eof(input)?;
            match version_and_build {
                Some(v_and_b) => {
                    ident.version = v_and_b.0;
                    ident.build = v_and_b.1;
                    Ok(("", ident))
                }
                None => Ok(("", ident)),
            }
        }

        ident(&known_repositories, source)
            .map(|(_, ident)| ident)
            .map_err(|err| match err {
                nom::Err::Error(e) | nom::Err::Failure(e) => {
                    crate::Error::String(convert_error(source, e))
                }
                nom::Err::Incomplete(_) => unreachable!(),
            })
    }
}

/// Parse a package identifier string.
pub fn parse_ident<S: AsRef<str>>(source: S) -> crate::Result<Ident> {
    Ident::from_str(source.as_ref())
}

impl Serialize for Ident {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for Ident {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::from_str(&s).map_err(de::Error::custom)
    }
}

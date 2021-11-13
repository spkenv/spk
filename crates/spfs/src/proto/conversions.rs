// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::{TryFrom, TryInto};

use crate::{encoding, tracking, Error, Result};

fn convert_to_datetime(source: Option<super::DateTime>) -> Result<chrono::DateTime<chrono::Utc>> {
    use std::str::FromStr;
    let source =
        source.ok_or_else(|| Error::String("Expected non-null digest in rpc message".into()))?;
    chrono::DateTime::<chrono::Utc>::from_str(&source.iso_timestamp)
        .map_err(|err| Error::String(format!("Received invalid timestamp string: {:?}", err)))
}

fn convert_from_datetime(source: &chrono::DateTime<chrono::Utc>) -> super::DateTime {
    super::DateTime {
        iso_timestamp: source.to_string(),
    }
}

impl TryFrom<Option<super::Digest>> for encoding::Digest {
    type Error = Error;
    fn try_from(source: Option<super::Digest>) -> Result<Self> {
        Ok(source
            .ok_or_else(|| Error::String("Expected non-null digest in rpc message".into()))?
            .into())
    }
}

impl From<super::Digest> for encoding::Digest {
    fn from(source: super::Digest) -> Self {
        Self::from_bytes(source.bytes.as_slice()).unwrap()
    }
}

impl From<&encoding::Digest> for super::Digest {
    fn from(source: &encoding::Digest) -> Self {
        Self {
            bytes: source.as_bytes().to_vec(),
        }
    }
}

impl TryFrom<Option<super::Tag>> for tracking::Tag {
    type Error = Error;
    fn try_from(source: Option<super::Tag>) -> Result<Self> {
        source
            .ok_or_else(|| Error::String("Expected non-null tag in rpc message".into()))?
            .try_into()
    }
}

impl TryFrom<super::Tag> for tracking::Tag {
    type Error = Error;
    fn try_from(source: super::Tag) -> Result<Self> {
        let mut tag = Self::new(source.org, source.name, source.target.try_into()?)?;
        tag.parent = source.parent.try_into()?;
        tag.user = source.user;
        tag.time = convert_to_datetime(source.time)?;
        Ok(tag)
    }
}

impl From<&tracking::Tag> for super::Tag {
    fn from(source: &tracking::Tag) -> Self {
        Self {
            org: source.org(),
            name: source.name(),
            target: Some((&source.target).into()),
            parent: Some((&source.parent).into()),
            user: source.user.clone(),
            time: Some(convert_from_datetime(&source.time)),
        }
    }
}

impl From<crate::Error> for super::Error {
    fn from(err: crate::Error) -> Self {
        let kind = Some(match err {
            crate::Error::UnknownObject(digest) => {
                super::error::Kind::UnknownObject(super::UnknownObjectError {
                    message: digest.to_string(),
                })
            }
            crate::Error::UnknownReference(message) => {
                super::error::Kind::UnknownReference(super::UnknownReferenceError { message })
            }
            crate::Error::AmbiguousReference(message) => {
                super::error::Kind::AmbiguousReference(super::AmbiguousReferenceError { message })
            }
            crate::Error::InvalidReference(message) => {
                super::error::Kind::InvalidReference(super::InvalidReferenceError { message })
            }
            err => super::error::Kind::Other(format!("{:?}", err)),
        });
        Self { kind }
    }
}

impl From<super::Error> for crate::Error {
    fn from(rpc: super::Error) -> Self {
        match rpc.kind {
            Some(super::error::Kind::UnknownObject(rpc)) => {
                match crate::encoding::Digest::parse(&rpc.message) {
                    Ok(digest) => crate::Error::UnknownObject(digest),
                    Err(_) => crate::Error::String(format!(
                        "Server reported UnknownObject but did not provide a valid digest"
                    )),
                }
            }
            Some(super::error::Kind::UnknownReference(rpc)) => {
                crate::Error::UnknownReference(rpc.message)
            }
            Some(super::error::Kind::AmbiguousReference(rpc)) => {
                crate::Error::AmbiguousReference(rpc.message)
            }
            Some(super::error::Kind::InvalidReference(rpc)) => {
                crate::Error::InvalidReference(rpc.message)
            }
            Some(super::error::Kind::Other(message)) => crate::Error::String(message),
            None => crate::Error::String(format!("Server did not provide an error message")),
        }
    }
}

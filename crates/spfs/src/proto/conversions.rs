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
            bytes: source.as_bytes().into_iter().cloned().collect(),
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

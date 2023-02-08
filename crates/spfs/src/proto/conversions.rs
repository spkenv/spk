// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk
use std::convert::{TryFrom, TryInto};
use std::ops::Not;

use crate::{encoding, graph, storage, tracking, Error, Result};

pub(crate) fn convert_to_datetime(
    source: Option<super::DateTime>,
) -> Result<chrono::DateTime<chrono::Utc>> {
    use std::str::FromStr;
    let source =
        source.ok_or_else(|| Error::String("Expected non-null digest in rpc message".into()))?;
    chrono::DateTime::<chrono::Utc>::from_str(&source.iso_timestamp)
        .map_err(|err| Error::String(format!("Received invalid timestamp string: {err:?}")))
}

pub(crate) fn convert_from_datetime(source: &chrono::DateTime<chrono::Utc>) -> super::DateTime {
    super::DateTime {
        iso_timestamp: source.to_string(),
    }
}

pub fn convert_digest(source: Option<super::Digest>) -> Result<encoding::Digest> {
    source
        .ok_or_else(|| Error::String("Expected non-null digest in rpc message".into()))?
        .try_into()
}

impl TryFrom<super::Digest> for encoding::Digest {
    type Error = Error;
    fn try_from(source: super::Digest) -> Result<Self> {
        Self::from_bytes(source.bytes.as_slice()).map_err(Error::Encoding)
    }
}

impl From<encoding::Digest> for super::Digest {
    fn from(source: encoding::Digest) -> Self {
        Self {
            bytes: Vec::from(source.into_bytes()),
        }
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
        let org = source.org.is_empty().not().then_some(source.org);
        let mut tag = Self::new(org, source.name, convert_digest(source.target)?)?;
        tag.parent = convert_digest(source.parent)?;
        tag.user = source.user;
        tag.time = convert_to_datetime(source.time)?;
        Ok(tag)
    }
}

impl From<&tracking::Tag> for super::Tag {
    fn from(source: &tracking::Tag) -> Self {
        Self {
            org: source.org().unwrap_or_default(),
            name: source.name(),
            target: Some((&source.target).into()),
            parent: Some((&source.parent).into()),
            user: source.user.clone(),
            time: Some(convert_from_datetime(&source.time)),
        }
    }
}

impl From<Error> for super::Error {
    fn from(err: Error) -> Self {
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
            err => super::error::Kind::Other(format!("{err:?}")),
        });
        Self { kind }
    }
}

impl From<super::Error> for Error {
    fn from(rpc: super::Error) -> Self {
        match rpc.kind {
            Some(super::error::Kind::UnknownObject(rpc)) => {
                match crate::encoding::Digest::parse(&rpc.message) {
                    Ok(digest) => crate::Error::UnknownObject(digest),
                    Err(_) => crate::Error::String(
                        "Server reported UnknownObject but did not provide a valid digest"
                            .to_string(),
                    ),
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
            Some(super::error::Kind::Other(message)) => Error::String(message),
            None => Error::String("Server did not provide an error message".to_string()),
        }
    }
}

impl From<&graph::Object> for super::Object {
    fn from(source: &graph::Object) -> Self {
        use super::object::Kind;
        super::Object {
            kind: Some(match source {
                graph::Object::Platform(o) => Kind::Platform(o.into()),
                graph::Object::Layer(o) => Kind::Layer(o.into()),
                graph::Object::Manifest(o) => Kind::Manifest(o.into()),
                graph::Object::Tree(o) => Kind::Tree(o.into()),
                graph::Object::Blob(o) => Kind::Blob(o.into()),
                graph::Object::Mask => Kind::Mask(true),
            }),
        }
    }
}

impl TryFrom<Option<super::Object>> for graph::Object {
    type Error = Error;
    fn try_from(source: Option<super::Object>) -> Result<Self> {
        source
            .ok_or_else(|| Error::String("Expected non-null object in rpc message".into()))?
            .try_into()
    }
}

impl TryFrom<super::Object> for graph::Object {
    type Error = Error;
    fn try_from(source: super::Object) -> Result<Self> {
        use super::object::Kind;
        match source.kind {
            Some(Kind::Platform(o)) => Ok(graph::Object::Platform(o.try_into()?)),
            Some(Kind::Layer(o)) => Ok(graph::Object::Layer(o.try_into()?)),
            Some(Kind::Manifest(o)) => Ok(graph::Object::Manifest(o.try_into()?)),
            Some(Kind::Tree(o)) => Ok(graph::Object::Tree(o.try_into()?)),
            Some(Kind::Blob(o)) => Ok(graph::Object::Blob(o.try_into()?)),
            Some(Kind::Mask(_)) => Ok(graph::Object::Mask),
            None => Err(Error::String(
                "Expected non-empty object kind in rpc message".to_string(),
            )),
        }
    }
}

impl From<&graph::Platform> for super::Platform {
    fn from(source: &graph::Platform) -> Self {
        Self {
            stack: source.stack.iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<super::Platform> for graph::Platform {
    type Error = Error;
    fn try_from(source: super::Platform) -> Result<Self> {
        Ok(Self {
            stack: source
                .stack
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<Vec<_>>>()?,
        })
    }
}

impl From<&graph::Layer> for super::Layer {
    fn from(source: &graph::Layer) -> Self {
        Self {
            manifest: Some((&source.manifest).into()),
        }
    }
}

impl TryFrom<super::Layer> for graph::Layer {
    type Error = Error;
    fn try_from(source: super::Layer) -> Result<Self> {
        Ok(Self {
            manifest: convert_digest(source.manifest)?,
        })
    }
}

impl From<&graph::Manifest> for super::Manifest {
    fn from(source: &graph::Manifest) -> Self {
        let mut trees = source.iter_trees();
        let root = trees.next().map(Into::into);
        Self {
            root,
            trees: trees.map(Into::into).collect(),
        }
    }
}

impl TryFrom<super::Manifest> for graph::Manifest {
    type Error = Error;
    fn try_from(source: super::Manifest) -> Result<Self> {
        let mut out = Self::new(source.root.try_into()?);
        for tree in source.trees.into_iter() {
            out.insert_tree(tree.try_into()?)?;
        }
        Ok(out)
    }
}

impl From<&graph::Tree> for super::Tree {
    fn from(source: &graph::Tree) -> Self {
        Self {
            entries: source.entries.iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<Option<super::Tree>> for graph::Tree {
    type Error = Error;
    fn try_from(source: Option<super::Tree>) -> Result<Self> {
        source
            .ok_or_else(|| Error::String("Expected non-null tree in rpc message".into()))?
            .try_into()
    }
}

impl TryFrom<super::Tree> for graph::Tree {
    type Error = Error;
    fn try_from(source: super::Tree) -> Result<Self> {
        Ok(Self {
            entries: source
                .entries
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_>>()?,
        })
    }
}

impl From<&graph::Entry> for super::Entry {
    fn from(source: &graph::Entry) -> Self {
        let kind = match source.kind {
            tracking::EntryKind::Tree => super::EntryKind::Tree as i32,
            tracking::EntryKind::Blob => super::EntryKind::Blob as i32,
            tracking::EntryKind::Mask => super::EntryKind::Mask as i32,
        };
        Self {
            object: Some((&source.object).into()),
            kind,
            mode: source.mode,
            size: source.size,
            name: source.name.clone(),
        }
    }
}

impl TryFrom<super::Entry> for graph::Entry {
    type Error = Error;
    fn try_from(source: super::Entry) -> Result<Self> {
        let kind = match super::EntryKind::from_i32(source.kind) {
            Some(super::EntryKind::Tree) => tracking::EntryKind::Tree,
            Some(super::EntryKind::Blob) => tracking::EntryKind::Blob,
            Some(super::EntryKind::Mask) => tracking::EntryKind::Mask,
            None => return Err("Received unknown entry kind in rpm data".into()),
        };
        Ok(Self {
            object: convert_digest(source.object)?,
            kind,
            mode: source.mode,
            size: source.size,
            name: source.name,
        })
    }
}

impl From<&graph::Blob> for super::Blob {
    fn from(source: &graph::Blob) -> Self {
        Self {
            payload: Some((&source.payload).into()),
            size: source.size,
        }
    }
}

impl TryFrom<super::Blob> for graph::Blob {
    type Error = Error;
    fn try_from(source: super::Blob) -> Result<Self> {
        Ok(Self {
            payload: convert_digest(source.payload)?,
            size: source.size,
        })
    }
}

impl From<&storage::EntryType> for super::ls_tags_response::Entry {
    fn from(e: &storage::EntryType) -> Self {
        Self {
            kind: Some(match e {
                storage::EntryType::Folder(e) => {
                    super::ls_tags_response::entry::Kind::Folder(e.to_owned())
                }
                storage::EntryType::Tag(e) => {
                    super::ls_tags_response::entry::Kind::Tag(e.to_owned())
                }
            }),
        }
    }
}

impl TryFrom<super::ls_tags_response::Entry> for storage::EntryType {
    type Error = Error;
    fn try_from(entry: super::ls_tags_response::Entry) -> Result<Self> {
        match entry.kind {
            Some(e) => match e {
                super::ls_tags_response::entry::Kind::Folder(e) => {
                    Ok(storage::EntryType::Folder(e))
                }
                super::ls_tags_response::entry::Kind::Tag(e) => Ok(storage::EntryType::Tag(e)),
            },
            None => Err("Unknown entry kind".into()),
        }
    }
}

impl From<graph::DigestSearchCriteria> for super::DigestSearchCriteria {
    fn from(search_criteria: graph::DigestSearchCriteria) -> Self {
        Self {
            criteria: Some(match search_criteria {
                graph::DigestSearchCriteria::All => super::digest_search_criteria::Criteria::All(
                    super::digest_search_criteria::All {},
                ),
                graph::DigestSearchCriteria::StartsWith(partial) => {
                    super::digest_search_criteria::Criteria::StartsWith(
                        super::digest_search_criteria::StartsWith {
                            bytes: partial.into(),
                        },
                    )
                }
            }),
        }
    }
}

impl TryFrom<super::DigestSearchCriteria> for graph::DigestSearchCriteria {
    type Error = Error;
    fn try_from(search_criteria: super::DigestSearchCriteria) -> Result<Self> {
        match search_criteria.criteria {
            Some(criteria) => match criteria {
                super::digest_search_criteria::Criteria::All(_) => {
                    Ok(graph::DigestSearchCriteria::All)
                }
                super::digest_search_criteria::Criteria::StartsWith(bytes) => {
                    Ok(graph::DigestSearchCriteria::StartsWith(bytes.bytes.into()))
                }
            },
            None => Err("Unknown criteria kind".into()),
        }
    }
}

impl TryFrom<Option<super::DigestSearchCriteria>> for graph::DigestSearchCriteria {
    type Error = Error;
    fn try_from(search_criteria: Option<super::DigestSearchCriteria>) -> Result<Self> {
        search_criteria
            .ok_or_else(|| Error::String("Expected non-null criteria in rpc message".into()))?
            .try_into()
    }
}

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
        Ok(Self::from_bytes(source.bytes.as_slice())?)
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

impl From<&graph::object::Enum> for super::Object {
    fn from(source: &graph::object::Enum) -> Self {
        use super::object::Kind;
        super::Object {
            kind: Some(match source {
                graph::object::Enum::Platform(o) => Kind::Platform(o.into()),
                graph::object::Enum::Layer(o) => Kind::Layer(o.into()),
                graph::object::Enum::Manifest(o) => Kind::Manifest(o.into()),
                graph::object::Enum::Blob(o) => Kind::Blob(o.into()),
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
            Some(Kind::Platform(o)) => Ok(graph::Platform::try_from(o)?.into_object()),
            Some(Kind::Layer(o)) => Ok(graph::Layer::try_from(o)?.into_object()),
            Some(Kind::Manifest(o)) => Ok(graph::Manifest::try_from(o)?.into_object()),
            Some(Kind::Blob(o)) => Ok(graph::Blob::try_from(o)?.into_object()),
            Some(Kind::Tree(_)) | Some(Kind::Mask(_)) => Err(Error::String(format!(
                "Unexpected and unsupported object kind {:?}",
                source.kind
            ))),
            None => Err(Error::String(
                "Expected non-empty object kind in rpc message".to_string(),
            )),
        }
    }
}

impl From<&graph::Platform> for super::Platform {
    fn from(source: &graph::Platform) -> Self {
        Self {
            stack: source.iter_bottom_up().map(Into::into).collect(),
        }
    }
}

impl TryFrom<super::Platform> for graph::Platform {
    type Error = Error;

    fn try_from(source: super::Platform) -> Result<Self> {
        Ok(Self::from(
            source
                .stack
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<graph::Stack>>()?,
        ))
    }
}

impl From<&graph::Layer> for super::Layer {
    fn from(source: &graph::Layer) -> Self {
        Self {
            manifest: Some(source.manifest().into()),
        }
    }
}

impl TryFrom<super::Layer> for graph::Layer {
    type Error = Error;
    fn try_from(source: super::Layer) -> Result<Self> {
        Ok(Self::new(&convert_digest(source.manifest)?))
    }
}

impl From<&graph::Manifest> for super::Manifest {
    fn from(source: &graph::Manifest) -> Self {
        let mut trees = source.iter_trees().map(|t| (&t).into());
        let root = trees.next();
        Self {
            root,
            trees: trees.collect(),
        }
    }
}

impl TryFrom<super::Manifest> for graph::Manifest {
    type Error = Error;
    fn try_from(source: super::Manifest) -> Result<Self> {
        let mut builder = flatbuffers::FlatBufferBuilder::with_capacity(256);
        let make_tree = |entry: super::Tree| {
            let entries = entry
                .entries
                .into_iter()
                .map(|entry: super::Entry| {
                    let kind = match super::EntryKind::try_from(entry.kind) {
                        Ok(super::EntryKind::Tree) => spfs_proto::EntryKind::Tree,
                        Ok(super::EntryKind::Blob) => spfs_proto::EntryKind::Blob,
                        Ok(super::EntryKind::Mask) => spfs_proto::EntryKind::Mask,
                        Err(_) => return Err("Received unknown entry kind in rpc data".into()),
                    };
                    let name = builder.create_string(&entry.name);
                    Ok(spfs_proto::Entry::create(
                        &mut builder,
                        &spfs_proto::EntryArgs {
                            kind,
                            object: Some(&convert_digest(entry.object)?),
                            mode: entry.mode,
                            size_: entry.size,
                            name: Some(name),
                        },
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let entries = builder.create_vector(&entries);
            Ok(spfs_proto::Tree::create(
                &mut builder,
                &spfs_proto::TreeArgs {
                    entries: Some(entries),
                },
            ))
        };
        let trees = source
            .root
            .into_iter()
            .chain(source.trees)
            .map(make_tree)
            .collect::<Result<Vec<_>>>()?;
        let trees = builder.create_vector(&trees);
        let manifest = spfs_proto::Manifest::create(
            &mut builder,
            &spfs_proto::ManifestArgs { trees: Some(trees) },
        );
        let any = spfs_proto::AnyObject::create(
            &mut builder,
            &spfs_proto::AnyObjectArgs {
                object_type: spfs_proto::Object::Manifest,
                object: Some(manifest.as_union_value()),
            },
        );
        builder.finish_minimal(any);
        Ok(unsafe {
            // Safety: buf must contain an AnyObject of the provided
            // type, which is what we just constructed
            graph::Object::new_with_default_header(
                builder.finished_data(),
                graph::ObjectKind::Manifest,
            )
        }
        .into_manifest()
        .expect("known to be a manifest"))
    }
}

impl<'buf> From<&graph::Tree<'buf>> for super::Tree {
    fn from(source: &graph::Tree) -> Self {
        Self {
            entries: source.entries().map(|e| (&e).into()).collect(),
        }
    }
}

impl<'buf> From<&graph::Entry<'buf>> for super::Entry {
    fn from(source: &graph::Entry) -> Self {
        let kind = match source.kind() {
            tracking::EntryKind::Tree => super::EntryKind::Tree as i32,
            tracking::EntryKind::Blob => super::EntryKind::Blob as i32,
            tracking::EntryKind::Mask => super::EntryKind::Mask as i32,
        };
        Self {
            object: Some((source.object()).into()),
            kind,
            mode: source.mode(),
            size: source.size(),
            name: source.name().to_owned(),
        }
    }
}

impl From<&graph::Blob> for super::Blob {
    fn from(source: &graph::Blob) -> Self {
        Self {
            payload: Some(source.payload().into()),
            size: source.size(),
        }
    }
}

impl TryFrom<super::Blob> for graph::Blob {
    type Error = Error;
    fn try_from(source: super::Blob) -> Result<Self> {
        Ok(Self::new(&convert_digest(source.payload)?, source.size))
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

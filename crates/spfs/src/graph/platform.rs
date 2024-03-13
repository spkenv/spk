// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use super::object::HeaderBuilder;
use super::{ObjectKind, Stack};
use crate::{encoding, Error, Result};

#[cfg(test)]
#[path = "./platform_test.rs"]
mod platform_test;

/// Platforms represent a predetermined collection of layers.
///
/// Platforms capture an entire runtime stack of layers or other platforms
/// as a single, identifiable object which can be applied/installed to
/// future runtimes.
pub type Platform = super::object::FlatObject<spfs_proto::Platform<'static>>;

impl std::fmt::Debug for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Platform")
            .field("stack", &self.to_stack())
            .finish()
    }
}

impl Platform {
    pub fn from_digestible<D, I>(layers: I) -> Result<Self>
    where
        D: encoding::Digestible,
        Error: std::convert::From<D::Error>,
        I: IntoIterator<Item = D>,
    {
        Stack::from_digestible(layers).map(Into::into)
    }

    #[inline]
    pub fn builder() -> PlatformBuilder {
        PlatformBuilder::default()
    }

    /// Reconstruct a mutable stack from this platform's layers
    pub fn to_stack(&self) -> Stack {
        self.iter_bottom_up().copied().collect()
    }

    /// Iterate the stack lazily from bottom to top
    pub fn iter_bottom_up(&self) -> impl Iterator<Item = &encoding::Digest> {
        self.proto().layers().iter()
    }

    /// Return the digests of objects that this manifest refers to.
    pub fn child_objects(&self) -> Vec<encoding::Digest> {
        self.iter_bottom_up().copied().collect()
    }

    pub(super) fn legacy_encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        // use a vec to know the name ahead of time and
        // avoid iterating the stack twice
        let digests = self.iter_bottom_up().collect::<Vec<_>>();
        encoding::write_uint64(&mut writer, digests.len() as u64)?;
        // for historical reasons, and to remain backward-compatible, platform
        // stacks are stored in reverse (top-down) order
        for digest in digests.into_iter().rev() {
            encoding::write_digest(&mut writer, digest)?;
        }
        Ok(())
    }
}

impl<T> From<T> for Platform
where
    T: Into<Stack>,
{
    fn from(value: T) -> Self {
        Self::builder().with_stack(value.into()).build()
    }
}

impl<T> FromIterator<T> for Platform
where
    Stack: FromIterator<T>,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Self::builder().with_stack(Stack::from_iter(iter)).build()
    }
}

#[derive(Debug)]
pub struct PlatformBuilder {
    header: HeaderBuilder,
    stack: Stack,
}

impl Default for PlatformBuilder {
    fn default() -> Self {
        Self {
            header: HeaderBuilder::new(ObjectKind::Platform),
            stack: Stack::default(),
        }
    }
}

impl PlatformBuilder {
    pub fn with_stack(mut self, stack: Stack) -> Self {
        self.stack.extend(stack.iter_bottom_up());
        self
    }

    pub fn with_header<F>(mut self, mut header: F) -> Self
    where
        F: FnMut(HeaderBuilder) -> HeaderBuilder,
    {
        self.header = header(self.header).with_object_kind(ObjectKind::Platform);
        self
    }

    pub fn build(self) -> Platform {
        super::BUILDER.with_borrow_mut(|builder| {
            let stack: Vec<_> = self.stack.iter_bottom_up().collect();
            let stack = builder.create_vector(&stack);
            let platform = spfs_proto::Platform::create(
                builder,
                &spfs_proto::PlatformArgs {
                    layers: Some(stack),
                },
            );
            let any = spfs_proto::AnyObject::create(
                builder,
                &spfs_proto::AnyObjectArgs {
                    object_type: spfs_proto::Object::Platform,
                    object: Some(platform.as_union_value()),
                },
            );
            builder.finish_minimal(any);
            let offset = unsafe {
                // Safety: we have just created this buffer
                // so already know the root type with certainty
                flatbuffers::root_unchecked::<spfs_proto::AnyObject>(builder.finished_data())
                    .object_as_platform()
                    .unwrap()
                    ._tab
                    .loc()
            };
            let obj = unsafe {
                // Safety: the provided buf and offset mut contain
                // a valid object and point to the contained layer
                // which is what we've done
                Platform::new_with_header(self.header.build(), builder.finished_data(), offset)
            };
            builder.reset(); // to be used again
            obj
        })
    }

    /// Read a data encoded using the legacy format, and
    /// use the data to fill and complete this builder
    pub fn legacy_decode(self, mut reader: &mut impl std::io::Read) -> Result<Platform> {
        let num_layers = encoding::read_uint64(&mut reader)?;
        tracing::error!("read {} layers in platform", num_layers);
        let mut layers = Vec::with_capacity(num_layers as usize);
        for _ in 0..num_layers {
            layers.push(encoding::read_digest(&mut reader)?);
        }
        // for historical reasons, and to remain backward-compatible, platform
        // stacks are stored in reverse (top-down) order
        Ok(Platform::from_iter(layers.into_iter().rev()))
    }
}

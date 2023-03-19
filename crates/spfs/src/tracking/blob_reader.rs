// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::pin::Pin;

use tokio::io::{AsyncBufRead, AsyncRead};

/// Primarily, a source of binary contents for a blob, but may
/// also provide additional information about the data when available
/// and relevant.
pub trait BlobRead: AsyncBufRead + Send + Sync + 'static {
    /// A blob may have permissions associated with it if it is
    /// being written for a specific file in a manifest.
    ///
    /// Storage implementations may choose to use these permissions
    /// when creating the payload in order to save the need for proxies in the future.
    /// The use of these permissions is not guaranteed, and also may be masked for
    /// safety and reuse by the storage implementation.
    fn permissions(&self) -> Option<u32> {
        None
    }
}

impl BlobRead for &'static [u8] {}

impl<R> BlobRead for tokio::io::BufReader<R> where R: AsyncRead + Send + Sync + 'static {}

impl<T> BlobRead for std::io::Cursor<T> where T: AsRef<[u8]> + Unpin + Send + Sync + 'static {}

impl<T> BlobRead for Pin<Box<T>>
where
    T: BlobRead + ?Sized,
{
    fn permissions(&self) -> Option<u32> {
        (**self).permissions()
    }
}

/// Extensions for the [`BlobRead`] trait that are not Object-safe
pub trait BlobReadExt {
    /// Associate the given permissions with this reader.
    ///
    /// See [`BlobRead::permissions`] for details.
    fn with_permissions(self, permissions: u32) -> WithPermissions<Self>
    where
        Self: Sized,
    {
        WithPermissions {
            inner: self,
            permissions,
        }
    }
}

impl<T> BlobReadExt for T where T: BlobRead + ?Sized {}

/// The type returned by [`BlobReadExt::with_permissions`].
pub struct WithPermissions<T> {
    inner: T,
    permissions: u32,
}

impl<T> AsyncRead for WithPermissions<T>
where
    T: AsyncRead + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<T> AsyncBufRead for WithPermissions<T>
where
    T: AsyncBufRead + Unpin,
    Self: Unpin,
{
    fn poll_fill_buf(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<&[u8]>> {
        // Safety: we must guarantee that the inner T will not move so long
        // as self does not move. We do not add a manual impl Unpin for Self
        // and so the Unpin bounds on this impl provide that promise
        unsafe { self.map_unchecked_mut(|s| &mut s.inner) }.poll_fill_buf(cx)
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        Pin::new(&mut self.inner).consume(amt)
    }
}

impl<T> BlobRead for WithPermissions<T>
where
    T: BlobRead + Unpin,
{
    fn permissions(&self) -> Option<u32> {
        Some(self.permissions)
    }
}

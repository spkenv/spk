// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::{io::ErrorKind, pin::Pin};

use futures::Stream;

use super::FSRepository;
use crate::{encoding, Error, Result};

impl crate::storage::PayloadStorage for FSRepository {
    fn iter_payload_digests(&self) -> Pin<Box<dyn Stream<Item = Result<encoding::Digest>>>> {
        match self.payloads.iter() {
            Ok(iter) => Box::pin(futures::stream::iter(iter)),
            Err(err) => Box::pin(futures::stream::once(async { Err(err) })),
        }
    }

    fn write_data(
        &mut self,
        reader: Box<dyn std::io::Read + Send + 'static>,
    ) -> Result<(encoding::Digest, u64)> {
        self.payloads.write_data(reader)
    }

    fn open_payload(
        &self,
        digest: &encoding::Digest,
    ) -> Result<Box<dyn std::io::Read + Send + 'static>> {
        let path = self.payloads.build_digest_path(digest);
        match std::fs::File::open(&path) {
            Ok(file) => Ok(Box::new(file)),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Err(Error::UnknownObject(*digest)),
                _ => Err(err.into()),
            },
        }
    }

    fn remove_payload(&mut self, digest: &encoding::Digest) -> Result<()> {
        let path = self.payloads.build_digest_path(digest);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Err(Error::UnknownObject(*digest)),
                _ => Err(err.into()),
            },
        }
    }
}

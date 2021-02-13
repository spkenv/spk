use std::io::ErrorKind;

use super::FSRepository;
use crate::{encoding, graph, Result};

impl crate::storage::PayloadStorage for FSRepository {
    fn iter_payload_digests(&self) -> Box<dyn Iterator<Item = Result<encoding::Digest>>> {
        match self.payloads.iter() {
            Ok(iter) => Box::new(iter),
            Err(err) => Box::new(vec![Err(err)].into_iter()),
        }
    }

    fn write_data(
        &mut self,
        reader: Box<&mut dyn std::io::Read>,
    ) -> Result<(encoding::Digest, u64)> {
        self.payloads.write_data(reader)
    }

    fn open_payload(&self, digest: &encoding::Digest) -> Result<Box<dyn std::io::Read>> {
        let path = self.payloads.build_digest_path(digest);
        Ok(Box::new(std::fs::File::open(&path)?))
    }

    fn remove_payload(&mut self, digest: &encoding::Digest) -> Result<()> {
        let path = self.payloads.build_digest_path(digest);
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(err) => match err.kind() {
                ErrorKind::NotFound => Err(graph::UnknownObjectError::new(digest).into()),
                _ => Err(err.into()),
            },
        }
    }
}

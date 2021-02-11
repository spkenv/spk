use std::path::Path;

use tar::Archive;

use crate::graph;
use crate::prelude::*;
use crate::Result;

/// An spfs repository in a tarball.
#[derive(Debug)]
pub struct TarRepository {
    path: std::path::PathBuf,
}

impl TarRepository {
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(Self {
            path: path.as_ref().canonicalize()?,
        })
        // let file = std::fs::OpenOptions::new()
        //     .create(true)
        //     .read(true)
        //     .write(true)
        //     .open(path.as_ref())?;
        // Ok(Self(Archive::new(file)))
    }

    // Open a repository over the given directory, which must already
    // exist and be a repository
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        Ok(Self {
            path: path.as_ref().canonicalize()?,
        })
        // let file = std::fs::OpenOptions::new()
        //     .read(true)
        //     .write(true)
        //     .open(path.as_ref())?;
        // Ok(Self(Archive::new(file)))
    }
}

impl graph::DatabaseView for TarRepository {
    fn read_object(&self, digest: &crate::encoding::Digest) -> graph::Result<graph::Object> {
        todo!()
    }

    fn iter_digests(&self) -> Box<dyn Iterator<Item = graph::Result<crate::encoding::Digest>>> {
        todo!()
    }

    fn iter_objects<'db>(&'db self) -> graph::DatabaseIterator<'db> {
        todo!()
    }

    fn walk_objects<'db>(&'db self, root: &crate::encoding::Digest) -> graph::DatabaseWalker<'db> {
        todo!()
    }
}
impl graph::Database for TarRepository {
    fn write_object(&mut self, obj: &graph::Object) -> graph::Result<()> {
        todo!()
    }

    fn remove_object(&mut self, digest: &crate::encoding::Digest) -> graph::Result<()> {
        todo!()
    }
}
impl PayloadStorage for TarRepository {
    fn iter_digests(&self) -> Box<dyn Iterator<Item = Result<crate::encoding::Digest>>> {
        todo!()
    }

    fn write_data(
        &mut self,
        reader: Box<&mut dyn std::io::Read>,
    ) -> Result<(crate::encoding::Digest, u64)> {
        todo!()
    }

    fn open_payload(&self, digest: &crate::encoding::Digest) -> Result<Box<dyn std::io::Read>> {
        todo!()
    }

    fn remove_payload(&mut self, digest: &crate::encoding::Digest) -> Result<()> {
        todo!()
    }
}
impl Repository for TarRepository {
    fn address(&self) -> url::Url {
        url::Url::from_file_path(&self.path).expect("unexpected failure creating url")
    }
}

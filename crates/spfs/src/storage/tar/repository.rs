use std::path::Path;

use tar::Archive;

use crate::prelude::*;
use crate::Result;

/// An spfs repository in a tarball.
#[derive(Debug)]
pub struct TarRepository(Archive<std::fs::File>);

impl TarRepository {
    pub fn create<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(path.as_ref())?;
        Ok(Self(Archive::new(file)))
    }

    // Open a repository over the given directory, which must already
    // exist and be a repository
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path.as_ref())?;
        Ok(Self(Archive::new(file)))
    }
}

impl Repository for TarRepository {
    fn address(&self) -> url::Url {}
}

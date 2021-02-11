use std::path::{Path, PathBuf};

use super::FSHashStore;
use crate::runtime::makedirs_with_perms;
use crate::storage::Repository;
use crate::Result;

/// A pure filesystem-based repository of spfs data.
pub struct FSRepository {
    root: PathBuf,
    /// stores the actual file data/payloads of this repo
    pub payloads: FSHashStore,
    /// stores all digraph object data for this repo
    pub objects: FSHashStore,
    /// stores rendered file system layers for use in overlayfs
    pub renders: FSHashStore,
}

impl FSRepository {
    /// Establish a new filesystem repository
    pub fn create<P: AsRef<Path>>(root: P) -> Result<Self> {
        makedirs_with_perms(root.as_ref(), 0o777)?;
        makedirs_with_perms(root.as_ref().join("tags"), 0o777)?;
        makedirs_with_perms(root.as_ref().join("objects"), 0o777)?;
        makedirs_with_perms(root.as_ref().join("payloads"), 0o777)?;
        makedirs_with_perms(root.as_ref().join("renders"), 0o777)?;
        Self::new(root)
    }

    // Open a repository over the given directory, which must already
    // exist and be a repository
    pub fn new<P: AsRef<Path>>(root: P) -> Result<Self> {
        let root = std::fs::canonicalize(root)?;
        Ok(Self {
            objects: FSHashStore::new(root.join("objects"))?,
            payloads: FSHashStore::new(root.join("payloads"))?,
            renders: FSHashStore::new(root.join("renders"))?,
            root: root,
        })
    }

    pub fn root(&self) -> PathBuf {
        self.root.clone()
    }
}

impl Repository for FSRepository {
    fn address(&self) -> url::Url {
        todo!()
    }
}

impl std::fmt::Debug for FSRepository {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("FSRepository @ {:?}", self.root()))
    }
}

// (Repository, FSManifestViewer):

//     def __init__(self, root: &str, create: bool = False):

//         if root.startswith("file:///"):
//             root = root[len("file://") :]
//         elif root.startswith("file:"):
//             root = root[len("file:") :]

//         self.__root = os.path.abspath(root)

//         if not os.path.exists(self.__root) and not create:
//             raise ValueError("Directory does not exist: " + self.__root)
//         makedirs_with_perms(self.__root)

//         if len(os.listdir(self.__root)) == 0:
//             set_last_migration(self.__root, spfs.__version__)

//         self.objects = FSDatabase(os.path.join(self.__root, "objects"))
//         self.payloads = FSPayloadStorage(os.path.join(self.__root, "payloads"))
//         FSManifestViewer.__init__(
//             self,
//             root=os.path.join(self.__root, "renders"),
//             payloads=self.payloads,
//         )
//         Repository.__init__(
//             self,
//             tags=TagStorage(os.path.join(self.__root, "tags")),
//             object_database=self.objects,
//             payload_storage=self.payloads,
//         )

//         self.minimum_compatible_version = "0.16.0"
//         repo_version = semver.VersionInfo.parse(self.last_migration())
//         if repo_version.compare(spfs.__version__) > 0:
//             raise RuntimeError(
//                 f"Repository requires a newer version of spfs [{repo_version}]: {self.address()}"
//             )
//         if repo_version.compare(self.minimum_compatible_version) < 0:
//             raise MigrationRequiredError(
//                 str(repo_version), self.minimum_compatible_version
//             )

//     @property
//     def root(self) -> str:
//         return self.__root

//     def concurrent(self) -> bool:
//         return True

//     def address(self) -> str:
//         return f"file://{self.root}"

//     def last_migration(self) -> str:

//         return read_last_migration_version(self.__root)

//     def set_last_migration(self, version: &str = None) -> None:

//         set_last_migration(self.__root, version)

// Read the last marked migration version for a repository root path.
pub fn read_last_migration_version<P: AsRef<Path>>(root: P) -> Result<String> {
    let version_file = root.as_ref().join("VERSION");
    match std::fs::read(version_file) {
        Ok(data) => {
            return Ok(String::from_utf8_lossy(data.as_slice()).trim().to_string());
        }
        Err(err) => match err.kind() {
            std::io::ErrorKind::NotFound => (),
            _ => return Err(err.into()),
        },
    }

    // versioned repo introduced in 0.13.0
    // best guess if the repo exists and it's missing
    // then it predates the creation of this file
    Ok("0.12.0".to_string())
}

/// Set the last migration version of the repo with the given root directory.
pub fn set_last_migration<P: AsRef<Path>>(root: P, version: Option<&str>) -> Result<()> {
    let version = match version {
        Some(v) => v,
        None => crate::VERSION,
    };
    let version_file = root.as_ref().join("VERSION");
    std::fs::write(version_file, version)?;
    Ok(())
}

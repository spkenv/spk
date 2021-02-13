use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use relative_path::RelativePath;

use super::FSRepository;
use crate::{encoding, tracking, Result};
use encoding::Decodable;

#[cfg(test)]
#[path = "./tag_test.rs"]
mod tag_test;

const TAG_EXT: &str = "tag";

impl FSRepository {
    fn tags_root(&self) -> PathBuf {
        self.root().join("tags")
    }
}

impl crate::storage::TagStorage for FSRepository {
    fn resolve_tag(&self, tag_spec: &tracking::TagSpec) -> Result<tracking::Tag> {
        todo!()
        //     spec = tracking.TagSpec(tag)
        //     try:
        //         stream = self.read_tag(tag)
        //         for _ in range(spec.version):
        //             next(stream)
        //         return next(stream)
        //     except StopIteration:
        //         raise graph.UnknownReferenceError(
        //             f"tag or tag version does not exist {tag}"
        //         )
    }

    fn ls_tags(&self, path: &RelativePath) -> Result<Box<dyn Iterator<Item = String>>> {
        let filepath = path.to_path(self.tags_root());
        let read_dir = match std::fs::read_dir(&filepath) {
            Ok(r) => r,
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => return Ok(Box::new(Vec::new().into_iter())),
                _ => return Err(err.into()),
            },
        };

        let mut entries = Vec::new();
        for entry in read_dir {
            let entry = entry?;
            let path = entry.path();
            match path.file_stem() {
                None => continue,
                Some(tag_name) => entries.push(tag_name.to_string_lossy().to_string()),
            }
        }
        Ok(Box::new(entries.into_iter()))
    }

    /// Find tags that point to the given digest.
    ///
    /// This is an O(n) operation based on the number of all
    /// tag versions in each tag stream.
    fn find_tags(
        &self,
        digest: &encoding::Digest,
    ) -> Box<dyn Iterator<Item = Result<tracking::TagSpec>>> {
        todo!()
        // for (spec, stream) in self.iter_tag_streams() {
        //     i = -1
        //     for tag in stream:
        //         i += 1
        //         if tag.target != digest:
        //             continue
        //         yield tracking.build_tag_spec(name=spec.name, org=spec.org, version=i)
        // }
    }

    /// Iterate through the available tags in this storage.
    fn iter_tag_streams(
        &self,
    ) -> Box<
        dyn Iterator<Item = Result<(tracking::TagSpec, Box<dyn Iterator<Item = tracking::Tag>>)>>,
    > {
        Box::new(TagStreamIter::new(&self.tags_root()))
    }

    fn read_tag(&self, tag: &tracking::TagSpec) -> Result<Box<dyn Iterator<Item = tracking::Tag>>> {
        let path = tag.to_path(self.tags_root());
        match read_tag_file(path) {
            Err(err) => Err(err),
            Ok(iter) => {
                let tags: Result<Vec<_>> = iter.into_iter().collect();
                Ok(Box::new(tags?.into_iter().rev()))
            }
        }
    }

    fn push_raw_tag(&mut self, tag: &tracking::Tag) -> Result<()> {
        todo!()
        //     filepath = os.path.join(self._root, tag.path + _TAG_EXT)
        //     makedirs_with_perms(os.path.dirname(filepath), perms=0o777)

        //     stream = io.BytesIO()
        //     tag.encode(stream)
        //     encoded_tag = stream.getvalue()
        //     size = len(encoded_tag)

        //     with _tag_lock(filepath):
        //         tag_file_fd = os.open(
        //             filepath, os.O_CREAT | os.O_WRONLY | os.O_APPEND, mode=0o777
        //         )
        //         with os.fdopen(tag_file_fd, "ab") as tag_file:
        //             encoding.write_int(tag_file, size)
        //             tag_file.write(encoded_tag)
        //         try:
        //             os.chmod(filepath, 0o777)
        //         except Exception as err:
        //             _LOGGER.error(
        //                 "Failed to set tag permissions", err=str(err), filepath=filepath
        //             )
        //             pass
    }

    fn remove_tag_stream(&mut self, tag: &tracking::TagSpec) -> Result<()> {
        todo!()
        //     tag_spec = tracking.TagSpec(tag)
        //     filepath = os.path.join(self._root, tag_spec.path + _TAG_EXT)
        //     try:
        //         with _tag_lock(filepath):
        //             os.remove(filepath)
        //     except (RuntimeError, FileNotFoundError):
        //         raise graph.UnknownReferenceError("Unknown tag: " + tag)
        //     head = os.path.dirname(filepath)
        //     while head != self._root:
        //         try:
        //             os.rmdir(head)
        //             head = os.path.dirname(head)
        //         except OSError as e:
        //             if e.errno != errno.ENOTEMPTY:
        //                 raise
        //             break
    }

    fn remove_tag(&mut self, tag: &tracking::Tag) -> Result<()> {
        todo!()
        //     tag_spec = tracking.TagSpec(tag.path)
        //     filepath = os.path.join(self._root, tag_spec.path + _TAG_EXT)
        //     with _tag_lock(filepath):

        //         all_versions = reversed(list(self.read_tag(tag_spec)))
        //         backup_path = filepath + ".backup"
        //         os.rename(filepath, backup_path)
        //         try:
        //             for version in all_versions:
        //                 if version == tag:
        //                     continue
        //                 self.push_raw_tag(version)
        //         except Exception as e:
        //             try:
        //                 os.remove(filepath)
        //             except:
        //                 pass
        //             os.rename(backup_path, filepath)
        //             raise
        //         else:
        //             os.remove(backup_path)
    }
}

struct TagStreamIter {
    root: PathBuf,
    inner: walkdir::IntoIter,
}

impl TagStreamIter {
    fn new<P: AsRef<std::path::Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            inner: walkdir::WalkDir::new(root).into_iter(),
        }
    }
}

impl Iterator for TagStreamIter {
    type Item = Result<(tracking::TagSpec, Box<dyn Iterator<Item = tracking::Tag>>)>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let entry = self.inner.next();
            match entry {
                None => break None,
                Some(Err(err)) => break Some(Err(err.into())),
                Some(Ok(entry)) => {
                    if !entry.file_type().is_file() {
                        continue;
                    }
                    let path = entry.path();
                    if path.extension() != Some(OsStr::new(TAG_EXT)) {
                        continue;
                    }
                    let spec = match tag_from_path(&path, &self.root) {
                        Err(err) => return Some(Err(err)),
                        Ok(spec) => spec,
                    };
                    let tags: Result<Vec<_>> = match read_tag_file(&path) {
                        Err(err) => return Some(Err(err)),
                        Ok(stream) => stream.into_iter().collect(),
                    };
                    break match tags {
                        Err(err) => Some(Err(err)),
                        Ok(tags) => Some(Ok((spec, Box::new(tags.into_iter().rev())))),
                    };
                }
            }
        }
    }
}

/// Return an iterator over all tags in the identified tag file
///
/// This iterator outputs tags from earliest to latest, as stored
/// in the file starting at the beginning
fn read_tag_file<P: AsRef<Path>>(path: P) -> Result<TagIter<std::fs::File>> {
    let reader = std::fs::File::open(path.as_ref())?;
    Ok(TagIter::new(reader))
}

struct TagIter<R: std::io::Read + std::io::Seek>(R);

impl<R: std::io::Read + std::io::Seek> TagIter<R> {
    fn new(reader: R) -> Self {
        Self(reader)
    }
}

impl<R: std::io::Read + std::io::Seek> Iterator for TagIter<R> {
    type Item = Result<tracking::Tag>;

    fn next(&mut self) -> Option<Self::Item> {
        let _size = match encoding::read_int(&mut self.0) {
            Ok(size) => size,
            Err(err) => match err.raw_os_error() {
                Some(libc::EOF) => return None,
                _ => return Some(Err(err)),
            },
        };
        match tracking::Tag::decode(&mut self.0) {
            Err(err) => Some(Err(err)),
            Ok(tag) => Some(Ok(tag)),
        }
    }
}

fn tag_from_path<P: AsRef<Path>, R: AsRef<Path>>(path: P, root: R) -> Result<tracking::TagSpec> {
    let mut path = path.as_ref().to_path_buf();
    let filename = match path.file_stem() {
        Some(stem) => stem.to_owned(),
        None => {
            return Err(format!("Path must end with '.{}' to be considered a tag", TAG_EXT).into())
        }
    };
    path.set_file_name(filename);
    let path = path.strip_prefix(root)?;
    tracking::TagSpec::parse(path.to_string_lossy())
}
pub trait TagExt {
    fn to_path<P: AsRef<Path>>(&self, root: P) -> PathBuf;
}

impl TagExt for tracking::TagSpec {
    fn to_path<P: AsRef<Path>>(&self, root: P) -> PathBuf {
        let mut filepath = root.as_ref().join(self.path());
        let new_name = self.name() + "." + TAG_EXT;
        filepath.set_file_name(new_name);
        filepath
    }
}

// _HAVE_LOCK = False

// @contextlib.contextmanager
// def _tag_lock(filepath: &str) -> Iterator[None]:

//     global _HAVE_LOCK
//     if _HAVE_LOCK:
//         yield
//         return

//     try:
//         open(filepath + ".lock", "xb").close()
//     except FileExistsError:
//         raise RuntimeError(f"Tag already locked [{filepath}]")
//     except Exception as e:
//         raise RuntimeError(f"Cannot lock tag: {str(e)} [{filepath}]")
//     try:
//         _HAVE_LOCK = True
//         yield
//     finally:
//         _HAVE_LOCK = False
//         os.remove(filepath + ".lock")

use std::path::PathBuf;

use relative_path::RelativePath;

use super::FSRepository;
use crate::{encoding, tracking, Result};

#[cfg(test)]
#[path = "./tag_test.rs"]
mod tag_test;

const TAG_EXT: &str = ".tag";

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
            let name = entry.file_name().to_string_lossy().to_string();
            entries.push(
                name.strip_suffix(TAG_EXT)
                    .unwrap_or(name.as_str())
                    .to_string(),
            );
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
        todo!()
        // for root, _, files in os.walk(self._root):

        //     for filename in files:
        //         if not filename.endswith(_TAG_EXT):
        //             continue
        //         filepath = os.path.join(root, filename)
        //         tag = os.path.relpath(filepath[: -len(_TAG_EXT)], self._root)
        //         spec = tracking.TagSpec(tag)
        //         yield (spec, self.read_tag(tag))
    }

    fn read_tag(&self, tag: &tracking::TagSpec) -> Result<Box<dyn Iterator<Item = tracking::Tag>>> {
        todo!()
        //     spec = tracking.TagSpec(tag)
        //     filepath = os.path.join(self._root, spec.path + _TAG_EXT)
        //     try:
        //         blocks = []
        //         with open(filepath, "rb") as f:
        //             while True:
        //                 try:
        //                     size = encoding.read_int(f)
        //                 except EOFError:
        //                     break
        //                 blocks.append(size)
        //                 f.seek(size, os.SEEK_CUR)

        //             for size in reversed(blocks):
        //                 f.seek(-size, os.SEEK_CUR)
        //                 yield tracking.Tag.decode(f)
        //                 f.seek(-size - encoding.INT_SIZE, os.SEEK_CUR)

        //     except FileNotFoundError:
        //         raise graph.UnknownReferenceError(f"Unknown tag: {tag}")
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

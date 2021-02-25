use crate::{encoding, prelude::*, tracking, Result};

const TAG_EXT: &str = ".tag";

impl TagStorage for super::TarRepository {
    fn resolve_tag(&self, _tag_spec: &tracking::TagSpec) -> Result<tracking::Tag> {
        todo!()
        // try:
        //     stream = self.read_tag(tag)
        //     for _ in range(spec.version):
        //         next(stream)
        //     return next(stream)
        // except StopIteration:
        //     raise graph.UnknownReferenceError(
        //         f"tag or tag version does not exist {tag}"
        //     )
    }

    fn ls_tags(
        &self,
        _path: &relative_path::RelativePath,
    ) -> Result<Box<dyn Iterator<Item = String>>> {
        todo!()
        // filepath = os.path.join(self._prefix, path.lstrip(os.sep), "")
        // names = set()
        // try:
        //     for info in self._tar:
        //         if not info.name.startswith(filepath):
        //             continue
        //         name = info.name[len(filepath) :]
        //         name, *_ = name.split("/")
        //         if name.endswith(TAG_EXT):
        //             name = name[: -len(TAG_EXT)]
        //         names.add(name)
        // except FileNotFoundError:
        //     pass
        // return list(names)
    }

    fn find_tags(
        &self,
        _digest: &encoding::Digest,
    ) -> Box<dyn Iterator<Item = Result<tracking::TagSpec>>> {
        todo!()
        // for spec, stream in self.iter_tag_streams():
        //     i = -1
        //     for tag in stream:
        //         i += 1
        //         if tag.target != digest:
        //             continue
        //         yield tracking.build_tag_spec(name=spec.name, org=spec.org, version=i)
    }

    fn iter_tag_streams(
        &self,
    ) -> Box<
        dyn Iterator<Item = Result<(tracking::TagSpec, Box<dyn Iterator<Item = tracking::Tag>>)>>,
    > {
        todo!()
        // for info in self._tar:
        //     if not info.name.startswith(self._prefix):
        //         continue
        //     if not info.name.endswith(TAG_EXT):
        //         continue
        //     filepath = info.name[len(self._prefix) :]
        //     tag = filepath[: -len(TAG_EXT)]
        //     spec = tracking.TagSpec(tag)
        //     yield (spec, self.read_tag(tag))
    }

    fn read_tag(&self, _tag: &tracking::TagSpec) -> Result<Box<dyn Iterator<Item = tracking::Tag>>> {
        todo!()
        // spec = tracking.TagSpec(tag)
        // filepath = os.path.join(self._prefix, spec.path + TAG_EXT)
        // blocks = []
        // reader: BinaryIO
        // try:
        //     if spec.path in self._tag_cache:
        //         reader = io.BytesIO(self._tag_cache[spec.path])
        //     else:
        //         reader = self._tar.extractfile(filepath)  # type: ignore
        //         if reader is None:
        //             raise KeyError(tag)
        // except (KeyError, OSError):
        //     raise graph.UnknownReferenceError(f"Unknown tag: {tag}")

        // while True:
        //     try:
        //         size = encoding.read_int(cast(BinaryIO, reader))
        //     except EOFError:
        //         break
        //     blocks.append(size)
        //     reader.seek(size, os.SEEK_CUR)

        // for size in reversed(blocks):
        //     reader.seek(-size, os.SEEK_CUR)
        //     yield tracking.Tag.decode(cast(BinaryIO, reader))
        //     reader.seek(-size - encoding.INT_SIZE, os.SEEK_CUR)
    }

    fn push_raw_tag(&mut self, _tag: &tracking::Tag) -> Result<()> {
        todo!()
        // filepath = os.path.join(self._prefix, tag.path + TAG_EXT)
        // stream = io.BytesIO()
        // tag.encode(stream)
        // encoded_tag = stream.getvalue()
        // size = len(encoded_tag)

        // tag_file = io.BytesIO()
        // try:
        //     existing = self._tar.extractfile(filepath)
        //     if existing is not None:
        //         raise NotImplementedError("Cannot update tags in existing tar archive")
        // except (FileNotFoundError, KeyError, OSError):
        //     pass
        // encoding.write_int(tag_file, size)
        // tag_file.write(encoded_tag)
        // tag_file.seek(0, os.SEEK_SET)
        // info = tarfile.TarInfo(filepath)
        // info.size = size + encoding.INT_SIZE
        // self._tar.addfile(info, tag_file)
        // self._tag_cache[tag.path] = tag_file.getvalue()
    }

    fn remove_tag_stream(&mut self, _tag: &tracking::TagSpec) -> Result<()> {
        Err("Tag removal not supported in tar archives".into())
    }

    fn remove_tag(&mut self, _tag: &tracking::Tag) -> Result<()> {
        Err("Tag removal not supported in tar archives".into())
    }
}

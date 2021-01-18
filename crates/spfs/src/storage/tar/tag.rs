from typing import Iterator, Tuple, Optional, List, cast, BinaryIO, Dict
import os
import io
import tarfile

from ... import tracking, graph, encoding

_TAG_EXT = ".tag"


class TagStorage:
    def __init__(self, tar: tarfile.TarFile) -> None:

        self._tar = tar
        self._prefix = "tags/"
        self._tag_cache: Dict[str, bytes] = {}

    def has_tag(self, tag: &str) -> bool:
        """Return true if the given tag exists in this storage."""

        if tag in self._tag_cache:
            return True
        try:
            self.resolve_tag(tag)
        except graph.UnknownReferenceError:
            return False
        return True

    def ls_tags(self, path: &str) -> List[str]:
        """List tags and tag directories based on a tag path.

        For example, if the repo contains the following tags:
          spi/stable/my_tag
          spi/stable/other_tag
          spi/latest/my_tag
        Then ls_tags("spi") would return:
          stable
          latest
        """

        filepath = os.path.join(self._prefix, path.lstrip(os.sep), "")
        names = set()
        try:
            for info in self._tar:
                if not info.name.startswith(filepath):
                    continue
                name = info.name[len(filepath) :]
                name, *_ = name.split("/")
                if name.endswith(_TAG_EXT):
                    name = name[: -len(_TAG_EXT)]
                names.add(name)
        except FileNotFoundError:
            pass
        return list(names)

    def find_tags(self, digest: encoding.Digest) -> Iterator[tracking.TagSpec]:
        """Find tags that point to the given digest.

        This is an O(n) operation based on the number of all
        tag versions in each tag stream.
        """
        for spec, stream in self.iter_tag_streams():
            i = -1
            for tag in stream:
                i += 1
                if tag.target != digest:
                    continue
                yield tracking.build_tag_spec(name=spec.name, org=spec.org, version=i)

    def iter_tags(self) -> Iterator[Tuple[tracking.TagSpec, tracking.Tag]]:
        """Iterate through the available tags in this storage."""

        for spec, stream in self.iter_tag_streams():
            yield spec, next(stream)

    def iter_tag_streams(
        self,
    ) -> Iterator[Tuple[tracking.TagSpec, Iterator[tracking.Tag]]]:
        """Iterate through the available tags in this storage."""

        for info in self._tar:
            if not info.name.startswith(self._prefix):
                continue
            if not info.name.endswith(_TAG_EXT):
                continue
            filepath = info.name[len(self._prefix) :]
            tag = filepath[: -len(_TAG_EXT)]
            spec = tracking.TagSpec(tag)
            yield (spec, self.read_tag(tag))

    def read_tag(self, tag: &str) -> Iterator[tracking.Tag]:
        """Read the entire tag stream for the given tag.

        Raises:
            graph.UnknownObjectError: if the tag does not exist in the storage
        """

        spec = tracking.TagSpec(tag)
        filepath = os.path.join(self._prefix, spec.path + _TAG_EXT)
        blocks = []
        reader: BinaryIO
        try:
            if spec.path in self._tag_cache:
                reader = io.BytesIO(self._tag_cache[spec.path])
            else:
                reader = self._tar.extractfile(filepath)  # type: ignore
                if reader is None:
                    raise KeyError(tag)
        except (KeyError, OSError):
            raise graph.UnknownReferenceError(f"Unknown tag: {tag}")

        while True:
            try:
                size = encoding.read_int(cast(BinaryIO, reader))
            except EOFError:
                break
            blocks.append(size)
            reader.seek(size, os.SEEK_CUR)

        for size in reversed(blocks):
            reader.seek(-size, os.SEEK_CUR)
            yield tracking.Tag.decode(cast(BinaryIO, reader))
            reader.seek(-size - encoding.INT_SIZE, os.SEEK_CUR)

    def resolve_tag(self, tag: &str) -> tracking.Tag:

        spec = tracking.TagSpec(tag)
        try:
            stream = self.read_tag(tag)
            for _ in range(spec.version):
                next(stream)
            return next(stream)
        except StopIteration:
            raise graph.UnknownReferenceError(
                f"tag or tag version does not exist {tag}"
            )

    def push_tag(self, tag: &str, target: encoding.Digest) -> tracking.Tag:
        """Push the given tag onto the tag stream."""

        tag_spec = tracking.TagSpec(tag)
        parent: Option<tracking.Tag> = None
        try:
            parent = self.resolve_tag(tag)
        except ValueError:
            pass

        parent_ref = encoding.NULL_DIGEST
        if parent is not None:
            if parent.target == target:
                return parent
            parent_ref = parent.digest()

        new_tag = tracking.Tag(
            org=tag_spec.org, name=tag_spec.name, target=target, parent=parent_ref
        )
        self.push_raw_tag(new_tag)
        return new_tag

    def push_raw_tag(self, tag: tracking.Tag) -> None:
        """Push the given tag data to the tag stream, regardless of if it's valid."""

        filepath = os.path.join(self._prefix, tag.path + _TAG_EXT)
        stream = io.BytesIO()
        tag.encode(stream)
        encoded_tag = stream.getvalue()
        size = len(encoded_tag)

        tag_file = io.BytesIO()
        try:
            existing = self._tar.extractfile(filepath)
            if existing is not None:
                raise NotImplementedError("Cannot update tags in existing tar archive")
        except (FileNotFoundError, KeyError, OSError):
            pass
        encoding.write_int(tag_file, size)
        tag_file.write(encoded_tag)
        tag_file.seek(0, os.SEEK_SET)
        info = tarfile.TarInfo(filepath)
        info.size = size + encoding.INT_SIZE
        self._tar.addfile(info, tag_file)
        self._tag_cache[tag.path] = tag_file.getvalue()

    def remove_tag_stream(self, tag: &str) -> None:
        """Remove an entire tag and all related tag history."""

        raise NotImplementedError("Tag removal not supported in tar archives")

    def remove_tag(self, tag: tracking.Tag) -> None:
        """Remove the oldest stored instance of the given tag."""

        raise NotImplementedError("Tag removal not supported in tar archives")

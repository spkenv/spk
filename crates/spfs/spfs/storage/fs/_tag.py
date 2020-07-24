from typing import Iterator, Tuple, Optional, List
import os
import io
import errno
import contextlib

from ... import tracking, graph, encoding

from ._database import makedirs_with_perms

_TAG_EXT = ".tag"


class TagStorage:
    def __init__(self, root: str) -> None:

        self._root = os.path.abspath(root)

    def has_tag(self, tag: str) -> bool:
        """Return true if the given tag exists in this storage."""

        try:
            self.resolve_tag(tag)
        except graph.UnknownReferenceError:
            return False
        return True

    def ls_tags(self, path: str) -> List[str]:
        """List tags and tag directories based on a tag path.

        For example, if the repo contains the following tags:
          spi/stable/my_tag
          spi/stable/other_tag
          spi/latest/my_tag
        Then ls_tags("spi") would return:
          stable
          latest
        """

        filepath = os.path.join(self._root, path.lstrip(os.sep))
        try:
            entries = os.listdir(filepath)
        except (FileNotFoundError, NotADirectoryError):
            return []
        return sorted(
            list(
                set(
                    map(
                        lambda x: x
                        if not x.endswith(_TAG_EXT)
                        else x[: -len(_TAG_EXT)],
                        entries,
                    )
                )
            )
        )

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

        for root, _, files in os.walk(self._root):

            for filename in files:
                if not filename.endswith(_TAG_EXT):
                    continue
                filepath = os.path.join(root, filename)
                tag = os.path.relpath(filepath[: -len(_TAG_EXT)], self._root)
                spec = tracking.TagSpec(tag)
                yield (spec, self.read_tag(tag))

    def read_tag(self, tag: str) -> Iterator[tracking.Tag]:
        """Read the entire tag stream for the given tag.

        Raises:
            graph.UnknownObjectError: if the tag does not exist in the storage
        """

        spec = tracking.TagSpec(tag)
        filepath = os.path.join(self._root, spec.path + _TAG_EXT)
        try:
            blocks = []
            with open(filepath, "rb") as f:
                while True:
                    try:
                        size = encoding.read_int(f)
                    except EOFError:
                        break
                    blocks.append(size)
                    f.seek(size, os.SEEK_CUR)

                for size in reversed(blocks):
                    f.seek(-size, os.SEEK_CUR)
                    yield tracking.Tag.decode(f)
                    f.seek(-size - encoding.INT_SIZE, os.SEEK_CUR)

        except FileNotFoundError:
            raise graph.UnknownReferenceError(f"Unknown tag: {tag}")

    def resolve_tag(self, tag: str) -> tracking.Tag:

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

    def push_tag(self, tag: str, target: encoding.Digest) -> tracking.Tag:
        """Push the given tag onto the tag stream."""

        tag_spec = tracking.TagSpec(tag)
        parent: Optional[tracking.Tag] = None
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

        filepath = os.path.join(self._root, tag.path + _TAG_EXT)
        makedirs_with_perms(os.path.dirname(filepath), perms=0o777)

        stream = io.BytesIO()
        tag.encode(stream)
        encoded_tag = stream.getvalue()
        size = len(encoded_tag)

        with _tag_lock(filepath):
            tag_file_fd = os.open(
                filepath, os.O_CREAT | os.O_WRONLY | os.O_APPEND, mode=0o777
            )
            with os.fdopen(tag_file_fd, "ab") as tag_file:
                encoding.write_int(tag_file, size)
                tag_file.write(encoded_tag)

    def remove_tag_stream(self, tag: str) -> None:
        """Remove an entire tag and all related tag history."""

        tag_spec = tracking.TagSpec(tag)
        filepath = os.path.join(self._root, tag_spec.path + _TAG_EXT)
        makedirs_with_perms(os.path.dirname(filepath), perms=0o777)
        try:
            with _tag_lock(filepath):
                os.remove(filepath)
            os.rmdir(os.path.dirname(filepath))
        except FileNotFoundError:
            raise graph.UnknownReferenceError("Unknown tag: " + tag)
        except OSError as e:
            if e.errno != errno.ENOTEMPTY:
                raise

    def remove_tag(self, tag: tracking.Tag) -> None:
        """Remove the oldest stored instance of the given tag."""

        tag_spec = tracking.TagSpec(tag.path)
        filepath = os.path.join(self._root, tag_spec.path + _TAG_EXT)
        with _tag_lock(filepath):

            all_versions = reversed(list(self.read_tag(tag_spec)))
            backup_path = filepath + ".backup"
            os.rename(filepath, backup_path)
            try:
                for version in all_versions:
                    if version == tag:
                        continue
                    self.push_raw_tag(version)
            except Exception as e:
                try:
                    os.remove(filepath)
                except:
                    pass
                os.rename(backup_path, filepath)
                raise
            else:
                os.remove(backup_path)


_HAVE_LOCK = False


@contextlib.contextmanager
def _tag_lock(filepath: str) -> Iterator[None]:

    global _HAVE_LOCK
    if _HAVE_LOCK:
        yield
        return

    try:
        open(filepath + ".lock", "xb").close()
    except FileExistsError:
        raise RuntimeError(f"Tag already locked [{filepath}]")
    except Exception as e:
        raise RuntimeError(f"Cannot lock tag: {str(e)} [{filepath}]")
    try:
        _HAVE_LOCK = True
        yield
    finally:
        _HAVE_LOCK = False
        os.remove(filepath + ".lock")

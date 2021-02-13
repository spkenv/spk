from typing import Dict
import os

import structlog


from .... import tracking, storage, graph, encoding
from ... import fs
from . import fs_015

_LOGGER = structlog.get_logger("spfs.storage.fs.migrations")


def migrate(src_dir: str, dst_dir: str) -> None:

    src = fs_015.Repository(src_dir)
    dst = fs.FSRepository(dst_dir, create=True)
    Migration(src, dst).run()


class Migration:
    def __init__(self, src: fs_015.Repository, dst: fs.FSRepository) -> None:

        self.src: fs_015.Repository = src
        self.dst: fs.FSRepository = dst
        self.migrated_digests: Dict[str, encoding.Digest] = {}

    def run(self) -> None:

        _LOGGER.info("migrating tags...")
        for spec, stream in self.src.tags.iter_tag_streams():
            _LOGGER.info("migrating tag", tag=spec)
            # start at the beginning
            for old_tag in reversed(list(stream)):
                self.migrate_tag(old_tag)
            _LOGGER.info("tag migration complete", tag=spec)

        _LOGGER.info("migrating orphaned layers...")
        for digest in self.src.platforms.iter_digests():
            self.migrate_object(digest)

        _LOGGER.info("migrating orphaned platforms...")
        for digest in self.src.platforms.iter_digests():
            self.migrate_object(digest)

        _LOGGER.info("migrating orphaned blobs...")
        for digest in self.src.blobs.iter_digests():
            self.migrate_blob(digest)

        _LOGGER.info("checking integrity of new data...")
        for error in graph.check_database_integrity(self.dst.objects):
            raise error
        for obj_digest in self.dst.objects.iter_digests():
            obj = self.dst.objects.read_object(obj_digest)
            assert obj.digest() == obj_digest, "Created object has mismatched digest"
            if isinstance(obj, tracking.Manifest):
                for _, entry in obj.walk():
                    if entry.kind is tracking.EntryKind.BLOB:
                        assert self.dst.has_blob(entry.object)
                        assert self.dst.payloads.has_payload(entry.object)

        self.dst.set_last_migration("0.16.0")

    def migrate_tag(self, old_tag: fs_015.tracking.Tag) -> tracking.Tag:

        new_target = self.migrate_object(old_tag.target)
        if not old_tag.parent:
            parent = encoding.NULL_DIGEST
        else:
            parent = convert_digest(old_tag.parent)
        new_tag = tracking.Tag(
            org=old_tag.org,
            name=old_tag.name,
            target=new_target,
            parent=parent,
            user=old_tag.user,
            time=old_tag.time,
        )
        self.dst.tags.push_raw_tag(new_tag)
        return new_tag

    def migrate_object(self, old_digest: str) -> encoding.Digest:

        if old_digest in self.migrated_digests:
            return self.migrated_digests[old_digest]

        old_obj = self.src.read_object(old_digest)
        if isinstance(old_obj, fs_015.Platform):
            new_digest = self.migrate_platform(old_obj).digest()
        elif isinstance(old_obj, fs_015.Layer):
            new_digest = self.migrate_layer(old_obj).digest()
        else:
            raise NotImplementedError(f"migration: {old_obj}")

        self.migrated_digests[old_digest] = new_digest
        return new_digest

    def migrate_platform(self, old_platform: fs_015.Platform) -> storage.Platform:

        _LOGGER.info("migrating platform...", digest=old_platform.digest)
        new_stack = []
        for digest in old_platform.stack:
            new_digest = self.migrate_object(digest)
            new_stack.append(new_digest)

        new_platform = storage.Platform(new_stack)
        self.dst.objects.write_object(new_platform)
        _LOGGER.info("platform migration complete", digest=new_platform.digest())
        return new_platform

    def migrate_layer(self, old_layer: fs_015.Layer) -> storage.Layer:

        _LOGGER.info("migrating layer...", digest=old_layer.digest)
        new_manifest = self.migrate_manifest(old_layer.manifest)
        new_layer = storage.Layer(manifest=new_manifest.digest())
        self.dst.objects.write_object(new_layer)
        _LOGGER.info("layer migration complete", digest=new_layer.digest())
        return new_layer

    def migrate_manifest(
        self, old_manifest: fs_015.tracking.Manifest
    ) -> storage.Manifest:

        new_manifest = tracking.Manifest()
        for path, old_entry in old_manifest.walk():
            if old_entry.kind is fs_015.tracking.EntryKind.BLOB:
                blob = self.migrate_blob(old_entry.object)
                size = blob.size
            elif old_entry.kind is fs_015.tracking.EntryKind.TREE:
                tree = old_manifest._trees[old_entry.object]
                size = len(tree)
            else:
                size = 0
            entry = new_manifest.mkfile(path)
            entry.object = convert_digest(old_entry.object)
            entry.kind = tracking.EntryKind(old_entry.kind.value)
            entry.mode = old_entry.mode
            entry.size = size

        final = storage.Manifest(new_manifest)
        self.dst.objects.write_object(final)
        return final

    def migrate_blob(self, old_digest: str) -> storage.Blob:

        if old_digest in self.migrated_digests:
            return self.dst.read_blob(self.migrated_digests[old_digest])

        blob_path = self.src.blobs.build_digest_path(old_digest)
        stat_result = os.lstat(blob_path)
        size = stat_result.st_size
        with open(blob_path, "rb") as reader:
            new_digest = self.dst.payloads.write_payload(reader)
            assert new_digest == convert_digest(
                old_digest
            ), "copied blob had different digest than before"
        blob = storage.Blob(payload=new_digest, size=size)
        self.dst.objects.write_object(blob)
        self.migrated_digests[old_digest] = blob.payload
        return blob


def convert_digest(old_digest: str) -> encoding.Digest:

    if not old_digest:
        return encoding.NULL_DIGEST
    old_bytes = bytes.fromhex(old_digest)
    return encoding.Digest(old_bytes)

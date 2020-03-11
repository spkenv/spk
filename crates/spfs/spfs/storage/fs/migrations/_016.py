import os

from .... import tracking, storage, graph, encoding
from ... import fs
from . import fs_015


def migrate(src_dir: str, dst_dir: str) -> None:

    src = fs_015.Repository(src_dir)
    dst = fs.FSRepository(dst_dir)

    for spec, stream in src.tags.iter_tag_streams():
        # start at the beginning
        for old_tag in reversed(list(stream)):
            migrate_tag(old_tag, src, dst)

    dst.set_last_migration("0.16.0")


def migrate_tag(
    old_tag: fs_015.tracking.Tag, src: fs_015.Repository, dst: fs.FSRepository
) -> tracking.Tag:

    new_target = migrate_object(old_tag.target, src, dst)
    if not old_tag.parent:
        parent = encoding.NULL_DIGEST
    else:
        parent = migrate_digest(old_tag.parent)
    new_tag = tracking.Tag(
        org=old_tag.org,
        name=old_tag.name,
        target=new_target,
        parent=parent,
        user=old_tag.user,
        time=old_tag.time,
    )
    dst.tags.push_raw_tag(new_tag)
    return new_tag


def migrate_object(
    old_digest: str, src: fs_015.Repository, dst: fs.FSRepository
) -> encoding.Digest:

    old_obj = src.read_object(old_digest)
    if isinstance(old_obj, fs_015.Platform):
        return migrate_platform(old_obj, src, dst).digest()
    elif isinstance(old_obj, fs_015.Layer):
        return migrate_layer(old_obj, src, dst).digest()
    else:
        raise NotImplementedError(f"migration: {old_obj}")


def migrate_platform(
    old_platform: fs_015.Platform, src: fs_015.Repository, dst: fs.FSRepository
) -> storage.Platform:

    new_stack = []
    for digest in old_platform.stack:
        new_digest = migrate_object(digest, src, dst)
        new_stack.append(new_digest)

    new_platform = storage.Platform(new_stack)
    dst.objects.write_object(new_platform)
    return new_platform


def migrate_layer(
    old_layer: fs_015.Layer, src: fs_015.Repository, dst: fs.FSRepository
) -> storage.Layer:

    new_manifest = migrate_manifest(old_layer.manifest, src, dst)
    new_layer = storage.Layer(manifest=new_manifest.digest())
    dst.objects.write_object(new_layer)
    return new_layer


def migrate_manifest(
    old_manifest: fs_015.tracking.Manifest, src: fs_015.Repository, dst: fs.FSRepository
) -> tracking.Manifest:

    new_manifest_builder = tracking.ManifestBuilder("/")
    for path, old_entry in old_manifest.walk():
        if old_entry.kind is fs_015.tracking.EntryKind.BLOB:
            blob_path = src.blobs.build_digest_path(old_entry.object)
            stat_result = os.stat(blob_path)
            size = stat_result.st_size
            blob = storage.Blob(payload=migrate_digest(old_entry.object), size=size)
            dst.objects.write_object(blob)
            with open(blob_path, "rb") as reader:
                new_digest = dst.payloads.write_payload(reader)
                assert new_digest == blob.payload, "unoh"
        else:
            tree = old_manifest._trees[old_entry.object]
            size = len(tree)
        new_entry = tracking.Entry(
            object=migrate_digest(old_entry.object),
            kind=tracking.EntryKind(old_entry.kind.value),
            mode=old_entry.mode,
            size=size,
            name=old_entry.name,
        )
        new_manifest_builder.add_entry(path, new_entry)
    manifest = new_manifest_builder.finalize()
    dst.objects.write_object(manifest)
    return manifest


def migrate_digest(old_digest: str) -> encoding.Digest:

    old_bytes = bytes.fromhex(old_digest)
    return encoding.Digest(old_bytes)

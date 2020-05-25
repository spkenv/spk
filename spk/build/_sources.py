import os

import spfs

from .. import api, storage
from ._build import build_dir_path


def make_source_package(spec: api.Spec) -> api.Ident:

    spfs_repo = spfs.get_config().get_repository()
    repo = storage.SpFSRepository(spfs_repo)
    layer = collect_and_commit_sources(spec)
    repo.publish_source_package(spec.pkg, layer.digest())
    return spec.pkg.with_build(api.SRC)


def collect_and_commit_sources(spec: api.Spec) -> spfs.storage.Layer:

    pkg = spec.pkg.with_build(api.SRC)

    runtime = spfs.active_runtime()

    source_dir = data_path(pkg)
    collect_sources(spec, source_dir)

    return spfs.commit_layer(runtime)


def collect_sources(spec: api.Spec, source_dir: str) -> None:
    os.makedirs(source_dir)

    for source in spec.sources:

        target_dir = source_dir
        subdir = source.subdir()
        if subdir:
            target_dir = os.path.join(source_dir, subdir.lstrip("/"))
            os.makedirs(target_dir, exist_ok=True)

        source.collect(target_dir)


def data_path(pkg: api.Ident) -> str:
    return f"/spfs/spk/pkg/{pkg}/"

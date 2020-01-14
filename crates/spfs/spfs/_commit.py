from . import storage, runtime
from ._config import get_config


def commit_layer(runtime: runtime.Runtime) -> storage.Layer:
    """Commit the working file changes of a runtime to a new layer."""

    config = get_config()
    repo = config.get_repository()
    manifest = repo.blobs.commit_dir(runtime.upper_dir)
    return repo.layers.commit_manifest(manifest)


def commit_platform(runtime: runtime.Runtime) -> storage.Platform:
    """Commit the full layer stack and working files to a new platform."""

    config = get_config()
    repo = config.get_repository()
    top_layer = commit_layer(runtime)
    runtime.push_digest(top_layer.digest)
    return repo.platforms.commit_stack(runtime.get_stack())

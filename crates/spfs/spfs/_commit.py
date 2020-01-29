from . import storage, runtime
from ._config import get_config


class NothingToCommitError(ValueError):
    """Denotes that a layer or manifest either is or would be empty."""

    def __init__(self, message: str) -> None:
        super(NothingToCommitError, self).__init__(f"Nothing to commit, " + message)


def commit_layer(runtime: runtime.Runtime) -> storage.Layer:
    """Commit the working file changes of a runtime to a new layer."""

    config = get_config()
    repo = config.get_repository()
    manifest = repo.blobs.commit_dir(runtime.upper_dir)
    if manifest.is_empty():
        raise NothingToCommitError("layer would be empty")
    return repo.layers.commit_manifest(manifest)


def commit_platform(runtime: runtime.Runtime) -> storage.Platform:
    """Commit the full layer stack and working files to a new platform."""

    config = get_config()
    repo = config.get_repository()

    try:
        top_layer = commit_layer(runtime)
    except NothingToCommitError:
        pass
    else:
        runtime.push_digest(top_layer.digest)

    stack = runtime.get_stack()
    if len(stack) == 0:
        raise NothingToCommitError("platform would be empty")

    return repo.platforms.commit_stack(stack)

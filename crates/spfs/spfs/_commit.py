from . import storage, runtime
from ._runtime import remount_runtime
from ._config import get_config


class NothingToCommitError(ValueError):
    """Denotes that a layer or manifest either is or would be empty."""

    def __init__(self, message: str) -> None:
        super(NothingToCommitError, self).__init__(f"Nothing to commit, " + message)


def commit_layer(runtime: runtime.Runtime) -> storage.Layer:
    """Commit the working file changes of a runtime to a new layer."""

    config = get_config()
    repo = config.get_repository()
    manifest = repo.commit_dir(runtime.upper_dir)
    if manifest.is_empty():
        raise NothingToCommitError("layer would be empty")
    layer = repo.create_layer(storage.Manifest(manifest))
    runtime.push_digest(layer.digest())
    runtime.set_editable(False)
    remount_runtime(runtime)
    return layer


def commit_platform(runtime: runtime.Runtime) -> storage.Platform:
    """Commit the full layer stack and working files to a new platform."""

    config = get_config()
    repo = config.get_repository()

    try:
        top_layer = commit_layer(runtime)
    except NothingToCommitError:
        pass

    stack = runtime.get_stack()
    if len(stack) == 0:
        raise NothingToCommitError("platform would be empty")

    return repo.create_platform(stack)

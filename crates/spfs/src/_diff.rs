from typing import List

import structlog

from . import tracking
from ._resolve import compute_manifest
from ._runtime import compute_runtime_manifest, active_runtime
from ._config import get_config


_LOGGER = structlog.get_logger("spfs")


def diff(base: str = None, top: str = None) -> List[tracking.Diff]:
    """Return the changes going from 'base' to 'top'.

    Args:
        base (Optional[str]): The tag or id to use as the base of the computed diff
            (defaults to the current runtime)
        top (Optional[str]): The tag or id to diff the base against
            (defaults to the contents of /spfs)",
    """

    config = get_config()
    repo = config.get_repository()

    if base is None:
        _LOGGER.debug("computing runtime manifest as base")
        runtime = active_runtime()
        base_manifest = compute_runtime_manifest(runtime)
    else:
        _LOGGER.debug("computing base manifest", ref=base)
        base_manifest = compute_manifest(base)

    if top is None:
        _LOGGER.debug("computing manifest for /spfs")
        top_manifest = tracking.compute_manifest("/spfs")
    else:
        _LOGGER.debug("computing top manifest", ref=top)
        top_manifest = compute_manifest(top)

    _LOGGER.debug("computing diffs")
    return tracking.compute_diff(base_manifest, top_manifest)

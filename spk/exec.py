import spfs
import structlog

from . import solve, io, storage

_LOGGER = structlog.get_logger("spk.exec")


def setup_current_runtime(solution: solve.Solution) -> None:
    """Modify the active spfs runtime to include exactly the packges in the given solution."""

    runtime = spfs.active_runtime()
    configure_runtime(runtime, solution)
    spfs.remount_runtime(runtime)


def create_runtime(solution: solve.Solution) -> spfs.runtime.Runtime:
    """Create a new runtime properly configured with the given solve."""

    runtime = spfs.get_config().get_runtime_storage().create_runtime()
    configure_runtime(runtime, solution)
    return runtime


def configure_runtime(runtime: spfs.runtime.Runtime, solution: solve.Solution) -> None:
    """Pull the necessary layers and setup the given runtime to have all solution packages."""

    local_repo = storage.local_repository()
    for _, spec, repo in solution.items():

        try:
            digest = repo.get_package(spec.pkg)
        except FileNotFoundError:
            raise RuntimeError("Resolved package disappeared, please try again")

        if isinstance(repo, storage.SpFSRepository):
            _LOGGER.info("collecting", pkg=io.format_ident(spec.pkg))
            spfs.sync_ref(
                str(digest), repo.as_spfs_repo(), local_repo.as_spfs_repo(),
            )

        runtime.push_digest(digest)

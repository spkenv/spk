import spfs
import structlog

from . import solve, storage, io, build, api

_LOGGER = structlog.get_logger("spk.exec")


def build_required_packages(solution: solve.Solution) -> solve.Solution:
    """Build any packages in the given solution that need building.

    Returns:
      solve.Solution: a new solution of only binary packages
    """

    local_repo = storage.local_repository()
    repos = solution.repositories()
    options = solution.options()
    compiled_solution = solve.Solution(options)
    for item in solution.items():
        if not item.is_source_build():
            compiled_solution.add(*item)
            continue

        req, spec, source = item
        _LOGGER.info(
            f"Building: {io.format_ident(spec.pkg)} for {io.format_options(options)}"
        )
        spec = (
            build.BinaryPackageBuilder.from_spec(source)  # type: ignore
            .with_repositories(repos)
            .with_options(options)
            .build()
        )
        compiled_solution.add(req, spec, local_repo)
    return compiled_solution


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
    for _, spec, source in solution.items():

        if isinstance(source, api.Spec):
            if source.pkg == spec.pkg.with_build(None):
                raise ValueError(
                    f"Solution includes package that needs building: {spec.pkg}"
                )

        if not isinstance(source, storage.Repository):
            continue
        repo = source

        try:
            digest = repo.get_package(spec.pkg)
        except FileNotFoundError:
            raise RuntimeError("Resolved package disappeared, please try again")

        if isinstance(repo, storage.SpFSRepository):
            if not local_repo.as_spfs_repo().objects.has_object(digest):
                _LOGGER.info("collecting " + io.format_ident(spec.pkg))
            spfs.sync_ref(str(digest), repo.as_spfs_repo(), local_repo.as_spfs_repo())

        runtime.push_digest(digest)

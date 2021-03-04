from typing import List
import spkrs
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

    runtime = spkrs.active_runtime()
    stack = resolve_runtime_layers(solution)
    spkrs.reconfigure_runtime(stack=stack)


def resolve_runtime_layers(solution: solve.Solution) -> List[spkrs.Digest]:
    """Pull and list the necessary layers to have all solution packages."""

    local_repo = storage.local_repository()
    stack = []
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
            if not repo.rs.has_digest(digest):
                _LOGGER.info("collecting " + io.format_ident(spec.pkg))
            repo.rs.localize_digest(digest)

        stack.append(digest)

    return stack

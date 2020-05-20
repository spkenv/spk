from typing import List, Union, Tuple
import os
import subprocess
import shlex

import structlog
import spfs

from .. import api, graph, storage
from ._env import expand_vars

_LOGGER = structlog.get_logger("spk.build")


class BuildError(Exception):
    pass


def build_variants(
    spec: api.Spec,
) -> List[Tuple[api.Spec, api.OptionMap, spfs.tracking.Tag]]:
    """Build all of the default variants defined for the given spec."""

    variants = spec.build.variants
    if not variants:
        _LOGGER.debug("generating default variant")
        variants.append(api.OptionMap())

    results = []
    for variant_options in variants:
        build_options = api.host_options()
        build_options.update(variant_options)

        tag = build(spec, build_options)
        results.append((spec, build_options, tag))

    return results


def build(
    spec: api.Spec, options: api.OptionMap = api.OptionMap()
) -> spfs.tracking.Tag:
    """Execute the build process for a package spec with the given build options."""

    from .._solver import Solver  # FIXME: cyclical import

    options = spec.resolve_all_options(options)
    release = options.digest()
    _LOGGER.info(f"building: {spec.pkg}/{release}")
    _LOGGER.debug("complete_inputs", **options)

    # TODO: Not sure that this should be a new solver, and creates
    #       the circular dependency... But the build environment
    #       does need to be created if it's going to run...
    #       solver -> nodes -> build -> solver
    solver = Solver(options)
    for opt in spec.opts:
        if not isinstance(opt, api.Spec):
            continue
        if opt.pkg.name in options:
            opt.pkg.version = api.parse_version(options[opt.pkg.name])
        # TODO: deal with release information??
        # TODO: what about other spec options that might have been given?
        solver.add_request(opt.pkg)
    for dep in spec.depends:
        # TODO: What about other spec info (not pkg field only)
        solver.add_request(dep.pkg)
    env = solver.solve()

    # TODO: this should be cleaner / configured?
    spfs_repo = spfs.get_config().get_repository()
    repo = storage.SpFSRepository(spfs_repo)

    stack = []
    for handle in env.packages():

        # TODO: pull if needed
        _LOGGER.info(f" - {handle}")
        # TODO: should theis be handled by something else?
        obj = spfs_repo.read_ref(handle)  # FIXME: better
        stack.append(obj.digest())

    layer = run_and_commit_build(spec.pkg, spec.build.script, *stack)
    return repo.publish_package(spec.pkg, options, layer.digest())


def run_and_commit_build(
    pkg: api.Ident, script: str, *stack: spfs.encoding.Digest
) -> spfs.storage.Layer:

    runtime = spfs.active_runtime()
    runtime.reset()
    for layer in stack:
        runtime.push_digest(layer)
        # TODO: pull if needed

    runtime.set_editable(True)
    spfs.remount_runtime(runtime)

    build_script = f"/spfs/var/spk/build/{pkg}/build.sh"
    os.makedirs(os.path.dirname(build_script), exist_ok=True)
    with open(build_script, "w+") as f:
        f.write(script)
    source_dir = f"/spfs/var/run/spk/src/{pkg}"
    execute_build(source_dir, build_script)

    diffs = spfs.diff()
    validate_changeset(diffs)

    return spfs.commit_layer(runtime)


def execute_build(source_dir: str, build_script: str) -> None:

    cmd = spfs.build_shell_initialized_command("bash", "-ex", build_script)
    subprocess.check_call(cmd, cwd=source_dir)


def validate_changeset(diffs: List[spfs.tracking.Diff]) -> None:

    diffs = list(
        filter(lambda diff: diff.mode is not spfs.tracking.DiffMode.unchanged, diffs)
    )

    if not diffs:
        raise BuildError("Build process created no files under /spfs")

    for diff in diffs:
        _LOGGER.debug(diff)
        if diff.mode is not spfs.tracking.DiffMode.added:
            raise BuildError(f"Existing file was modified: /spfs{diff.path}")

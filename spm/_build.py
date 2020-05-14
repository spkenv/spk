from typing import List, Union
import os
import subprocess
import shlex

import structlog
import spfs

from . import api, graph, storage
from ._handle import BinaryPackageHandle
from ._env import expand_vars

_LOGGER = structlog.get_logger("spm.build")


def build_variants(spec: api.Spec) -> List[BinaryPackageHandle]:
    """Build all of the default variants defined for the given spec."""

    variants = spec.build.variants
    if not variants:
        _LOGGER.debug("generating default variant")
        variants.append(api.OptionMap())

    handles = []
    for variant_options in variants:
        build_options = api.host_options()
        build_options.update(variant_options)

        handle = build(spec, build_options)
        handles.append(handle)

    return handles


def build(
    spec: api.Spec, options: api.OptionMap = api.OptionMap()
) -> BinaryPackageHandle:
    """Execute the build process for a package spec with the given build options."""

    from ._solver import Solver  # FIXME: cyclical import

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
    tag = repo.publish_package(spec.pkg, options, layer.digest())

    return BinaryPackageHandle(spec, tag)


def run_and_commit_build(
    pkg: api.Ident, script: str, *stack: spfs.encoding.Digest
) -> spfs.storage.Layer:

    runtime = spfs.active_runtime()
    runtime.reset()
    repo = spfs.get_config().get_repository()
    for layer in stack:
        runtime.push_digest(layer)
        # TODO: pull if needed

    runtime.set_editable(True)
    spfs.remount_runtime(runtime)

    build_script = f"/spfs/var/spm/build/{pkg}/build.sh"
    os.makedirs(os.path.dirname(build_script), exist_ok=True)
    with open(build_script, "w+") as f:
        f.write(script)
    cmd = spfs.build_shell_initialized_command("bash", "-ex", build_script)
    subprocess.check_call(cmd)

    diffs = spfs.diff()
    for diff in diffs:
        _LOGGER.debug(diff)
        if diff.mode is not spfs.tracking.DiffMode.added:
            _LOGGER.warning(f"Underlying file was modified: /spfs{diff.path}")

    # TODO: check that there are file changes
    # TODO: check that there are no overwritten files

    return spfs.commit_layer(runtime)

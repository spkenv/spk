from typing import List, Union
import os
import subprocess
import shlex

import structlog
import spfs

from ._spec import Spec, VarSpec
from ._version import parse_version
from ._option_map import OptionMap
from ._handle import Handle, SpFSHandle
from ._solver import Solver
from ._env import expand_vars

_LOGGER = structlog.get_logger("spm.build")


def build_variants(spec: Spec) -> List[Handle]:
    """Build all of the default variants defined for the given spec."""

    variants = spec.build.variants
    if not variants:
        _LOGGER.debug("generating default variant")
        variants.append(OptionMap())

    handles = []
    for variant_options in variants:
        build_options = spec.build.options.copy()
        build_options.update(variant_options)
        build_options = spec.resolve_all_options(build_options)

        handle = build(spec, build_options)
        handles.append(handle)

    return handles


def build(spec: Spec, options: OptionMap) -> Handle:
    """Execute the build process for a package spec with the given build options."""

    release = options.digest()
    _LOGGER.info(f"building: {spec.pkg}/{release}")
    _LOGGER.debug("complete_inputs", **options)

    solver = Solver(options)
    for opt in spec.opts:
        if not isinstance(opt, Spec):
            continue
        if opt.pkg.name in options:
            opt.pkg.version = parse_version(options[opt.pkg.name])
        # TODO: deal with release information??
        solver.add_request(opt)
    for dep in spec.depends:
        solver.add_request(dep)
    packages = solver.solve()

    runtime = spfs.active_runtime()
    runtime.reset()
    repo = spfs.get_config().get_repository()
    _LOGGER.info(f"Using:")
    for handle in packages:
        # TODO: pull if needed
        _LOGGER.info(f" - {handle.url()}")
        obj = repo.read_ref(handle.url()[len("spfs:/") :])  # FIXME: better
        runtime.push_digest(obj.digest())

    runtime.set_editable(True)
    spfs.remount_runtime(runtime)

    build_script = f"/spfs/var/spm/build/{spec.pkg}/build.sh"
    os.makedirs(os.path.dirname(build_script), exist_ok=True)
    with open(build_script, "w+") as f:
        f.write(spec.build.script)
    cmd = spfs.build_shell_initialized_command("bash", "-ex", build_script)
    subprocess.check_call(cmd)

    diffs = spfs.diff()
    for diff in diffs:
        _LOGGER.debug(diff)
        if diff.mode is not spfs.tracking.DiffMode.added:
            _LOGGER.warning(f"Underlying file was modified: /spfs{diff.path}")

    # TODO: check that there are file changes
    # TODO: check that there are no overwritten files

    layer = spfs.commit_layer(runtime)
    tag = f"spm/pkg/{spec.pkg}"
    spfs.get_config().get_repository().tags.push_tag(tag, layer.digest())

    return SpFSHandle(spec, tag)
